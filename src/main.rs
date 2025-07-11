#![no_std]
#![no_main]

use esp_hal::clock::CpuClock;
use esp_hal::rmt::{Rmt, TxChannelCreator};
use esp_hal::rng::Rng;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;

// Standard library imports
extern crate alloc;
use alloc::vec::Vec;

// WiFi imports
use esp_wifi::wifi;

// Embassy-net imports
use embassy_net::{Config, Stack, StackResources};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use esp_hal_embassy::Executor;
use static_cell::StaticCell;

// LED control imports - using direct RMT for RGBW

// Import our library modules
use board_rs::config;
use board_rs::state_machine::{Action, SystemEvent, SystemStateMachine};

// Add app descriptor for espflash compatibility
esp_bootloader_esp_idf::esp_app_desc!();

// Static cells for embassy components
static WIFI_INIT_CELL: StaticCell<esp_wifi::EspWifiController<'static>> = StaticCell::new();
static STACK_CELL: StaticCell<Stack<'static>> = StaticCell::new();
static WIFI_MANAGER_CELL: StaticCell<board_rs::wifi::WiFiManager<'static>> = StaticCell::new();
// Use the concrete channel type
type ConcreteChannel = esp_hal::rmt::Channel<esp_hal::Blocking, 0>;
type LedControllerType = board_rs::led_control::UniversalDriverBoard<ConcreteChannel>;
static LED_CONTROLLER_CELL: StaticCell<
    embassy_sync::mutex::Mutex<
        embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
        LedControllerType,
    >,
> = StaticCell::new();

// Static cell for system state machine
static STATE_MACHINE_CELL: StaticCell<
    embassy_sync::mutex::Mutex<
        embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
        SystemStateMachine,
    >,
> = StaticCell::new();

// Static executor for embassy tasks
static EXECUTOR: StaticCell<Executor> = StaticCell::new();

// Static cells for LED communication channels
static LED_STATUS_SENDER_CELL: StaticCell<
    embassy_sync::channel::Sender<
        'static,
        CriticalSectionRawMutex,
        board_rs::led_control::LedStatus,
        8,
    >,
> = StaticCell::new();
static LED_DATA_SENDER_CELL: StaticCell<
    embassy_sync::channel::Sender<
        'static,
        CriticalSectionRawMutex,
        board_rs::led_control::LedData,
        4,
    >,
> = StaticCell::new();
static LED_MODE_SENDER_CELL: StaticCell<
    embassy_sync::channel::Sender<
        'static,
        CriticalSectionRawMutex,
        board_rs::led_control::LedMode,
        2,
    >,
> = StaticCell::new();

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// Embassy task to run the network stack
#[embassy_executor::task]
async fn net_task(
    mut runner: embassy_net::Runner<'static, esp_wifi::wifi::WifiDevice<'static>>,
) -> ! {
    runner.run().await
}

// State machine driven main application task
#[embassy_executor::task]
async fn state_machine_task(
    wifi_manager: &'static mut board_rs::wifi::WiFiManager<'static>,
    _stack: &'static Stack<'static>,
    led_status_sender: &'static embassy_sync::channel::Sender<
        'static,
        CriticalSectionRawMutex,
        board_rs::led_control::LedStatus,
        8,
    >,
    state_machine: &'static Mutex<CriticalSectionRawMutex, SystemStateMachine>,
) -> ! {
    use board_rs::state_machine::SystemState;
    use embassy_time::{Duration, Timer};

    // Initialize state machine
    {
        let mut sm = state_machine.lock().await;
        sm.handle_event(SystemEvent::SystemStarted);
    }

    // Track last logged error to avoid repetition
    let mut last_logged_error: Option<SystemState> = None;
    // Track last sent LED status to avoid repetition
    let mut last_led_status: Option<board_rs::led_control::LedStatus> = None;

    // Main state machine loop
    loop {
        // Get current state and actions
        let (_current_state, actions) = {
            let mut sm = state_machine.lock().await;
            let actions = sm.update();
            (sm.get_current_state(), actions)
        };

        // Collect events to send to state machine to reduce lock contention
        let mut events_to_send = Vec::new();

        // Execute actions based on state machine output
        for action in actions {
            match action {
                Action::UpdateLEDStatus(status) => {
                    // Only send status update if it's different from the last one
                    if last_led_status != Some(status) {
                        let _ = led_status_sender.try_send(status);
                        last_led_status = Some(status);
                    }
                }
                Action::StartWiFiConnection => {
                    match wifi_manager
                        .connect(config::WIFI_SSID, config::WIFI_PASSWORD)
                        .await
                    {
                        Ok(_) => {
                            println!("[WIFI] Connected");
                            events_to_send.push(SystemEvent::WiFiConnected);
                        }
                        Err(_) => {
                            events_to_send.push(SystemEvent::WiFiConnectionFailed);
                        }
                    }
                }
                Action::StartDHCPRequest => {
                    if let Some(ip) = wifi_manager.get_ip_address() {
                        println!("[DHCP] IP: {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);
                        events_to_send.push(SystemEvent::DHCPSuccess);
                    } else {
                        // Continue waiting for DHCP
                        Timer::after(Duration::from_millis(1000)).await;
                    }
                }
                Action::StartNetworkServices => {
                    events_to_send.push(SystemEvent::UDPServerStarted);
                }
                Action::StartUDPServer => {
                    events_to_send.push(SystemEvent::UDPServerStarted);
                }
                Action::StartMDNSService => {
                    // mDNS service is handled by the dedicated mdns_server_task
                    // Just mark the action as processed
                    println!("[MDNS] Service start requested - handled by mdns_server_task");
                }
                Action::MonitorConnection => {
                    // Monitor WiFi connection without triggering state machine events
                    // to avoid deadlock. Events will be handled in the next loop iteration.
                    let _ = wifi_manager.monitor_connection();
                }
                Action::SystemRecover => {
                    println!("[STATE] Initiating system recovery...");
                    events_to_send.push(SystemEvent::RecoveryRequested);
                }
                Action::LogError(error_state) => {
                    // Only log if this is a new error state
                    if last_logged_error != Some(error_state) {
                        println!("[STATE] Error logged: {:?}", error_state);
                        last_logged_error = Some(error_state);
                    }
                }
                _ => {
                    // Handle other actions as needed
                }
            }
        }

        // Send all collected events in a single lock acquisition
        if !events_to_send.is_empty() {
            let mut sm = state_machine.lock().await;
            for event in events_to_send {
                sm.handle_event(event);
            }
        }

        // LED display is now handled by the dedicated LED task at 30fps
        // No need to update LED display here anymore

        // Increase delay to reduce CPU usage and lock contention
        Timer::after(Duration::from_millis(200)).await;
    }
}

/// UDP server background task
#[embassy_executor::task]
async fn udp_server_task(
    stack: &'static Stack<'static>,
    led_data_sender: &'static embassy_sync::channel::Sender<
        'static,
        CriticalSectionRawMutex,
        board_rs::led_control::LedData,
        4,
    >,
    state_machine: &'static Mutex<CriticalSectionRawMutex, SystemStateMachine>,
) {
    use board_rs::udp_server::UdpServer;

    // Create UDP server
    let mut udp_server = UdpServer::new();
    udp_server.set_stack(stack);

    // Bind to the configured port
    match udp_server.bind(config::UDP_PORT) {
        Ok(_) => {
            println!("[UDP] Listening on port {}", config::UDP_PORT);

            // Start listening for packets
            match udp_server
                .start_listening(led_data_sender, state_machine)
                .await
            {
                Ok(_) => {
                    println!("[UDP] Server stopped");
                }
                Err(e) => {
                    println!("[UDP] Error: {:?}", e);
                }
            }
        }
        Err(e) => {
            println!("[UDP] Bind failed: {:?}", e);
        }
    }
}

/// mDNS server background task
#[embassy_executor::task]
async fn mdns_server_task(stack: &'static Stack<'static>) {
    use embassy_net::udp::UdpSocket;
    use embassy_net::{IpAddress, IpEndpoint};
    use embassy_time::{Duration, Timer};

    // Wait for network to be ready
    stack.wait_config_up().await;
    Timer::after(Duration::from_secs(2)).await;

    // Get our IP address
    let config = stack.config_v4();
    if let Some(config) = config {
        let our_ip = config.address.address();

        // Join mDNS multicast group (224.0.0.251)
        let mdns_multicast_addr = IpAddress::v4(224, 0, 0, 251);
        match stack.join_multicast_group(mdns_multicast_addr) {
            Ok(_) => println!("[MDNS] Joined multicast group 224.0.0.251"),
            Err(e) => {
                println!("[MDNS] Failed to join multicast group: {:?}", e);
                return;
            }
        }

        // Create UDP socket for mDNS
        let mut rx_buffer = [0; 1500];
        let mut tx_buffer = [0; 1500];
        let mut rx_meta = [embassy_net::udp::PacketMetadata::EMPTY; 8];
        let mut tx_meta = [embassy_net::udp::PacketMetadata::EMPTY; 8];
        let mut socket = UdpSocket::new(
            *stack,
            &mut rx_meta,
            &mut rx_buffer,
            &mut tx_meta,
            &mut tx_buffer,
        );

        // Bind to mDNS port (5353)
        match socket.bind(5353) {
            Ok(_) => {
                println!("[MDNS] Bound to port 5353");

                // Create mDNS response packet
                let response = create_mdns_response(our_ip, board_rs::config::UDP_PORT);
                let mdns_multicast = IpEndpoint::new(mdns_multicast_addr, 5353);

                // Send initial mDNS announcement
                match socket.send_to(&response, mdns_multicast).await {
                    Ok(_) => println!("[MDNS] Initial announcement sent"),
                    Err(e) => println!("[MDNS] Failed to send initial announcement: {:?}", e),
                }

                let mut last_announcement = embassy_time::Instant::now();

                // Start mDNS responder loop
                loop {
                    let mut buffer = [0u8; 1500];

                    // Send periodic announcements every 30 seconds
                    let now = embassy_time::Instant::now();
                    if now.duration_since(last_announcement) > Duration::from_secs(30) {
                        // Silent periodic announcement
                        match socket.send_to(&response, mdns_multicast).await {
                            Ok(_) => {}  // Silent success
                            Err(_) => {} // Silent error - mDNS is not critical
                        }
                        last_announcement = now;
                    }

                    // Listen for mDNS queries with timeout
                    match embassy_time::with_timeout(
                        Duration::from_millis(1000),
                        socket.recv_from(&mut buffer),
                    )
                    .await
                    {
                        Ok(Ok((len, endpoint))) => {
                            println!("[MDNS] Received query from {:?} ({} bytes)", endpoint, len);

                            // Simple mDNS query detection and response
                            if len > 12 {
                                // Check if this is a query (QR bit = 0)
                                if (buffer[2] & 0x80) == 0 {
                                    println!("[MDNS] Processing mDNS query");

                                    // Create response with matching transaction ID
                                    let mut query_response = response.clone();
                                    query_response[0] = buffer[0]; // Copy transaction ID
                                    query_response[1] = buffer[1];

                                    // Send mDNS response to multicast address
                                    match socket.send_to(&query_response, mdns_multicast).await {
                                        Ok(_) => println!("[MDNS] Sent multicast response"),
                                        Err(e) => println!(
                                            "[MDNS] Failed to send multicast response: {:?}",
                                            e
                                        ),
                                    }

                                    // Also send unicast response for compatibility
                                    match socket.send_to(&query_response, endpoint).await {
                                        Ok(_) => println!(
                                            "[MDNS] Sent unicast response to {:?}",
                                            endpoint
                                        ),
                                        Err(e) => println!(
                                            "[MDNS] Failed to send unicast response: {:?}",
                                            e
                                        ),
                                    }
                                }
                            }
                        }
                        Ok(Err(_)) => {
                            // Silent socket error - mDNS is not critical
                        }
                        Err(_) => {
                            // Timeout - normal, continue loop
                        }
                    }
                }
            }
            Err(e) => {
                println!("[MDNS] Failed to bind to port 5353: {:?}", e);
            }
        }
    }
}

/// Create a proper mDNS response packet for service discovery
fn create_mdns_response(ip: embassy_net::Ipv4Address, port: u16) -> [u8; 512] {
    let mut response = [0u8; 512];

    // DNS Header (12 bytes) - Standard mDNS response format
    response[0] = 0x00;
    response[1] = 0x00; // Transaction ID: 0
    response[2] = 0x84;
    response[3] = 0x00; // Flags: Response (1), Authoritative (1), no recursion
    response[4] = 0x00;
    response[5] = 0x00; // Questions: 0
    response[6] = 0x00;
    response[7] = 0x03; // Answer RRs: 3 (PTR, SRV, A)
    response[8] = 0x00;
    response[9] = 0x00; // Authority RRs: 0
    response[10] = 0x00;
    response[11] = 0x00; // Additional RRs: 0

    let mut offset = 12;

    // Record 1: PTR Record "_ambient_light._udp.local." -> "board-rs._ambient_light._udp.local."
    let service_type_encoded = b"\x0e_ambient_light\x04_udp\x05local\x00";
    response[offset..offset + service_type_encoded.len()].copy_from_slice(service_type_encoded);
    offset += service_type_encoded.len();

    // PTR record header
    response[offset] = 0x00;
    response[offset + 1] = 0x0C; // Type: PTR (12)
    response[offset + 2] = 0x80;
    response[offset + 3] = 0x01; // Class: IN (1) with cache flush bit
    response[offset + 4] = 0x00;
    response[offset + 5] = 0x00; // TTL high
    response[offset + 6] = 0x00;
    response[offset + 7] = 0x78; // TTL low (120 seconds)
    offset += 8;

    // PTR data: "board-rs._ambient_light._udp.local."
    let instance_full = b"\x08board-rs\x0e_ambient_light\x04_udp\x05local\x00";
    response[offset] = 0x00;
    response[offset + 1] = instance_full.len() as u8; // Data length
    offset += 2;

    let instance_name_offset = offset;
    response[offset..offset + instance_full.len()].copy_from_slice(instance_full);
    offset += instance_full.len();

    // Record 2: SRV Record "board-rs._ambient_light._udp.local."
    // Use compression pointer to instance name
    response[offset] = 0xC0;
    response[offset + 1] = instance_name_offset as u8;
    offset += 2;

    // SRV record header
    response[offset] = 0x00;
    response[offset + 1] = 0x21; // Type: SRV (33)
    response[offset + 2] = 0x80;
    response[offset + 3] = 0x01; // Class: IN with cache flush bit
    response[offset + 4] = 0x00;
    response[offset + 5] = 0x00; // TTL high
    response[offset + 6] = 0x00;
    response[offset + 7] = 0x78; // TTL low
    offset += 8;

    // SRV data
    let hostname_encoded = b"\x08board-rs\x05local\x00";
    let srv_data_len = 6 + hostname_encoded.len(); // priority + weight + port + hostname
    response[offset] = 0x00;
    response[offset + 1] = srv_data_len as u8;
    offset += 2;

    response[offset] = 0x00;
    response[offset + 1] = 0x00; // Priority: 0
    response[offset + 2] = 0x00;
    response[offset + 3] = 0x00; // Weight: 0
    response[offset + 4] = (port >> 8) as u8;
    response[offset + 5] = (port & 0xFF) as u8; // Port
    offset += 6;

    // Target hostname "board-rs.local."
    let hostname_offset = offset;
    response[offset..offset + hostname_encoded.len()].copy_from_slice(hostname_encoded);
    offset += hostname_encoded.len();

    // Record 3: A Record "board-rs.local."
    // Use compression pointer to hostname
    response[offset] = 0xC0;
    response[offset + 1] = hostname_offset as u8;
    offset += 2;

    response[offset] = 0x00;
    response[offset + 1] = 0x01; // Type: A (1)
    response[offset + 2] = 0x80;
    response[offset + 3] = 0x01; // Class: IN with cache flush bit
    response[offset + 4] = 0x00;
    response[offset + 5] = 0x00; // TTL high
    response[offset + 6] = 0x00;
    response[offset + 7] = 0x78; // TTL low
    response[offset + 8] = 0x00;
    response[offset + 9] = 0x04; // Data length: 4
    offset += 10;

    // IP address
    let ip_octets = ip.octets();
    response[offset] = ip_octets[0];
    response[offset + 1] = ip_octets[1];
    response[offset + 2] = ip_octets[2];
    response[offset + 3] = ip_octets[3];

    response
}

#[esp_hal::main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Initialize heap allocator for WiFi (72KB)
    esp_alloc::heap_allocator!(size: 72 * 1024);

    // Initialize embassy time system
    let timer_group0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timer_group0.timer0);

    // Initialize WiFi driver
    let timer_group1 = TimerGroup::new(peripherals.TIMG1);
    let rng = Rng::new(peripherals.RNG);
    let wifi_init = esp_wifi::init(timer_group1.timer0, rng, peripherals.RADIO_CLK).unwrap();

    // Store wifi_init in static cell for 'static lifetime
    let wifi_init_ref = WIFI_INIT_CELL.init(wifi_init);

    // Create WiFi controller and device using esp-wifi 0.14.1 API with embassy-net support
    let (wifi_controller, wifi_interfaces) = wifi::new(wifi_init_ref, peripherals.WIFI).unwrap();
    let wifi_device = wifi_interfaces.sta;

    // Create embassy-net stack with DHCP configuration
    static STACK_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    let stack_resources = STACK_RESOURCES.init(StackResources::new());

    let config = Config::dhcpv4(Default::default());

    let (stack, runner) = embassy_net::new(wifi_device, config, stack_resources, 1234);

    // Create WiFi manager with controller
    use board_rs::wifi::WiFiManager;
    let mut wifi_manager = WiFiManager::new(wifi_controller);

    // Set the embassy-net stack for real DHCP functionality
    let stack_ref = STACK_CELL.init(stack);
    wifi_manager.set_stack(*stack_ref);

    // Initialize LED controller with WS2812 hardware driver
    use esp_hal::gpio::{Level, Output, OutputConfig};
    let mut test_pin = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());

    // Quick GPIO test
    for _ in 0..3 {
        test_pin.set_high();
        for _ in 0..500000 {
            unsafe {
                core::ptr::read_volatile(&0u32);
            }
        }
        test_pin.set_low();
        for _ in 0..500000 {
            unsafe {
                core::ptr::read_volatile(&0u32);
            }
        }
    }

    // Now reconfigure for RMT use
    let led_pin = test_pin.into_peripheral_output(); // Convert back to peripheral for RMT use

    // Initialize RMT peripheral with 10MHz frequency for better WS2812 timing
    let frequency = Rate::from_mhz(10);
    let rmt = Rmt::new(peripherals.RMT, frequency).unwrap();

    // Configure RMT channel for RGBW control
    let tx_config = esp_hal::rmt::TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output_level(esp_hal::gpio::Level::Low)
        .with_idle_output(false)
        .with_carrier_modulation(false);

    let rmt_channel = rmt.channel0.configure(led_pin, tx_config).unwrap();

    // Create LED controller with RMT channel
    use board_rs::led_control::UniversalDriverBoard;
    let led_controller = UniversalDriverBoard::new(rmt_channel);

    // Create static references for embassy tasks
    let _wifi_manager = WIFI_MANAGER_CELL.init(wifi_manager);
    let led_controller = LED_CONTROLLER_CELL.init(Mutex::new(led_controller));

    // Initialize system state machine
    let state_machine = SystemStateMachine::new();
    let _state_machine = STATE_MACHINE_CELL.init(Mutex::new(state_machine));

    // Initialize LED communication channels
    let (
        led_status_sender,
        led_data_sender,
        led_mode_sender,
        led_status_receiver,
        led_data_receiver,
        led_mode_receiver,
    ) = board_rs::led_control::init_led_channels();

    // Store senders in static cells for task access
    let _led_status_sender = LED_STATUS_SENDER_CELL.init(led_status_sender);
    let _led_data_sender = LED_DATA_SENDER_CELL.init(led_data_sender);
    let _led_mode_sender = LED_MODE_SENDER_CELL.init(led_mode_sender);

    // Initialize embassy executor and run tasks
    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(net_task(runner)).ok();
        spawner
            .spawn(state_machine_task(
                _wifi_manager,
                stack_ref,
                _led_status_sender,
                _state_machine,
            ))
            .ok();
        spawner
            .spawn(udp_server_task(stack_ref, _led_data_sender, _state_machine))
            .ok();
        spawner.spawn(mdns_server_task(stack_ref)).ok();
        // Start the LED task at 30fps
        spawner
            .spawn(board_rs::led_control::led_task(
                led_controller,
                led_status_receiver,
                led_data_receiver,
                led_mode_receiver,
            ))
            .ok();
    });
}

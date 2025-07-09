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
    led_controller: &'static Mutex<CriticalSectionRawMutex, LedControllerType>,
    state_machine: &'static Mutex<CriticalSectionRawMutex, SystemStateMachine>,
) -> ! {
    use embassy_time::{Duration, Timer};

    println!("[STATE] Starting state machine driven main task");

    // Initialize state machine
    {
        let mut sm = state_machine.lock().await;
        sm.handle_event(SystemEvent::SystemStarted);
    }

    // Main state machine loop
    loop {
        // Get current state and actions
        let (_current_state, actions) = {
            let mut sm = state_machine.lock().await;
            let actions = sm.update();
            (sm.get_current_state(), actions)
        };

        // Execute actions based on state machine output
        for action in actions {
            match action {
                Action::UpdateLEDStatus(status) => {
                    led_controller.lock().await.set_status(status);
                }
                Action::StartWiFiConnection => {
                    println!("[STATE] Executing WiFi connection...");
                    match wifi_manager.connect(config::WIFI_SSID, config::WIFI_PASSWORD) {
                        Ok(_) => {
                            println!("[WIFI] WiFi connection successful!");
                            state_machine
                                .lock()
                                .await
                                .handle_event(SystemEvent::WiFiConnected);
                        }
                        Err(_) => {
                            println!("[WIFI] WiFi connection failed");
                            state_machine
                                .lock()
                                .await
                                .handle_event(SystemEvent::WiFiConnectionFailed);
                        }
                    }
                }
                Action::StartDHCPRequest => {
                    println!("[STATE] Checking DHCP status...");
                    if let Some(ip) = wifi_manager.get_ip_address() {
                        println!(
                            "[DHCP] IP address obtained: {}.{}.{}.{}",
                            ip[0], ip[1], ip[2], ip[3]
                        );
                        state_machine
                            .lock()
                            .await
                            .handle_event(SystemEvent::DHCPSuccess);
                    } else {
                        // Continue waiting for DHCP
                        Timer::after(Duration::from_millis(1000)).await;
                    }
                }
                Action::StartNetworkServices => {
                    println!("[STATE] Starting UDP server...");
                    state_machine
                        .lock()
                        .await
                        .handle_event(SystemEvent::UDPServerStarted);
                }
                Action::StartUDPServer => {
                    println!("[STATE] UDP server already running as background task");
                    state_machine
                        .lock()
                        .await
                        .handle_event(SystemEvent::UDPServerStarted);
                }
                Action::StartMDNSService => {
                    if let Some(ip) = wifi_manager.get_ip_address() {
                        println!("[STATE] Starting mDNS service...");
                        use board_rs::mdns::MdnsManager;
                        let mut mdns_manager = MdnsManager::new();
                        match mdns_manager.start_service(ip) {
                            Ok(_) => {
                                println!("[MDNS] mDNS service started successfully");
                                // mDNS启动成功，但不改变状态机状态
                            }
                            Err(_) => {
                                println!("[MDNS] Failed to start mDNS service");
                                state_machine
                                    .lock()
                                    .await
                                    .handle_event(SystemEvent::UDPServerFailed);
                            }
                        }
                    }
                }
                Action::MonitorConnection => match wifi_manager.monitor_connection() {
                    Ok(_) => {
                        if wifi_manager.is_connected() {
                            if wifi_manager.get_ip_address().is_some() {
                                state_machine
                                    .lock()
                                    .await
                                    .handle_event(SystemEvent::WiFiConnected);
                            }
                        } else {
                            state_machine
                                .lock()
                                .await
                                .handle_event(SystemEvent::WiFiDisconnected);
                        }
                    }
                    Err(_) => {
                        state_machine
                            .lock()
                            .await
                            .handle_event(SystemEvent::WiFiDisconnected);
                    }
                },
                Action::SystemRecover => {
                    println!("[STATE] Initiating system recovery...");
                    state_machine
                        .lock()
                        .await
                        .handle_event(SystemEvent::RecoveryRequested);
                }
                Action::LogError(error_state) => {
                    println!("[STATE] Error logged: {:?}", error_state);
                }
                _ => {
                    // Handle other actions as needed
                }
            }
        }

        // Always update LED display
        led_controller.lock().await.update_display();

        // Small delay to prevent busy loop
        Timer::after(Duration::from_millis(100)).await;
    }
}

/// UDP server background task
#[embassy_executor::task]
async fn udp_server_task(
    stack: &'static Stack<'static>,
    led_controller: &'static Mutex<CriticalSectionRawMutex, LedControllerType>,
    state_machine: &'static Mutex<CriticalSectionRawMutex, SystemStateMachine>,
) {
    use board_rs::udp_server::UdpServer;

    println!("[UDP] Starting UDP server task...");

    // Create UDP server
    let mut udp_server = UdpServer::new();
    udp_server.set_stack(stack);

    // Bind to the configured port
    match udp_server.bind(config::UDP_PORT) {
        Ok(_) => {
            println!("[UDP] UDP server bound to port {}", config::UDP_PORT);

            // Start listening for packets
            match udp_server
                .start_listening(led_controller, state_machine)
                .await
            {
                Ok(_) => {
                    println!("[UDP] UDP server stopped unexpectedly");
                }
                Err(e) => {
                    println!("[UDP] UDP server error: {:?}", e);
                }
            }
        }
        Err(e) => {
            println!("[UDP] Failed to bind UDP server: {:?}", e);
        }
    }
}

/// mDNS server background task
#[embassy_executor::task]
async fn mdns_server_task(stack: &'static Stack<'static>) {
    use embassy_net::udp::UdpSocket;
    use embassy_net::{IpAddress, IpEndpoint};
    use embassy_time::{Duration, Timer};

    println!("[MDNS] Starting mDNS server task...");

    // Wait for network to be ready
    stack.wait_config_up().await;
    Timer::after(Duration::from_secs(2)).await;

    // Get our IP address
    let config = stack.config_v4();
    if let Some(config) = config {
        let our_ip = config.address.address();
        println!(
            "[MDNS] Our IP: {}.{}.{}.{}",
            our_ip.octets()[0],
            our_ip.octets()[1],
            our_ip.octets()[2],
            our_ip.octets()[3]
        );

        // Join mDNS multicast group (224.0.0.251)
        let mdns_multicast_addr = IpAddress::v4(224, 0, 0, 251);
        match stack.join_multicast_group(mdns_multicast_addr) {
            Ok(_) => {
                println!("[MDNS] ✅ Successfully joined mDNS multicast group 224.0.0.251");
            }
            Err(e) => {
                println!("[MDNS] ❌ Failed to join mDNS multicast group: {:?}", e);
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
                println!("[MDNS] ✅ mDNS socket bound to port 5353");

                // Create mDNS response packet
                let response = create_mdns_response(our_ip, board_rs::config::UDP_PORT);
                let mdns_multicast = IpEndpoint::new(mdns_multicast_addr, 5353);

                // Send initial mDNS announcement
                println!("[MDNS] Sending initial mDNS announcement...");
                match socket.send_to(&response, mdns_multicast).await {
                    Ok(_) => println!("[MDNS] ✅ Initial announcement sent"),
                    Err(e) => println!("[MDNS] ❌ Failed to send initial announcement: {:?}", e),
                }

                let mut last_announcement = embassy_time::Instant::now();

                // Start mDNS responder loop
                loop {
                    let mut buffer = [0u8; 1500];

                    // Send periodic announcements every 30 seconds
                    let now = embassy_time::Instant::now();
                    if now.duration_since(last_announcement) > Duration::from_secs(30) {
                        println!("[MDNS] Sending periodic mDNS announcement");
                        match socket.send_to(&response, mdns_multicast).await {
                            Ok(_) => println!("[MDNS] ✅ Periodic announcement sent"),
                            Err(e) => {
                                println!("[MDNS] ❌ Failed to send periodic announcement: {:?}", e)
                            }
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
                            println!("[MDNS] Received {} bytes from {:?}", len, endpoint);

                            // Simple mDNS query detection and response
                            if len > 12 {
                                // Check if this is a query (QR bit = 0)
                                if (buffer[2] & 0x80) == 0 {
                                    println!("[MDNS] 📡 Detected mDNS query, sending response");

                                    // Create response with matching transaction ID
                                    let mut query_response = response.clone();
                                    query_response[0] = buffer[0]; // Copy transaction ID
                                    query_response[1] = buffer[1];

                                    // Send mDNS response to multicast address
                                    match socket.send_to(&query_response, mdns_multicast).await {
                                        Ok(_) => println!(
                                            "[MDNS] ✅ Sent mDNS response to multicast address"
                                        ),
                                        Err(e) => println!(
                                            "[MDNS] ❌ Failed to send multicast response: {:?}",
                                            e
                                        ),
                                    }

                                    // Also send unicast response for compatibility
                                    match socket.send_to(&query_response, endpoint).await {
                                        Ok(_) => println!(
                                            "[MDNS] ✅ Sent unicast response to {:?}",
                                            endpoint
                                        ),
                                        Err(e) => println!(
                                            "[MDNS] ❌ Failed to send unicast response: {:?}",
                                            e
                                        ),
                                    }
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            println!("[MDNS] Socket error: {:?}", e);
                        }
                        Err(_) => {
                            // Timeout - normal, continue loop
                        }
                    }
                }
            }
            Err(e) => {
                println!("[MDNS] ❌ Failed to bind mDNS socket: {:?}", e);
            }
        }
    } else {
        println!("[MDNS] ❌ No IPv4 configuration available");
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

    println!("[WIFI] WiFi driver initialized successfully");

    // Store wifi_init in static cell for 'static lifetime
    let wifi_init_ref = WIFI_INIT_CELL.init(wifi_init);

    // Create WiFi controller and device using esp-wifi 0.14.1 API with embassy-net support
    let (wifi_controller, wifi_interfaces) = wifi::new(wifi_init_ref, peripherals.WIFI).unwrap();
    let wifi_device = wifi_interfaces.sta;

    println!("[WIFI] WiFi controller and device created successfully");

    // Create embassy-net stack with DHCP configuration
    static STACK_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    let stack_resources = STACK_RESOURCES.init(StackResources::new());

    let config = Config::dhcpv4(Default::default());

    let (stack, runner) = embassy_net::new(wifi_device, config, stack_resources, 1234);

    println!("[WIFI] Embassy-net stack created with DHCP configuration");

    // Create WiFi manager with controller
    use board_rs::wifi::WiFiManager;
    let mut wifi_manager = WiFiManager::new(wifi_controller);

    // Set the embassy-net stack for real DHCP functionality
    let stack_ref = STACK_CELL.init(stack);
    wifi_manager.set_stack(*stack_ref);

    println!("[WIFI] WiFi manager created successfully with real DHCP stack");
    println!("[WIFI] Embassy-net WiFi initialization completed");

    // Initialize LED controller with WS2812 hardware driver
    println!("[LED] Initializing LED controller...");

    // Initialize GPIO and RMT for WS2812 LED control
    println!(
        "[LED] Setting up GPIO pin {} for LED data...",
        config::LED_DATA_PIN
    );

    // First, test basic GPIO functionality
    println!("[LED] 🔧 Testing basic GPIO4 functionality...");
    use esp_hal::gpio::{Level, Output, OutputConfig};
    let mut test_pin = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());

    // Toggle GPIO4 a few times to test basic functionality
    for i in 0..5 {
        test_pin.set_high();
        println!("[LED] GPIO4 set HIGH (iteration {})", i + 1);
        // Small delay (busy wait)
        for _ in 0..1000000 {
            unsafe {
                core::ptr::read_volatile(&0u32);
            }
        }

        test_pin.set_low();
        println!("[LED] GPIO4 set LOW (iteration {})", i + 1);
        // Small delay (busy wait)
        for _ in 0..1000000 {
            unsafe {
                core::ptr::read_volatile(&0u32);
            }
        }
    }
    println!("[LED] ✅ Basic GPIO4 toggle test completed");

    // Now reconfigure for RMT use
    let led_pin = test_pin.into_peripheral_output(); // Convert back to peripheral for RMT use

    // Initialize RMT peripheral with 10MHz frequency for better WS2812 timing
    println!("[LED] Initializing RMT peripheral...");
    let frequency = Rate::from_mhz(10); // Further reduced from 20MHz for better WS2812 compatibility
    let rmt = match Rmt::new(peripherals.RMT, frequency) {
        Ok(rmt) => {
            println!("[LED] RMT peripheral initialized successfully at 10MHz");
            rmt
        }
        Err(e) => {
            println!("[LED] ❌ Failed to initialize RMT: {:?}", e);
            panic!("RMT initialization failed");
        }
    };

    // Configure RMT channel for RGBW control
    println!("[LED] Configuring RMT channel for RGBW control...");
    let tx_config = esp_hal::rmt::TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output_level(esp_hal::gpio::Level::Low)
        .with_idle_output(false)
        .with_carrier_modulation(false);

    let rmt_channel = rmt.channel0.configure(led_pin, tx_config).unwrap();

    // Create LED controller with RMT channel
    println!("[LED] Creating LED controller with RMT channel...");
    use board_rs::led_control::UniversalDriverBoard;
    let led_controller = UniversalDriverBoard::new(rmt_channel);

    println!("[LED] ✅ Universal driver board initialized successfully");

    // Create static references for embassy tasks
    let wifi_manager = WIFI_MANAGER_CELL.init(wifi_manager);
    let led_controller = LED_CONTROLLER_CELL.init(Mutex::new(led_controller));

    // Initialize system state machine
    println!("[STATE] Initializing system state machine...");
    let state_machine = SystemStateMachine::new();
    let state_machine = STATE_MACHINE_CELL.init(Mutex::new(state_machine));
    println!("[STATE] System state machine initialized successfully");

    // Initialize embassy executor and run tasks
    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        println!("[MAIN] Spawning network task...");
        spawner.spawn(net_task(runner)).ok();

        println!("[MAIN] Spawning state machine task...");
        spawner
            .spawn(state_machine_task(
                wifi_manager,
                stack_ref,
                led_controller,
                state_machine,
            ))
            .ok();

        println!("[MAIN] Spawning UDP server task...");
        spawner
            .spawn(udp_server_task(stack_ref, led_controller, state_machine))
            .ok();

        println!("[MAIN] Spawning mDNS server task...");
        match spawner.spawn(mdns_server_task(stack_ref)) {
            Ok(_) => println!("[MAIN] ✅ mDNS task spawned successfully"),
            Err(e) => println!("[MAIN] ❌ Failed to spawn mDNS task: {:?}", e),
        }
    });
}

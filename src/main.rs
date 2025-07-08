#![no_std]
#![no_main]

use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::rng::Rng;
use esp_hal::rmt::{Rmt, TxChannelCreator};
use esp_hal::time::Rate;
use esp_println::println;

// Standard library imports
extern crate alloc;
use alloc::boxed::Box;

// WiFi imports
use esp_wifi::wifi;

// Embassy-net imports
use embassy_net::{Config, StackResources, Stack};
use static_cell::StaticCell;
use esp_hal_embassy::Executor;


// LED control imports - using direct RMT for RGBW

// Import our library modules
use board_rs::config;

// Add app descriptor for espflash compatibility
esp_bootloader_esp_idf::esp_app_desc!();

// Static cells for embassy components
static WIFI_INIT_CELL: StaticCell<esp_wifi::EspWifiController<'static>> = StaticCell::new();
static STACK_CELL: StaticCell<Stack<'static>> = StaticCell::new();
static WIFI_MANAGER_CELL: StaticCell<board_rs::wifi::WiFiManager<'static>> = StaticCell::new();
static LED_CONTROLLER_CELL: StaticCell<board_rs::led_control::LedController> = StaticCell::new();

// Static executor for embassy tasks
static EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// Embassy task to run the network stack
#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, esp_wifi::wifi::WifiDevice<'static>>) -> ! {
    runner.run().await
}

// Main application task
#[embassy_executor::task]
async fn main_task(wifi_manager: &'static mut board_rs::wifi::WiFiManager<'static>, stack: &'static Stack<'static>) -> ! {
    use embassy_time::{Duration, Timer};

    // Connect to WiFi network
    println!("[WIFI] Attempting to connect to WiFi...");
    match wifi_manager.connect(config::WIFI_SSID, config::WIFI_PASSWORD) {
        Ok(_) => {
            println!("[WIFI] WiFi connection successful!");
            println!("[WIFI] DHCP will be handled automatically by embassy-net stack");
        },
        Err(_) => println!("[WIFI] WiFi connection failed, but continuing..."),
    }

    // Wait for DHCP to complete
    let mut dhcp_ip = None;
    for i in 0..20 {
        if let Some(ip) = wifi_manager.get_ip_address() {
            println!("[SUCCESS] DHCP IP address obtained: {}.{}.{}.{}",
                ip[0], ip[1], ip[2], ip[3]);
            wifi_manager.print_dhcp_info();
            dhcp_ip = Some(ip);
            break;
        } else {
            println!("[DHCP] Waiting for DHCP... attempt {}/20", i + 1);
            Timer::after(Duration::from_millis(1000)).await;
        }
    }

    // Start network services if DHCP succeeded
    if let Some(ip) = dhcp_ip {
        println!("[SERVICES] Starting network services...");

        // UDP server is started as background task in main()
        println!("[UDP] UDP server task already running on port {}", config::UDP_PORT);

        // Start mDNS service
        use board_rs::mdns::MdnsManager;
        let mut mdns_manager = MdnsManager::new();
        match mdns_manager.start_service(ip) {
            Ok(_) => println!("[MDNS] mDNS service started - device discoverable"),
            Err(_) => println!("[MDNS] Failed to start mDNS service"),
        }

        println!("[SERVICES] All network services initialized");
        println!("[READY] Hardware ready for desktop connection!");
        println!("[INFO] Desktop should discover device as: {}", config::MDNS_SERVICE_NAME);
        println!("[INFO] UDP port: {}", config::UDP_PORT);
        println!("[INFO] IP address: {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);
    } else {
        println!("[ERROR] DHCP failed - network services not started");
    }

    // Main application loop
    let mut counter = 0u32;
    let mut services_started = false;

    loop {
        counter += 1;
        println!("[STATUS] Heartbeat #{:06} - System operational", counter);

        // Monitor WiFi connection every 5 iterations
        if counter % 5 == 0 {
            match wifi_manager.monitor_connection() {
                Ok(_) => {
                    if wifi_manager.is_connected() {
                        if let Some(ip) = wifi_manager.get_ip_address() {
                            println!("[WIFI] Connected - IP: {:?}", ip);

                            // Start services if not already started
                            if !services_started {
                                println!("[SERVICES] Starting network services...");

                                // UDP server is started as background task in main()
                                println!("[UDP] UDP server task already running on port {}", config::UDP_PORT);

                                // Start mDNS service
                                use board_rs::mdns::MdnsManager;
                                let mut mdns_manager = MdnsManager::new();
                                match mdns_manager.start_service(ip) {
                                    Ok(_) => println!("[MDNS] mDNS service started - device discoverable"),
                                    Err(_) => println!("[MDNS] Failed to start mDNS service"),
                                }

                                println!("[SERVICES] All network services initialized");
                                println!("[READY] Hardware ready for desktop connection!");
                                println!("[INFO] Desktop should discover device as: {}", config::MDNS_SERVICE_NAME);
                                println!("[INFO] UDP port: {}", config::UDP_PORT);
                                println!("[INFO] IP address: {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);

                                services_started = true;
                            }
                        } else {
                            println!("[WIFI] Connected - IP pending");
                        }
                    } else {
                        println!("[WIFI] Disconnected");
                        services_started = false; // Reset services if disconnected
                    }
                },
                Err(_) => println!("[WIFI] Connection monitoring error"),
            }
        }

        Timer::after(Duration::from_millis(1000)).await;
    }
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
    let wifi_init = esp_wifi::init(
        timer_group1.timer0,
        rng,
        peripherals.RADIO_CLK,
    ).unwrap();

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

    // Create LED controller first (software only)
    use board_rs::led_control::{LedController, RgbwController};
    let mut led_controller = LedController::new();
    led_controller.init().unwrap();
    println!("[LED] LED controller basic initialization complete");

    // Initialize GPIO and RMT for WS2812 LED control
    println!("[LED] Setting up GPIO pin {} for LED data...", config::LED_DATA_PIN);

    // First, test basic GPIO functionality
    println!("[LED] 🔧 Testing basic GPIO4 functionality...");
    use esp_hal::gpio::{Output, Level, OutputConfig};
    let mut test_pin = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());

    // Toggle GPIO4 a few times to test basic functionality
    for i in 0..5 {
        test_pin.set_high();
        println!("[LED] GPIO4 set HIGH (iteration {})", i + 1);
        // Small delay (busy wait)
        for _ in 0..1000000 { unsafe { core::ptr::read_volatile(&0u32); } }

        test_pin.set_low();
        println!("[LED] GPIO4 set LOW (iteration {})", i + 1);
        // Small delay (busy wait)
        for _ in 0..1000000 { unsafe { core::ptr::read_volatile(&0u32); } }
    }
    println!("[LED] ✅ Basic GPIO4 toggle test completed");

    // Now reconfigure for RMT use
    let led_pin = test_pin.into_peripheral_output(); // Convert back to peripheral for RMT use

    // Initialize RMT peripheral with 10MHz frequency for better WS2812 timing
    println!("[LED] Initializing RMT peripheral...");
    let frequency = Rate::from_mhz(10);  // Further reduced from 20MHz for better WS2812 compatibility
    let rmt = match Rmt::new(peripherals.RMT, frequency) {
        Ok(rmt) => {
            println!("[LED] RMT peripheral initialized successfully at 10MHz");
            rmt
        },
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

    // Create RGBW controller for SK6812-RGBW control
    println!("[LED] Creating RGBW controller...");
    let rgbw_controller = RgbwController::new(rmt_channel);
    led_controller.set_strip_controller(Box::new(rgbw_controller));

    println!("[LED] ✅ LED controller initialized successfully with RGBW hardware driver");

    // 运行最小LED测试来验证基本驱动功能
    println!("[LED] 🧪 运行最小LED测试...");
    match led_controller.minimal_led_test() {
        Ok(_) => println!("[LED] ✅ 最小LED测试完成"),
        Err(e) => println!("[LED] ❌ 最小LED测试失败: {:?}", e),
    }

    // Create static references for embassy tasks
    let wifi_manager = WIFI_MANAGER_CELL.init(wifi_manager);
    let led_controller = LED_CONTROLLER_CELL.init(led_controller);

    // Initialize embassy executor and run tasks
    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(net_task(runner)).ok();
        spawner.spawn(main_task(wifi_manager, stack_ref)).ok();
        spawner.spawn(udp_server_task(stack_ref, led_controller)).ok();
        spawner.spawn(mdns_broadcast_task(stack_ref)).ok();
    });
}

/// UDP server background task
#[embassy_executor::task]
async fn udp_server_task(stack: &'static Stack<'static>, led_controller: &'static mut board_rs::led_control::LedController) {
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
            match udp_server.start_listening(led_controller).await {
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

/// mDNS responder task for service discovery
#[embassy_executor::task]
async fn mdns_broadcast_task(stack: &'static Stack<'static>) {
    use embassy_net::udp::UdpSocket;
    use embassy_time::{Duration, Timer};
    use embassy_net::{IpEndpoint, IpAddress};

    println!("[MDNS] Starting mDNS responder task...");

    // Wait for network to be ready
    stack.wait_config_up().await;
    Timer::after(Duration::from_secs(2)).await;

    // Join mDNS multicast group (224.0.0.251) - CRITICAL for receiving multicast queries
    let mdns_multicast_addr = IpAddress::v4(224, 0, 0, 251);
    match stack.join_multicast_group(mdns_multicast_addr) {
        Ok(_) => {
            println!("[MDNS] ✅ Successfully joined mDNS multicast group 224.0.0.251");
            println!("[MDNS] ESP32 can now receive multicast mDNS queries");
        }
        Err(e) => {
            println!("[MDNS] ❌ Failed to join mDNS multicast group: {:?}", e);
            println!("[MDNS] Continuing without multicast support - only direct queries will work");
        }
    }

    // Create UDP socket for mDNS multicast
    let mut rx_buffer = [0; 1500];
    let mut tx_buffer = [0; 1500];
    let mut rx_meta = [embassy_net::udp::PacketMetadata::EMPTY; 8];
    let mut tx_meta = [embassy_net::udp::PacketMetadata::EMPTY; 8];
    let mut socket = UdpSocket::new(*stack, &mut rx_meta, &mut rx_buffer, &mut tx_meta, &mut tx_buffer);

    // Bind to mDNS port (5353) for multicast reception
    match socket.bind(5353) {
        Ok(_) => {
            println!("[MDNS] mDNS socket bound to port 5353");
            println!("[MDNS] Socket ready to receive multicast packets on 224.0.0.251:5353");

            // Get our IP address for responses
            let config = stack.config_v4();
            if let Some(config) = config {
                let our_ip = config.address.address();
                println!("[MDNS] Our IP: {:?}", our_ip);

                // Send initial mDNS announcement
                let mdns_multicast = IpEndpoint::new(IpAddress::v4(224, 0, 0, 251), 5353);
                let response = create_mdns_response(our_ip, config::UDP_PORT);

                println!("[MDNS] Sending initial mDNS announcement...");
                match socket.send_to(&response, mdns_multicast).await {
                    Ok(_) => println!("[MDNS] Initial announcement sent successfully"),
                    Err(e) => println!("[MDNS] Failed to send initial announcement: {:?}", e),
                }

                let mut last_announcement = embassy_time::Instant::now();

                // Start mDNS responder loop
                loop {
                    let mut buffer = [0u8; 1500];

                    // Send periodic announcements every 30 seconds
                    let now = embassy_time::Instant::now();
                    if now.duration_since(last_announcement) > embassy_time::Duration::from_secs(30) {
                        println!("[MDNS] Sending periodic mDNS announcement");
                        match socket.send_to(&response, mdns_multicast).await {
                            Ok(_) => println!("[MDNS] Periodic announcement sent"),
                            Err(e) => println!("[MDNS] Failed to send periodic announcement: {:?}", e),
                        }
                        last_announcement = now;
                    }

                    // Listen for mDNS queries with timeout
                    match embassy_time::with_timeout(
                        embassy_time::Duration::from_millis(1000),
                        socket.recv_from(&mut buffer)
                    ).await {
                        Ok(Ok((len, endpoint))) => {
                            println!("[MDNS] Received {} bytes from {:?}", len, endpoint);

                            // Log packet header for debugging
                            if len >= 12 {
                                println!("[MDNS] Header: {:02x}{:02x} {:02x}{:02x} {:02x}{:02x} {:02x}{:02x}",
                                    buffer[0], buffer[1], buffer[2], buffer[3],
                                    buffer[4], buffer[5], buffer[6], buffer[7]);
                            }

                            // Simple mDNS query detection and response
                            if len > 12 { // Minimum DNS header size
                                // Check if this is a query (QR bit = 0)
                                if (buffer[2] & 0x80) == 0 {
                                    println!("[MDNS] Detected mDNS query, sending response");
                                    println!("[MDNS] Query from: {:?}", endpoint);

                                    // Create response with matching transaction ID
                                    let mut query_response = response.clone();
                                    query_response[0] = buffer[0]; // Copy transaction ID
                                    query_response[1] = buffer[1];

                                    // CRITICAL FIX: Send mDNS response to multicast address, not back to sender
                                    // This is required for standard mDNS compliance
                                    let multicast_endpoint = IpEndpoint::new(IpAddress::v4(224, 0, 0, 251), 5353);
                                    match socket.send_to(&query_response, multicast_endpoint).await {
                                        Ok(_) => println!("[MDNS] Sent mDNS response to multicast address"),
                                        Err(e) => println!("[MDNS] Failed to send multicast response: {:?}", e),
                                    }

                                    // Also send unicast response for compatibility
                                    match socket.send_to(&query_response, endpoint).await {
                                        Ok(_) => println!("[MDNS] Sent unicast response to {:?}", endpoint),
                                        Err(e) => println!("[MDNS] Failed to send unicast response: {:?}", e),
                                    }
                                } else {
                                    println!("[MDNS] Received mDNS response (ignoring)");
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            println!("[MDNS] Error receiving: {:?}", e);
                        }
                        Err(_) => {
                            // Timeout - normal, continue loop
                        }
                    }
                }
            } else {
                println!("[MDNS] No IPv4 configuration available");
            }
        }
        Err(e) => {
            println!("[MDNS] Failed to bind mDNS socket: {:?}", e);
        }
    }
}

/// Create a proper mDNS response packet for service discovery
fn create_mdns_response(ip: embassy_net::Ipv4Address, port: u16) -> [u8; 512] {
    let mut response = [0u8; 512];

    // DNS Header (12 bytes) - Standard mDNS response format
    response[0] = 0x00; response[1] = 0x00; // Transaction ID: 0
    response[2] = 0x84; response[3] = 0x00; // Flags: Response (1), Authoritative (1), no recursion
    response[4] = 0x00; response[5] = 0x00; // Questions: 0
    response[6] = 0x00; response[7] = 0x03; // Answer RRs: 3 (PTR, SRV, A)
    response[8] = 0x00; response[9] = 0x00; // Authority RRs: 0
    response[10] = 0x00; response[11] = 0x00; // Additional RRs: 0

    let mut offset = 12;

    // Record 1: PTR Record "_ambient_light._udp.local." -> "board-rs._ambient_light._udp.local."
    // Name: "_ambient_light._udp.local." (FIXED: 14 chars, not 15)
    let service_type_encoded = b"\x0e_ambient_light\x04_udp\x05local\x00";
    response[offset..offset + service_type_encoded.len()].copy_from_slice(service_type_encoded);
    offset += service_type_encoded.len();

    // PTR record header
    response[offset] = 0x00; response[offset + 1] = 0x0C; // Type: PTR (12)
    response[offset + 2] = 0x80; response[offset + 3] = 0x01; // Class: IN (1) with cache flush bit
    response[offset + 4] = 0x00; response[offset + 5] = 0x00; // TTL high
    response[offset + 6] = 0x00; response[offset + 7] = 0x78; // TTL low (120 seconds)
    offset += 8;

    // PTR data: "board-rs._ambient_light._udp.local." (FIXED: 14 chars)
    let instance_full = b"\x08board-rs\x0e_ambient_light\x04_udp\x05local\x00";
    response[offset] = 0x00; response[offset + 1] = instance_full.len() as u8; // Data length
    offset += 2;

    let instance_name_offset = offset;
    response[offset..offset + instance_full.len()].copy_from_slice(instance_full);
    offset += instance_full.len();

    // Record 2: SRV Record "board-rs._ambient_light._udp.local."
    // Use compression pointer to instance name
    response[offset] = 0xC0; response[offset + 1] = instance_name_offset as u8;
    offset += 2;

    // SRV record header
    response[offset] = 0x00; response[offset + 1] = 0x21; // Type: SRV (33)
    response[offset + 2] = 0x80; response[offset + 3] = 0x01; // Class: IN with cache flush bit
    response[offset + 4] = 0x00; response[offset + 5] = 0x00; // TTL high
    response[offset + 6] = 0x00; response[offset + 7] = 0x78; // TTL low
    offset += 8;

    // SRV data
    let hostname_encoded = b"\x08board-rs\x05local\x00";
    let srv_data_len = 6 + hostname_encoded.len(); // priority + weight + port + hostname
    response[offset] = 0x00; response[offset + 1] = srv_data_len as u8;
    offset += 2;

    response[offset] = 0x00; response[offset + 1] = 0x00; // Priority: 0
    response[offset + 2] = 0x00; response[offset + 3] = 0x00; // Weight: 0
    response[offset + 4] = (port >> 8) as u8; response[offset + 5] = (port & 0xFF) as u8; // Port
    offset += 6;

    // Target hostname "board-rs.local."
    let hostname_offset = offset;
    response[offset..offset + hostname_encoded.len()].copy_from_slice(hostname_encoded);
    offset += hostname_encoded.len();

    // Record 3: A Record "board-rs.local."
    // Use compression pointer to hostname
    response[offset] = 0xC0; response[offset + 1] = hostname_offset as u8;
    offset += 2;

    response[offset] = 0x00; response[offset + 1] = 0x01; // Type: A (1)
    response[offset + 2] = 0x80; response[offset + 3] = 0x01; // Class: IN with cache flush bit
    response[offset + 4] = 0x00; response[offset + 5] = 0x00; // TTL high
    response[offset + 6] = 0x00; response[offset + 7] = 0x78; // TTL low
    response[offset + 8] = 0x00; response[offset + 9] = 0x04; // Data length: 4
    offset += 10;

    // IP address
    let ip_octets = ip.octets();
    response[offset] = ip_octets[0];
    response[offset + 1] = ip_octets[1];
    response[offset + 2] = ip_octets[2];
    response[offset + 3] = ip_octets[3];

    response
}

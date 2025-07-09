//! UDP communication server module
//!
//! Handles UDP socket creation, packet reception, and protocol parsing.

use crate::led_control::UniversalDriverBoard;
use crate::{BoardError, config};
use embassy_net::{
    Stack,
    udp::{PacketMetadata, UdpSocket},
};
use esp_println::println;
use heapless::Vec;

/// Maximum UDP packet size for LED data
const MAX_PACKET_SIZE: usize = 4096;

/// UDP packet structure for LED data
#[derive(Debug)]
pub struct LedPacket {
    /// LED start offset (16-bit big-endian)
    pub offset: u16,
    /// LED color data (RGB or RGBW)
    pub data: Vec<u8, MAX_PACKET_SIZE>,
}

/// UDP server for receiving LED data packets
pub struct UdpServer<'a> {
    port: u16,
    is_bound: bool,
    stack: Option<&'a Stack<'a>>,
}

impl<'a> UdpServer<'a> {
    /// Create a new UDP server instance
    pub fn new() -> Self {
        Self {
            port: 0,
            is_bound: false,
            stack: None,
        }
    }

    /// Set the network stack for UDP operations
    pub fn set_stack(&mut self, stack: &'a Stack<'a>) {
        self.stack = Some(stack);
    }

    /// Bind to the specified port and start listening
    pub fn bind(&mut self, port: u16) -> Result<(), BoardError> {
        println!("[UDP] Binding UDP server to port {}", port);

        if self.stack.is_none() {
            println!("[UDP] Error: Network stack not set");
            return Err(BoardError::UdpError);
        }

        // For now, just mark as bound - actual socket creation will be done in receive_packet
        // This is because embassy-net UDP sockets are created per-operation
        self.port = port;
        self.is_bound = true;

        println!("[UDP] UDP server bound successfully to port {}", port);
        println!("[UDP] Ready to receive LED data packets");

        Ok(())
    }

    /// Start UDP server and listen for packets (async)
    pub async fn start_listening<TX>(
        &mut self,
        led_controller: &embassy_sync::mutex::Mutex<
            embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
            UniversalDriverBoard<TX>,
        >,
        state_machine: &embassy_sync::mutex::Mutex<
            embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
            crate::state_machine::SystemStateMachine,
        >,
    ) -> Result<(), BoardError>
    where
        TX: esp_hal::rmt::TxChannel,
    {
        if !self.is_bound {
            return Err(BoardError::UdpError);
        }

        let stack = self.stack.ok_or(BoardError::UdpError)?;

        // Create UDP socket buffers
        let mut rx_buffer = [0; 4096];
        let mut tx_buffer = [0; 4096];
        let mut rx_meta = [PacketMetadata::EMPTY; 16];
        let mut tx_meta = [PacketMetadata::EMPTY; 16];
        let mut socket = UdpSocket::new(
            *stack,
            &mut rx_meta,
            &mut rx_buffer,
            &mut tx_meta,
            &mut tx_buffer,
        );

        // Bind to the configured port
        match socket.bind(self.port) {
            Ok(_) => {
                println!("[UDP] Successfully bound to port {}", self.port);
            }
            Err(e) => {
                println!("[UDP] Failed to bind to port {}: {:?}", self.port, e);
                return Err(BoardError::UdpError);
            }
        }

        println!("[UDP] UDP server listening on port {}", self.port);

        // Start packet reception loop
        self.packet_loop(&mut socket, led_controller, state_machine)
            .await
    }

    /// Main packet reception loop
    async fn packet_loop<TX>(
        &mut self,
        socket: &mut UdpSocket<'_>,
        led_controller: &embassy_sync::mutex::Mutex<
            embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
            UniversalDriverBoard<TX>,
        >,
        state_machine: &embassy_sync::mutex::Mutex<
            embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
            crate::state_machine::SystemStateMachine,
        >,
    ) -> Result<(), BoardError>
    where
        TX: esp_hal::rmt::TxChannel,
    {
        use embassy_time::{Duration, Instant};

        let mut buffer = [0u8; MAX_PACKET_SIZE];
        let mut last_connection_check = Instant::now();
        let connection_timeout = Duration::from_secs(30); // 30秒超时

        loop {
            // 使用超时接收数据
            match embassy_time::with_timeout(
                Duration::from_millis(1000),
                socket.recv_from(&mut buffer),
            )
            .await
            {
                Ok(Ok((len, endpoint))) => {
                    println!("[UDP] Received {} bytes from {:?}", len, endpoint);

                    // Check if this is a connection check packet
                    if Self::is_connection_check(&buffer[..len]) {
                        println!("[UDP] ✅ Connection check packet - sending response");

                        // 更新最后收到连接检查的时间
                        last_connection_check = Instant::now();

                        // 触发状态机事件：收到0x01消息
                        state_machine.lock().await.handle_event(
                            crate::state_machine::SystemEvent::ConnectionCheckReceived,
                        );

                        // Send connection response (echo back 0x01)
                        let response = [config::CONNECTION_CHECK_HEADER];
                        match socket.send_to(&response, endpoint.endpoint).await {
                            Ok(_) => {
                                println!(
                                    "[UDP] ✅ Connection response sent to {:?}",
                                    endpoint.endpoint
                                );
                            }
                            Err(e) => {
                                println!("[UDP] ❌ Failed to send connection response: {:?}", e);
                            }
                        }
                        continue; // Skip LED packet processing
                    }

                    // Check for 0x03 packets and ignore them completely
                    if !buffer.is_empty() && buffer[0] == 0x03 {
                        println!(
                            "[UDP] 🚫 Ignoring 0x03 packet ({} bytes) - not a frame sync",
                            len
                        );
                        continue; // Skip processing this packet entirely
                    }

                    // Process LED data packets
                    match Self::parse_packet(&buffer[..len]) {
                        Ok(packet) => {
                            println!(
                                "[UDP] ✅ Parsed LED packet: offset={}, data_len={}",
                                packet.offset,
                                packet.data.len()
                            );

                            // Forward packet to LED controller
                            match led_controller.lock().await.update_leds(&packet) {
                                Ok(_) => {
                                    // 触发状态机事件：收到LED数据
                                    state_machine.lock().await.handle_event(
                                        crate::state_machine::SystemEvent::LEDDataReceived,
                                    );
                                }
                                Err(e) => {
                                    println!("[UDP] ❌ Failed to process LED packet: {:?}", e);
                                }
                            }
                        }
                        Err(e) => {
                            println!("[UDP] ❌ Error parsing LED packet: {:?}", e);
                        }
                    }
                }
                Ok(Err(e)) => {
                    println!("[UDP] Socket error: {:?}", e);
                    // Continue listening despite socket errors
                }
                Err(_) => {
                    // 超时 - 检查是否需要触发超时事件
                    let now = Instant::now();
                    if now.duration_since(last_connection_check) > connection_timeout {
                        println!(
                            "[UDP] ⚠️ Connection check timeout - no 0x01 message received for {} seconds",
                            connection_timeout.as_secs()
                        );

                        // 触发状态机事件：UDP超时
                        state_machine
                            .lock()
                            .await
                            .handle_event(crate::state_machine::SystemEvent::UDPTimeout);

                        // 重置超时计时器
                        last_connection_check = now;
                    }
                }
            }
        }
    }

    /// Receive and parse a UDP packet (legacy method - now deprecated)
    pub fn receive_packet(&mut self) -> Result<Option<LedPacket>, BoardError> {
        // This method is now deprecated in favor of start_listening()
        // which provides proper async UDP reception
        println!("[UDP] Warning: receive_packet() is deprecated, use start_listening() instead");
        Ok(None)
    }

    /// Check if server is bound and listening
    pub fn is_bound(&self) -> bool {
        self.is_bound
    }

    /// Get the bound port
    pub fn get_port(&self) -> u16 {
        self.port
    }

    /// Check if packet is a connection check packet
    pub fn is_connection_check(data: &[u8]) -> bool {
        data.len() == 1 && data[0] == config::CONNECTION_CHECK_HEADER
    }

    /// Parse raw packet data according to protocol specification
    pub fn parse_packet(data: &[u8]) -> Result<LedPacket, BoardError> {
        // 解析数据包，不打印详细的数据内容

        // Connection check packets should be handled before calling this function
        if Self::is_connection_check(data) {
            println!("[UDP] ❌ Connection check packet should not reach parse_packet()");
            return Err(BoardError::ProtocolError);
        }

        if data.len() < 3 {
            println!(
                "[UDP] Packet too short: {} bytes (minimum 3 required)",
                data.len()
            );
            return Err(BoardError::ProtocolError);
        }

        // Check protocol header
        if data[0] != config::PROTOCOL_HEADER {
            println!(
                "[UDP] Invalid header: 0x{:02x} (expected 0x{:02x})",
                data[0],
                config::PROTOCOL_HEADER
            );
            return Err(BoardError::ProtocolError);
        }

        // Parse offset (16-bit big-endian)
        let offset = u16::from_be_bytes([data[1], data[2]]);
        println!(
            "[UDP] Valid LED packet: header=0x{:02x}, offset={}",
            data[0], offset
        );

        // Extract LED data
        let led_data = &data[3..];
        let mut data_vec = Vec::new();

        for &byte in led_data {
            if data_vec.push(byte).is_err() {
                return Err(BoardError::ProtocolError);
            }
        }

        // LED数据解析完成，不打印数据长度

        Ok(LedPacket {
            offset,
            data: data_vec,
        })
    }
}

impl<'a> Default for UdpServer<'a> {
    fn default() -> Self {
        Self::new()
    }
}

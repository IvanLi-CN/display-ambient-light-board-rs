//! UDP communication server module
//!
//! Handles UDP socket creation, packet reception, and protocol parsing.

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
        if self.stack.is_none() {
            return Err(BoardError::UdpError);
        }

        // For now, just mark as bound - actual socket creation will be done in receive_packet
        // This is because embassy-net UDP sockets are created per-operation
        self.port = port;
        self.is_bound = true;

        Ok(())
    }

    /// Start UDP server and listen for packets (async)
    pub async fn start_listening(
        &mut self,
        led_data_sender: &embassy_sync::channel::Sender<
            'static,
            embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
            crate::led_control::LedData,
            4,
        >,
        state_machine: &embassy_sync::mutex::Mutex<
            embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
            crate::state_machine::SystemStateMachine,
        >,
    ) -> Result<(), BoardError> {
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
                println!("[UDP] Listening on port {}", self.port);
            }
            Err(e) => {
                println!("[UDP] Bind failed: {:?}", e);
                return Err(BoardError::UdpError);
            }
        }

        // Start packet reception loop
        self.packet_loop(&mut socket, led_data_sender, state_machine)
            .await
    }

    /// Main packet reception loop
    async fn packet_loop(
        &mut self,
        socket: &mut UdpSocket<'_>,
        led_data_sender: &embassy_sync::channel::Sender<
            'static,
            embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
            crate::led_control::LedData,
            4,
        >,
        state_machine: &embassy_sync::mutex::Mutex<
            embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
            crate::state_machine::SystemStateMachine,
        >,
    ) -> Result<(), BoardError> {
        use embassy_time::{Duration, Instant};

        let mut buffer = [0u8; MAX_PACKET_SIZE];
        let mut last_connection_check = Instant::now();
        let connection_timeout = Duration::from_secs(30); // 30秒超时

        // Batch state machine events to reduce lock contention
        let mut pending_events = heapless::Vec::<crate::state_machine::SystemEvent, 8>::new();
        let mut last_state_update = Instant::now();
        let state_update_interval = Duration::from_millis(100); // Update state machine every 100ms

        loop {
            // 使用超时接收数据
            match embassy_time::with_timeout(
                Duration::from_millis(100), // Reduced timeout for more responsive state updates
                socket.recv_from(&mut buffer),
            )
            .await
            {
                Ok(Ok((len, endpoint))) => {
                    // Check if this is a connection check packet
                    if Self::is_connection_check(&buffer[..len]) {
                        // 更新最后收到连接检查的时间
                        last_connection_check = Instant::now();

                        // Queue state machine event instead of immediate lock
                        let _ = pending_events
                            .push(crate::state_machine::SystemEvent::ConnectionCheckReceived);

                        // Send connection response (echo back 0x01)
                        let response = [config::CONNECTION_CHECK_HEADER];
                        socket.send_to(&response, endpoint.endpoint).await.ok();
                        continue; // Skip LED packet processing
                    }

                    // Check for 0x03 packets and ignore them completely
                    if !buffer.is_empty() && buffer[0] == 0x03 {
                        continue; // Skip processing this packet entirely
                    }

                    // Process LED data packets
                    match Self::parse_packet(&buffer[..len]) {
                        Ok(packet) => {
                            // Create LED data and send to LED task
                            let led_data = crate::led_control::LedData {
                                data: packet.data.to_vec(),
                                timestamp: embassy_time::Instant::now(),
                            };

                            // Send LED data to LED task via channel
                            match led_data_sender.try_send(led_data) {
                                Ok(_) => {
                                    // Queue state machine event instead of immediate lock
                                    let _ = pending_events
                                        .push(crate::state_machine::SystemEvent::LEDDataReceived);
                                }
                                Err(_) => {
                                    // Channel full or other error - silent handling
                                }
                            }
                        }
                        Err(_) => {
                            // Silent error - invalid packets are common
                        }
                    }
                }
                Ok(Err(_)) => {
                    // Continue listening despite socket errors
                }
                Err(_) => {
                    // 超时 - 检查是否需要触发超时事件
                    let now = Instant::now();
                    if now.duration_since(last_connection_check) > connection_timeout {
                        static mut LAST_TIMEOUT_LOG: Option<Instant> = None;
                        let should_log = unsafe {
                            LAST_TIMEOUT_LOG.map_or(true, |last| {
                                now.duration_since(last) > Duration::from_secs(30)
                            })
                        };

                        if should_log {
                            println!(
                                "[UDP] ⚠️ Connection check timeout - no 0x01 message received for {} seconds",
                                connection_timeout.as_secs()
                            );
                            unsafe {
                                LAST_TIMEOUT_LOG = Some(now);
                            }
                        }

                        // Queue timeout event
                        let _ = pending_events.push(crate::state_machine::SystemEvent::UDPTimeout);

                        // 重置超时计时器
                        last_connection_check = now;
                    }
                }
            }

            // Process batched state machine events periodically
            let now = Instant::now();
            if !pending_events.is_empty()
                && now.duration_since(last_state_update) >= state_update_interval
            {
                let mut sm = state_machine.lock().await;
                for event in pending_events.iter() {
                    sm.handle_event(*event);
                }
                pending_events.clear();
                last_state_update = now;
            }
        }
    }

    /// Receive and parse a UDP packet (legacy method - now deprecated)
    pub fn receive_packet(&mut self) -> Result<Option<LedPacket>, BoardError> {
        // This method is now deprecated in favor of start_listening()
        // which provides proper async UDP reception
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
            return Err(BoardError::ProtocolError);
        }

        if data.len() < 3 {
            return Err(BoardError::ProtocolError);
        }

        // Check protocol header
        if data[0] != config::PROTOCOL_HEADER {
            return Err(BoardError::ProtocolError);
        }

        // Parse offset (16-bit big-endian)
        let offset = u16::from_be_bytes([data[1], data[2]]);

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

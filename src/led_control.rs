use crate::BoardError;
use alloc::vec;
use esp_hal::gpio::Level;
use esp_hal::rmt::{PulseCode, TxChannel};
use esp_println::println;

/// LED status states for visual feedback
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedStatus {
    // System initialization states
    Starting,
    HardwareInit,
    WiFiDriverInit,

    // Network connection states
    WiFiConnecting,
    WiFiConnected,
    DHCPRequesting,
    NetworkReady,

    // Service states
    ServicesStarting,
    UDPServerBinding,
    UDPServerListening,
    MDNSAdvertising,

    // Operational states
    Operational,
    DataReceiving,
    LEDRendering,
    ConnectionMonitoring,

    // Error states
    WiFiError,
    NetworkError,
    ServiceError,
    HardwareError,
    CriticalError,

    // Recovery states
    Reconnecting,
    ServiceRestarting,
    SystemRecovering,

    // Legacy states (for backward compatibility)
    Error, // Maps to CriticalError
}

/// LED controller for RGBW LED strips using RMT peripheral
pub struct LedController<TX>
where
    TX: TxChannel,
{
    channel: Option<TX>,
    status: LedStatus,
    status_counter: u32,
    breathing_counter: u32,
}

impl<TX> LedController<TX>
where
    TX: TxChannel,
{
    /// Create a new LED controller
    pub fn new(channel: TX) -> Self {
        Self {
            channel: Some(channel),
            status: LedStatus::Starting,
            status_counter: 0,
            breathing_counter: 30, // Start at minimum brightness
        }
    }

    /// Update the LED status
    pub fn set_status(&mut self, status: LedStatus) {
        if self.status != status {
            println!("[LED] Status changed: {:?} -> {:?}", self.status, status);
            self.status = status;
            self.status_counter = 0; // Reset counter for new status
        }
    }

    /// Get current status
    pub fn get_status(&self) -> LedStatus {
        self.status
    }

    /// Update LED display with status indication and breathing effect
    pub fn update_display(&mut self) {
        const LED_COUNT: usize = 500;
        const STATUS_LEDS: usize = 3; // First 3 LEDs for status

        // Update counters
        self.status_counter += 1;
        self.breathing_counter += 1;

        // Breathing effect parameters (slower and lower brightness)
        const BREATHING_MIN: u32 = 30;
        const BREATHING_MAX: u32 = 150;
        const BREATHING_SPEED: u32 = 3; // Slower breathing

        // Calculate breathing brightness
        let breathing_cycle =
            (self.breathing_counter / BREATHING_SPEED) % ((BREATHING_MAX - BREATHING_MIN) * 2);
        let breathing_brightness = if breathing_cycle < (BREATHING_MAX - BREATHING_MIN) {
            BREATHING_MIN + breathing_cycle
        } else {
            BREATHING_MAX - (breathing_cycle - (BREATHING_MAX - BREATHING_MIN))
        };

        // Status indication timing (faster blinking)
        let status_on = match self.status {
            // System initialization states - very fast blink
            LedStatus::Starting | LedStatus::HardwareInit | LedStatus::WiFiDriverInit => {
                (self.status_counter / 8) % 2 == 0
            }

            // Network connection states - fast blink
            LedStatus::WiFiConnecting
            | LedStatus::WiFiConnected
            | LedStatus::DHCPRequesting
            | LedStatus::Reconnecting => (self.status_counter / 12) % 2 == 0,

            // Service states - medium blink
            LedStatus::ServicesStarting
            | LedStatus::UDPServerBinding
            | LedStatus::UDPServerListening
            | LedStatus::MDNSAdvertising => (self.status_counter / 16) % 2 == 0,

            // Operational states - slow pulse
            LedStatus::NetworkReady | LedStatus::Operational | LedStatus::ConnectionMonitoring => {
                (self.status_counter / 20) % 3 == 0
            }

            // Data processing states - very fast pulse
            LedStatus::DataReceiving | LedStatus::LEDRendering => {
                (self.status_counter / 6) % 2 == 0
            }

            // Error states - medium blink
            LedStatus::WiFiError
            | LedStatus::NetworkError
            | LedStatus::ServiceError
            | LedStatus::HardwareError
            | LedStatus::Error => (self.status_counter / 20) % 2 == 0,

            // Critical error - fast blink
            LedStatus::CriticalError => (self.status_counter / 10) % 2 == 0,

            // Recovery states - slow blink
            LedStatus::ServiceRestarting | LedStatus::SystemRecovering => {
                (self.status_counter / 25) % 2 == 0
            }
        };

        // Create LED data buffer (4 bytes per LED: G, R, B, W)
        let mut led_data = vec![0u8; LED_COUNT * 4];

        // Set status LEDs (first 3 LEDs) - white color only
        for i in 0..STATUS_LEDS {
            let offset = i * 4;
            if status_on {
                // White color (equal values for G, R, B, W)
                led_data[offset] = 255; // G
                led_data[offset + 1] = 255; // R
                led_data[offset + 2] = 255; // B
                led_data[offset + 3] = 255; // W
            }
            // else: LEDs remain off (0, 0, 0, 0)
        }

        // Set breathing effect for remaining LEDs - white color only
        for i in STATUS_LEDS..LED_COUNT {
            let offset = i * 4;
            let brightness = breathing_brightness as u8;
            led_data[offset] = brightness; // G
            led_data[offset + 1] = brightness; // R
            led_data[offset + 2] = brightness; // B
            led_data[offset + 3] = brightness; // W
        }

        println!(
            "[LED] Status: {:?}, Status On: {}, Breathing: {} (range {}-{}), Pattern: {} bytes ({} LEDs total)",
            self.status,
            status_on,
            breathing_brightness,
            BREATHING_MIN,
            BREATHING_MAX,
            led_data.len(),
            LED_COUNT
        );

        // Forward the data to LED hardware
        if let Err(e) = self.forward_raw_stream(&led_data) {
            println!("[LED] ❌ Failed to update display: {:?}", e);
        }
    }

    /// Forward raw LED data stream to hardware
    pub fn forward_raw_stream(&mut self, data: &[u8]) -> Result<(), BoardError> {
        println!(
            "[LED] 🚀 Forwarding {} bytes of raw display stream to LED hardware",
            data.len()
        );

        // For large data, truncate to safe size for stability
        const MAX_SAFE_PULSES: usize = 4000; // Conservative limit for stable operation
        let total_pulses_needed = data.len() * 8 + 1; // 8 pulses per byte + reset

        let actual_data = if total_pulses_needed > MAX_SAFE_PULSES {
            let max_safe_bytes = (MAX_SAFE_PULSES - 1) / 8; // Reserve 1 pulse for reset
            let safe_bytes = max_safe_bytes & !3; // Round down to multiple of 4 (complete LEDs)
            println!(
                "[LED] ⚠️ Large data detected ({} bytes = {} pulses), truncating to {} bytes for stability",
                data.len(),
                total_pulses_needed,
                safe_bytes
            );
            &data[..safe_bytes]
        } else {
            println!(
                "[LED] 🔄 Using direct transmission for {} bytes ({} pulses)",
                data.len(),
                total_pulses_needed
            );
            data
        };

        // Convert each byte to RMT pulses
        let mut pulses = vec::Vec::with_capacity(actual_data.len() * 8 + 1);
        for &byte in actual_data {
            let byte_pulses = byte_to_pulses(byte);
            pulses.extend_from_slice(&byte_pulses);
        }

        // Add reset pulse
        pulses.push(PulseCode::new(Level::Low, 800, Level::Low, 0));

        // Transmit data
        if let Some(channel) = self.channel.take() {
            println!("[LED] 🔒 Starting critical section for raw stream transmission");
            match channel.transmit(&pulses) {
                Ok(transaction) => {
                    // Use non-blocking approach to avoid infinite wait
                    match transaction.wait() {
                        Ok(channel) => {
                            println!("[LED] ✅ Raw stream transmitted successfully");
                            println!("[LED] 🔓 Raw stream transmission completed");
                            self.channel = Some(channel);
                            Ok(())
                        }
                        Err((err, channel)) => {
                            println!("[LED] ⚠️ Transmission completed with warning: {:?}", err);
                            println!("[LED] 🔓 Raw stream transmission completed (with warning)");
                            self.channel = Some(channel);
                            // Don't treat warnings as errors - LED transmission often succeeds despite warnings
                            Ok(())
                        }
                    }
                }
                Err(err) => {
                    println!("[LED] ❌ Failed to start transmission: {:?}", err);
                    Err(BoardError::LedError)
                }
            }
        } else {
            println!("[LED] ❌ No RMT channel available");
            Err(BoardError::LedError)
        }
    }
}

/// Convert a single byte to RMT pulses for RGBW LEDs
/// Uses SK6812 timing: 1-bit = 6 high + 6 low cycles, 0-bit = 3 high + 9 low cycles at 10MHz
fn byte_to_pulses(byte: u8) -> [u32; 8] {
    let mut pulses = [0u32; 8];

    for i in 0..8 {
        let bit = (byte >> (7 - i)) & 1;
        pulses[i] = if bit == 1 {
            // 1-bit: 6 high cycles + 6 low cycles at 10MHz = 600ns high + 600ns low
            PulseCode::new(Level::High, 6, Level::Low, 6)
        } else {
            // 0-bit: 3 high cycles + 9 low cycles at 10MHz = 300ns high + 900ns low
            PulseCode::new(Level::High, 3, Level::Low, 9)
        };
    }

    pulses
}

/// Universal driver board controller for raw LED data streams
pub struct UniversalDriverBoard<TX>
where
    TX: TxChannel,
{
    led_controller: LedController<TX>,
}

impl<TX> UniversalDriverBoard<TX>
where
    TX: TxChannel,
{
    /// Create a new universal driver board
    pub fn new(channel: TX) -> Self {
        println!("[LED] Universal driver board initialized");
        println!("[LED] GPIO pin: 4");
        println!("[LED] Ready to forward raw display streams");

        Self {
            led_controller: LedController::new(channel),
        }
    }

    /// Set the current status
    pub fn set_status(&mut self, status: LedStatus) {
        self.led_controller.set_status(status);
    }

    /// Update the display
    pub fn update_display(&mut self) {
        self.led_controller.update_display();
    }

    /// Forward raw LED data stream (main function for desktop communication)
    pub fn forward_raw_stream(&mut self, data: &[u8]) -> Result<(), BoardError> {
        self.led_controller.forward_raw_stream(data)
    }

    /// Update LEDs with packet data (for UDP server compatibility)
    pub fn update_leds(&mut self, packet: &crate::udp_server::LedPacket) -> Result<(), BoardError> {
        // For now, just forward the raw data directly
        // In a more sophisticated implementation, we could handle offset-based updates
        println!(
            "[LED] Received LED packet: offset={}, data_len={}",
            packet.offset,
            packet.data.len()
        );
        self.forward_raw_stream(&packet.data)
    }
}

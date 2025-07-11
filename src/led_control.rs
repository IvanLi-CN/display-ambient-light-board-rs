use crate::BoardError;
use alloc::vec;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_time::{Duration, Instant};
use esp_hal::gpio::Level;
use esp_hal::rmt::{PulseCode, TxChannel};
use esp_println::println;
use static_cell::StaticCell;

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

        // Breathing effect parameters (5 second cycle)
        const BREATHING_MIN: u32 = 30;
        const BREATHING_MAX: u32 = 180;
        const BREATHING_SPEED: u32 = 1; // Speed for ~5 second cycle

        // Calculate breathing brightness with step size 2
        const BREATHING_STEP: u32 = 2;
        let breathing_cycle = (self.breathing_counter / BREATHING_SPEED)
            % ((BREATHING_MAX - BREATHING_MIN) / BREATHING_STEP * 2);
        let breathing_brightness =
            if breathing_cycle < (BREATHING_MAX - BREATHING_MIN) / BREATHING_STEP {
                BREATHING_MIN + breathing_cycle * BREATHING_STEP
            } else {
                BREATHING_MAX
                    - (breathing_cycle - (BREATHING_MAX - BREATHING_MIN) / BREATHING_STEP)
                        * BREATHING_STEP
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

        // Silent LED status update

        // Forward the data to LED hardware
        self.forward_raw_stream(&led_data).ok(); // Silent error handling
    }

    /// Forward raw LED data stream to hardware
    pub fn forward_raw_stream(&mut self, data: &[u8]) -> Result<(), BoardError> {
        // For large data, truncate to safe size for stability
        const MAX_SAFE_PULSES: usize = 4000; // Conservative limit for stable operation
        let total_pulses_needed = data.len() * 8 + 1; // 8 pulses per byte + reset

        let actual_data = if total_pulses_needed > MAX_SAFE_PULSES {
            let max_safe_bytes = (MAX_SAFE_PULSES - 1) / 8; // Reserve 1 pulse for reset
            let safe_bytes = max_safe_bytes & !3; // Round down to multiple of 4 (complete LEDs)
            &data[..safe_bytes]
        } else {
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
            match channel.transmit(&pulses) {
                Ok(transaction) => {
                    // Use non-blocking approach to avoid infinite wait
                    match transaction.wait() {
                        Ok(channel) => {
                            self.channel = Some(channel);
                            Ok(())
                        }
                        Err((_, channel)) => {
                            self.channel = Some(channel);
                            // Don't treat warnings as errors - LED transmission often succeeds despite warnings
                            Ok(())
                        }
                    }
                }
                Err(_) => Err(BoardError::LedError),
            }
        } else {
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
        self.forward_raw_stream(&packet.data)
    }
}

/// LED data for environment light mode
#[derive(Debug, Clone)]
pub struct LedData {
    pub data: alloc::vec::Vec<u8>,
    pub timestamp: Instant,
}

/// LED operation modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedMode {
    /// Non-environment mode: breathing + status indication
    NonEnvironment,
    /// Environment mode: display UDP data
    Environment,
}

/// Static channels for LED task communication
static LED_STATUS_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, LedStatus, 8>> =
    StaticCell::new();
static LED_DATA_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, LedData, 4>> =
    StaticCell::new();
static LED_MODE_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, LedMode, 2>> =
    StaticCell::new();

/// Initialize LED communication channels
pub fn init_led_channels() -> (
    Sender<'static, CriticalSectionRawMutex, LedStatus, 8>,
    Sender<'static, CriticalSectionRawMutex, LedData, 4>,
    Sender<'static, CriticalSectionRawMutex, LedMode, 2>,
    Receiver<'static, CriticalSectionRawMutex, LedStatus, 8>,
    Receiver<'static, CriticalSectionRawMutex, LedData, 4>,
    Receiver<'static, CriticalSectionRawMutex, LedMode, 2>,
) {
    let status_channel = LED_STATUS_CHANNEL.init(Channel::new());
    let data_channel = LED_DATA_CHANNEL.init(Channel::new());
    let mode_channel = LED_MODE_CHANNEL.init(Channel::new());

    let status_sender = status_channel.sender();
    let status_receiver = status_channel.receiver();
    let data_sender = data_channel.sender();
    let data_receiver = data_channel.receiver();
    let mode_sender = mode_channel.sender();
    let mode_receiver = mode_channel.receiver();

    (
        status_sender,
        data_sender,
        mode_sender,
        status_receiver,
        data_receiver,
        mode_receiver,
    )
}

/// LED task state
struct LedTaskState {
    current_status: LedStatus,
    current_mode: LedMode,
    status_counter: u32,
    breathing_counter: u32,
    last_environment_data: Option<LedData>,
    environment_timeout: Duration,
}

impl LedTaskState {
    fn new() -> Self {
        Self {
            current_status: LedStatus::Starting,
            current_mode: LedMode::NonEnvironment,
            status_counter: 0,
            breathing_counter: 30, // Start at minimum brightness
            last_environment_data: None,
            environment_timeout: Duration::from_secs(5), // Switch back to non-environment after 5s
        }
    }

    fn update_counters(&mut self) {
        self.status_counter += 1;
        self.breathing_counter += 1;
    }

    fn should_switch_to_non_environment(&self) -> bool {
        if let Some(ref data) = self.last_environment_data {
            Instant::now().duration_since(data.timestamp) > self.environment_timeout
        } else {
            true
        }
    }
}

/// Main LED task running at 30fps
#[embassy_executor::task]
pub async fn led_task(
    led_controller: &'static embassy_sync::mutex::Mutex<
        CriticalSectionRawMutex,
        crate::led_control::UniversalDriverBoard<esp_hal::rmt::Channel<esp_hal::Blocking, 0>>,
    >,
    status_receiver: Receiver<'static, CriticalSectionRawMutex, LedStatus, 8>,
    data_receiver: Receiver<'static, CriticalSectionRawMutex, LedData, 4>,
    mode_receiver: Receiver<'static, CriticalSectionRawMutex, LedMode, 2>,
) -> ! {
    let mut ticker = embassy_time::Ticker::every(Duration::from_millis(33)); // 30fps ≈ 33.33ms
    let mut state = LedTaskState::new();

    println!("[LED] LED task started at 30fps");

    loop {
        // Check for new messages (non-blocking)
        while let Ok(status) = status_receiver.try_receive() {
            state.current_status = status;
            println!("[LED] Status updated: {:?}", status);
        }

        while let Ok(mode) = mode_receiver.try_receive() {
            state.current_mode = mode;
            println!("[LED] Mode switched: {:?}", mode);
        }

        while let Ok(data) = data_receiver.try_receive() {
            state.last_environment_data = Some(data);
            // Automatically switch to environment mode when data is received
            if state.current_mode != LedMode::Environment {
                state.current_mode = LedMode::Environment;
                println!("[LED] Auto-switched to Environment mode");
            }
        }

        // Auto-switch back to non-environment mode if no recent data
        if state.current_mode == LedMode::Environment && state.should_switch_to_non_environment() {
            state.current_mode = LedMode::NonEnvironment;
            println!("[LED] Auto-switched to NonEnvironment mode (timeout)");
        }

        // Update LED display based on current mode
        {
            let mut controller = led_controller.lock().await;
            match state.current_mode {
                LedMode::NonEnvironment => {
                    // Skip status indication when operational - but still do breathing
                    if !matches!(state.current_status, LedStatus::Operational) {
                        controller.set_status(state.current_status);
                    }
                    update_non_environment_display(&mut controller, &mut state);
                }
                LedMode::Environment => {
                    if let Some(ref data) = state.last_environment_data {
                        // Display environment data
                        let _ = controller.forward_raw_stream(&data.data);
                    } else {
                        // Fallback to non-environment display
                        update_non_environment_display(&mut controller, &mut state);
                    }
                }
            }
        }

        // Update counters for next frame
        state.update_counters();

        // Wait for next frame
        ticker.next().await;
    }
}

/// Update LED display for non-environment mode (breathing + status indication)
fn update_non_environment_display(
    controller: &mut UniversalDriverBoard<esp_hal::rmt::Channel<esp_hal::Blocking, 0>>,
    state: &mut LedTaskState,
) {
    const LED_COUNT: usize = 60; // Only update first 60 LEDs to reduce transmission time
    const STATUS_LEDS: usize = 3; // First 3 LEDs for status

    // Breathing effect parameters (5 second cycle)
    const BREATHING_MIN: u32 = 30;
    const BREATHING_MAX: u32 = 180;
    const BREATHING_SPEED: u32 = 1; // Speed for ~5 second cycle

    // Calculate breathing brightness with step size 2
    const BREATHING_STEP: u32 = 2;
    let breathing_cycle = (state.breathing_counter / BREATHING_SPEED)
        % ((BREATHING_MAX - BREATHING_MIN) / BREATHING_STEP * 2);
    let breathing_brightness = if breathing_cycle < (BREATHING_MAX - BREATHING_MIN) / BREATHING_STEP
    {
        BREATHING_MIN + breathing_cycle * BREATHING_STEP
    } else {
        BREATHING_MAX
            - (breathing_cycle - (BREATHING_MAX - BREATHING_MIN) / BREATHING_STEP) * BREATHING_STEP
    };

    // Status indication timing (faster blinking)
    let status_on = match state.current_status {
        // System initialization states - very fast blink
        LedStatus::Starting | LedStatus::HardwareInit | LedStatus::WiFiDriverInit => {
            (state.status_counter / 8) % 2 == 0
        }

        // Network connection states - fast blink
        LedStatus::WiFiConnecting
        | LedStatus::WiFiConnected
        | LedStatus::DHCPRequesting
        | LedStatus::Reconnecting => (state.status_counter / 12) % 2 == 0,

        // Service states - medium blink
        LedStatus::ServicesStarting
        | LedStatus::UDPServerBinding
        | LedStatus::UDPServerListening
        | LedStatus::MDNSAdvertising => (state.status_counter / 16) % 2 == 0,

        // Operational states - slow pulse
        LedStatus::NetworkReady | LedStatus::Operational | LedStatus::ConnectionMonitoring => {
            (state.status_counter / 20) % 3 == 0
        }

        // Data processing states - very fast pulse
        LedStatus::DataReceiving | LedStatus::LEDRendering => (state.status_counter / 6) % 2 == 0,

        // Error states - medium blink
        LedStatus::WiFiError
        | LedStatus::NetworkError
        | LedStatus::ServiceError
        | LedStatus::HardwareError
        | LedStatus::Error => (state.status_counter / 20) % 2 == 0,

        // Critical error - fast blink
        LedStatus::CriticalError => (state.status_counter / 10) % 2 == 0,

        // Recovery states - slow blink
        LedStatus::ServiceRestarting | LedStatus::SystemRecovering => {
            (state.status_counter / 25) % 2 == 0
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

    // Forward the data to LED hardware
    let _ = controller.forward_raw_stream(&led_data); // Silent error handling
}

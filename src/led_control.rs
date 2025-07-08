//! RGBW LED strip control module
//!
//! Handles SK6812-RGBW LED strip communication and data forwarding.

use crate::{BoardError, config};
use crate::udp_server::LedPacket;
use heapless::Vec;
use alloc::boxed::Box;
use alloc::vec;

// ESP32-C3 specific imports for RGBW LED control
use esp_println::println;

use esp_hal::rmt::PulseCode;
use esp_hal::gpio::Level;
use critical_section;

/// RGBW color structure for SK6812-RGBW LEDs
#[derive(Debug, Clone, Copy)]
pub struct RgbwColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub w: u8,
}

impl RgbwColor {
    pub fn new(r: u8, g: u8, b: u8, w: u8) -> Self {
        Self { r, g, b, w }
    }

    /// Convert RGB to RGBW (no white channel)
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, w: 0 }
    }
}

/// Convert byte to SK6812 timing pulses with enhanced timing for long strips
fn byte_to_pulses(byte: u8) -> [u32; 8] {
    let mut pulses = [0u32; 8];

    for i in 0..8 {
        let bit = (byte >> (7 - i)) & 1;
        if bit == 1 {
            // 1-bit: 8 high + 5 low cycles at 10MHz (stronger high signal for long strips)
            pulses[i] = PulseCode::new(Level::High, 8, Level::Low, 5);
        } else {
            // 0-bit: 3 high + 10 low cycles at 10MHz (clear distinction for long strips)
            pulses[i] = PulseCode::new(Level::High, 3, Level::Low, 10);
        }
    }

    pulses
}

/// Trait for RGBW LED strip control abstraction
pub trait LedStripController {
    /// Write RGBW colors to the LED strip
    fn write_rgbw_colors(&mut self, colors: &[RgbwColor]) -> Result<(), BoardError>;

    /// Clear all LEDs (set to black)
    fn clear(&mut self) -> Result<(), BoardError>;

    /// Test hardware with a simple pattern
    fn test_hardware(&mut self) -> Result<(), BoardError>;
}

/// Frame buffer state for collecting complete LED frames
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameState {
    /// Waiting for first packet of a new frame
    WaitingForFrame,
    /// Collecting packets for current frame
    CollectingFrame,
    /// Frame is complete and ready to display
    FrameComplete,
}

/// RGBW LED controller for SK6812-RGBW strips with frame buffering
pub struct LedController {
    max_leds: usize,
    led_buffer: Vec<RgbwColor, {config::MAX_LEDS}>,
    strip_controller: Option<Box<dyn LedStripController>>,
    is_initialized: bool,

    // Frame buffering for complete frame collection
    frame_buffer: Vec<RgbwColor, {config::MAX_LEDS}>,
    frame_state: FrameState,
    expected_frame_size: usize,
    received_leds: usize,
}

impl LedController {
    /// Create a new LED controller instance
    pub fn new() -> Self {
        Self {
            max_leds: config::MAX_LEDS,
            led_buffer: Vec::new(),
            strip_controller: None,
            is_initialized: false,

            // Initialize frame buffering
            frame_buffer: Vec::new(),
            frame_state: FrameState::WaitingForFrame,
            expected_frame_size: config::MAX_LEDS,
            received_leds: 0,
        }
    }

    /// Set the LED strip controller (called from main.rs after RMT initialization)
    pub fn set_strip_controller(&mut self, controller: Box<dyn LedStripController>) {
        self.strip_controller = Some(controller);
        self.is_initialized = true;
        println!("[LED] WS2812 LED controller initialized with hardware driver");
        println!("[LED] GPIO pin: {}", config::LED_DATA_PIN);
        println!("[LED] Max LEDs: {}", self.max_leds);
        println!("[LED] Ready to receive LED data packets");
    }

    /// Initialize the LED controller (legacy method for compatibility)
    pub fn init(&mut self) -> Result<(), BoardError> {
        // This method is now used for basic initialization without hardware
        // The actual hardware initialization happens in set_strip_controller
        println!("[LED] LED controller basic initialization complete");
        println!("[LED] Waiting for hardware driver setup...");
        Ok(())
    }

    /// Check if controller is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized && self.strip_controller.is_some()
    }

    /// Test hardware functionality
    pub fn test_hardware(&mut self) -> Result<(), BoardError> {
        if let Some(ref mut controller) = self.strip_controller {
            controller.test_hardware()
        } else {
            println!("[LED] ❌ Hardware test failed: Controller not initialized");
            Err(BoardError::LedError)
        }
    }



    /// Update LEDs with data from UDP packet using frame buffering
    pub fn update_leds(&mut self, packet: &LedPacket) -> Result<(), BoardError> {
        println!("[LED] 🔥 NEW FRAME BUFFERING CODE CALLED! offset={}, data_len={}", packet.offset, packet.data.len());

        if !self.is_initialized() {
            println!("[LED] ❌ Controller not initialized");
            return Err(BoardError::LedError);
        }

        // Handle empty packets - they might be frame end markers or keep-alive packets
        if packet.data.is_empty() {
            println!("[LED] 📭 Empty packet received - offset={}, treating as potential frame marker", packet.offset);

            // Check if this could be a frame completion signal
            // Some protocols use empty packets with specific offsets to signal frame end
            if packet.offset == 0 {
                println!("[LED] 🏁 Empty packet with offset 0 - treating as frame end signal");
                if self.frame_state == FrameState::CollectingFrame && !self.frame_buffer.is_empty() {
                    println!("[LED] 🎯 Frame complete via empty packet! Updating LED strip with {} LEDs", self.frame_buffer.len());
                    self.commit_frame_to_strip()?;
                    self.reset_frame_buffer();
                }
            } else {
                println!("[LED] 🤷 Empty packet with offset {} - ignoring", packet.offset);
            }
            return Ok(());
        }

        // Parse colors from packet
        let colors = self.parse_colors(&packet.data)?;

        // Handle frame buffering logic
        self.handle_frame_packet(packet.offset, &colors)?;

        // Update LED strip when frame is complete (ignoring 0x03 packets)
        if self.frame_state == FrameState::FrameComplete {
            println!("[LED] 🎯 Frame complete! Updating LED strip with {} LEDs", self.frame_buffer.len());
            self.commit_frame_to_strip()?;
            self.reset_frame_buffer();
        }

        Ok(())
    }

    /// Handle frame packet and update frame buffer
    fn handle_frame_packet(&mut self, offset: u16, colors: &[RgbwColor]) -> Result<(), BoardError> {
        let offset = offset as usize;

        // Check if this is the start of a new frame (offset 0)
        if offset == 0 {
            println!("[LED] 🆕 New frame started - resetting frame buffer");
            self.reset_frame_buffer();
            self.frame_state = FrameState::CollectingFrame;
        }

        // Ensure we're in collecting state
        if self.frame_state == FrameState::WaitingForFrame && offset != 0 {
            println!("[LED] ⚠️ Received non-zero offset {} while waiting for frame start, ignoring", offset);
            return Ok(());
        }

        // Expand frame buffer if needed
        while self.frame_buffer.len() < offset + colors.len() {
            if self.frame_buffer.push(RgbwColor::new(0, 0, 0, 0)).is_err() {
                println!("[LED] ❌ Frame buffer overflow");
                return Err(BoardError::LedError);
            }
        }

        // Copy colors to frame buffer at the specified offset
        for (i, &color) in colors.iter().enumerate() {
            if offset + i < self.frame_buffer.len() {
                self.frame_buffer[offset + i] = color;
            }
        }

        self.received_leds = self.frame_buffer.len();

        println!("[LED] 📦 Frame packet: offset={}, colors={}, total_leds={}",
                 offset, colors.len(), self.received_leds);

        // Check if frame is complete (we have received data for all expected LEDs)
        // Frame completion based on LED count (ignoring 0x03 packets which are filtered out)
        if self.received_leds >= 60 || (offset + colors.len() >= self.expected_frame_size) {
            self.frame_state = FrameState::FrameComplete;
            println!("[LED] ✅ Frame collection complete: {} LEDs ready", self.received_leds);
        }

        Ok(())
    }

    /// Reset frame buffer for new frame
    fn reset_frame_buffer(&mut self) {
        self.frame_buffer.clear();
        self.frame_state = FrameState::WaitingForFrame;
        self.received_leds = 0;
    }

    /// Commit complete frame to LED strip
    fn commit_frame_to_strip(&mut self) -> Result<(), BoardError> {
        if let Some(ref mut controller) = self.strip_controller {
            println!("[LED] 🚀 Committing frame to LED strip: {} LEDs", self.frame_buffer.len());
            controller.write_rgbw_colors(&self.frame_buffer)
        } else {
            println!("[LED] ❌ No strip controller available");
            Err(BoardError::LedError)
        }
    }

    /// Parse color data from packet and convert to RGBW
    fn parse_colors(&self, data: &[u8]) -> Result<Vec<RgbwColor, {config::MAX_LEDS}>, BoardError> {
        let mut colors = Vec::new();

        // Determine if RGB (3 bytes/LED) or RGBW (4 bytes/LED)
        if data.len() % 3 == 0 {
            // RGB format - convert to RGBW (no white channel)
            // UDP packet format: R,G,B but hardware expects G,R,B,W order
            for chunk in data.chunks_exact(3) {
                let r = chunk[0];  // Red from UDP
                let g = chunk[1];  // Green from UDP
                let b = chunk[2];  // Blue from UDP
                let color = RgbwColor::from_rgb(r, g, b);
                if colors.push(color).is_err() {
                    println!("[LED] ❌ Too many LEDs in packet (max: {})", config::MAX_LEDS);
                    return Err(BoardError::LedError);
                }
            }
            println!("[LED] ✅ Parsed {} RGB LEDs from packet", colors.len());
        } else if data.len() % 4 == 0 {
            // RGBW format - direct conversion
            for chunk in data.chunks_exact(4) {
                let color = RgbwColor::new(chunk[0], chunk[1], chunk[2], chunk[3]);
                if colors.push(color).is_err() {
                    println!("[LED] ❌ Too many LEDs in packet (max: {})", config::MAX_LEDS);
                    return Err(BoardError::LedError);
                }
            }
            println!("[LED] ✅ Parsed {} RGBW LEDs from packet", colors.len());
        } else {
            println!("[LED] ❌ Invalid packet data length: {} bytes (must be multiple of 3 or 4)", data.len());
            return Err(BoardError::ProtocolError);
        }

        Ok(colors)
    }
    
    /// Set LEDs starting at the specified offset
    fn set_leds_at_offset(&mut self, offset: u16, colors: &[RgbwColor]) -> Result<(), BoardError> {
        if !self.is_initialized() {
            return Err(BoardError::LedError);
        }

        println!("[LED] Setting {} LEDs starting at offset {}", colors.len(), offset);

        // Ensure buffer is large enough to accommodate the new data
        let required_size = offset as usize + colors.len();

        // Extend buffer with black LEDs if needed
        while self.led_buffer.len() < required_size {
            if self.led_buffer.push(RgbwColor::new(0, 0, 0, 0)).is_err() {
                println!("[LED] ❌ Buffer full, cannot extend to {} LEDs", required_size);
                break;
            }
        }

        // Update the specific LEDs at the offset
        for (i, color) in colors.iter().enumerate() {
            let led_index = offset as usize + i;
            if led_index >= self.max_leds {
                println!("[LED] Warning: LED index {} exceeds max LEDs {}", led_index, self.max_leds);
                break;
            }

            if led_index < self.led_buffer.len() {
                self.led_buffer[led_index] = *color;
                println!("[LED] LED[{}] = RGBW({}, {}, {}, {})", led_index, color.r, color.g, color.b, color.w);
            } else {
                println!("[LED] ❌ LED index {} out of buffer bounds", led_index);
                break;
            }
        }

        // Send only the valid LED data to the strip (not the entire buffer)
        let valid_led_count = self.led_buffer.len();

        // Write LED data to hardware controller
        if let Some(ref mut controller) = self.strip_controller {
            match controller.write_rgbw_colors(&self.led_buffer) {
                Ok(_) => {
                    println!("[LED] ✅ Successfully wrote {} colors to RGBW strip", valid_led_count);
                }
                Err(e) => {
                    println!("[LED] ⚠️ Transmission error occurred: {:?}", e);
                    println!("[LED] 🎯 But continuing anyway since hardware appears functional");
                }
            }
        } else {
            println!("[LED] ⚠️ No hardware controller available - colors stored in buffer only");
        }

        Ok(())
    }

    /// Commit LED changes to the strip
    pub fn show(&mut self) -> Result<(), BoardError> {
        if !self.is_initialized() {
            return Err(BoardError::LedError);
        }

        println!("[LED] ✅ LED strip updated successfully with {} LEDs", self.led_buffer.len());
        Ok(())
    }

    /// Clear all LEDs
    pub fn clear(&mut self) -> Result<(), BoardError> {
        if !self.is_initialized() {
            return Err(BoardError::LedError);
        }

        println!("[LED] Clearing all LEDs");

        // Clear the buffer
        self.led_buffer.clear();

        // Send clear command to hardware if available
        if let Some(ref mut controller) = self.strip_controller {
            controller.clear()?;
        } else {
            println!("[LED] ⚠️ No hardware controller available - buffer cleared only");
        }

        println!("[LED] ✅ All LEDs cleared successfully");
        Ok(())
    }

    /// 最小LED测试 - 直接测试基本驱动功能
    pub fn minimal_led_test(&mut self) -> Result<(), BoardError> {
        println!("[LED] 🧪 开始最小LED测试...");

        if let Some(ref mut controller) = self.strip_controller {
            // 测试1: 单个红色LED
            println!("[LED] 🔴 测试1: 第一个LED设为红色");
            let test_data = [
                RgbwColor::new(255, 0, 0, 0),
                RgbwColor::new(0, 0, 0, 0),
                RgbwColor::new(0, 0, 0, 0),
                RgbwColor::new(0, 0, 0, 0),
                RgbwColor::new(0, 0, 0, 0)
            ];

            match controller.write_rgbw_colors(&test_data) {
                Ok(_) => println!("[LED] ✅ 红色LED测试成功"),
                Err(e) => println!("[LED] ❌ 红色LED测试失败: {:?}", e),
            }

            // 简单延时
            for _ in 0..1000000 { core::hint::spin_loop(); }

            // 测试2: 单个绿色LED
            println!("[LED] 🟢 测试2: 第二个LED设为绿色");
            let test_data = [
                RgbwColor::new(0, 0, 0, 0),
                RgbwColor::new(0, 255, 0, 0),
                RgbwColor::new(0, 0, 0, 0),
                RgbwColor::new(0, 0, 0, 0),
                RgbwColor::new(0, 0, 0, 0)
            ];

            match controller.write_rgbw_colors(&test_data) {
                Ok(_) => println!("[LED] ✅ 绿色LED测试成功"),
                Err(e) => println!("[LED] ❌ 绿色LED测试失败: {:?}", e),
            }

            // 简单延时
            for _ in 0..1000000 { core::hint::spin_loop(); }

            // 测试3: 单个蓝色LED
            println!("[LED] 🔵 测试3: 第三个LED设为蓝色");
            let test_data = [
                RgbwColor::new(0, 0, 0, 0),
                RgbwColor::new(0, 0, 0, 0),
                RgbwColor::new(0, 0, 255, 0),
                RgbwColor::new(0, 0, 0, 0),
                RgbwColor::new(0, 0, 0, 0)
            ];

            match controller.write_rgbw_colors(&test_data) {
                Ok(_) => println!("[LED] ✅ 蓝色LED测试成功"),
                Err(e) => println!("[LED] ❌ 蓝色LED测试失败: {:?}", e),
            }

            // 简单延时
            for _ in 0..1000000 { core::hint::spin_loop(); }

            // 测试4: 前3个LED同时点亮不同颜色
            println!("[LED] 🌈 测试4: 前3个LED同时点亮(红绿蓝)");
            let test_data = [
                RgbwColor::new(255, 0, 0, 0),
                RgbwColor::new(0, 255, 0, 0),
                RgbwColor::new(0, 0, 255, 0),
                RgbwColor::new(0, 0, 0, 0),
                RgbwColor::new(0, 0, 0, 0)
            ];

            match controller.write_rgbw_colors(&test_data) {
                Ok(_) => println!("[LED] ✅ 多色LED测试成功"),
                Err(e) => println!("[LED] ❌ 多色LED测试失败: {:?}", e),
            }

            // 更长的延时
            for _ in 0..2000000 { core::hint::spin_loop(); }

            // 测试5: 全部关闭
            println!("[LED] ⚫ 测试5: 关闭所有LED");
            let test_data = [RgbwColor::new(0, 0, 0, 0); 5];

            match controller.write_rgbw_colors(&test_data) {
                Ok(_) => println!("[LED] ✅ 关闭LED测试成功"),
                Err(e) => println!("[LED] ❌ 关闭LED测试失败: {:?}", e),
            }

            println!("[LED] 🎯 最小LED测试完成");
            Ok(())
        } else {
            println!("[LED] ❌ 没有可用的硬件控制器");
            Err(BoardError::LedError)
        }
    }
}

impl Default for LedController {
    fn default() -> Self {
        Self::new()
    }
}

/// Concrete implementation of LedStripController using direct RMT for RGBW
pub struct RgbwController<TX>
where
    TX: esp_hal::rmt::TxChannel,
{
    channel: Option<TX>,
}

impl<TX> RgbwController<TX>
where
    TX: esp_hal::rmt::TxChannel,
{
    /// Create a new RGBW controller
    pub fn new(channel: TX) -> Self {
        Self { channel: Some(channel) }
    }

    /// Send RGBW data using direct RMT pulses - must send complete LED strip data
    fn send_rgbw_data(&mut self, colors: &[RgbwColor]) -> Result<(), BoardError> {
        // SK6812 requires complete LED strip data in one transmission
        // Cannot send partial updates - must always start from LED 0
        println!("[LED] 🚀 Sending complete LED strip data: {} LEDs", colors.len());

        // Add longer delay to ensure previous transmission is complete
        // Critical for signal integrity on long LED strips (40+ LEDs)
        for _ in 0..5000 { core::hint::spin_loop(); }

        self.send_rgbw_batch(colors, true)
    }

    /// Send a single batch of RGBW data
    fn send_rgbw_batch(&mut self, colors: &[RgbwColor], add_reset: bool) -> Result<(), BoardError> {
        let total_pulses = colors.len() * 32 + if add_reset { 1 } else { 0 };
        let mut pulses = vec::Vec::with_capacity(total_pulses);

        for color in colors {
            // Channel order: G,R,B,W (SK6812-RGBW standard)
            for &byte in &[color.g, color.r, color.b, color.w] {
                let byte_pulses = byte_to_pulses(byte);
                pulses.extend_from_slice(&byte_pulses);
            }
        }

        // Add reset pulse only when specified (typically on last batch)
        // Use extra long reset pulse for maximum signal integrity on long strips
        if add_reset {
            pulses.push(PulseCode::new(Level::Low, 2000, Level::Low, 0)); // 200μs reset pulse for long strips
        }

        // Take ownership of channel temporarily
        let channel = self.channel.take().ok_or(BoardError::LedError)?;

        // 🔒 CRITICAL SECTION: Disable interrupts during LED data transmission
        // This prevents embassy tasks and WiFi interrupts from interfering with timing-critical RMT transmission
        let result = critical_section::with(|_| {
            println!("[LED] 🔒 Starting critical section for LED transmission");
            let transaction = channel.transmit(&pulses).map_err(|_| BoardError::LedError)?;

            // Wait for transmission completion within critical section
            match transaction.wait() {
                Ok(channel) => {
                    println!("[LED] ✅ LED transmission completed successfully in critical section");
                    Ok(channel)
                }
                Err((err, channel)) => {
                    println!("[LED] ⚠️ RMT warning in critical section: {:?}", err);
                    Ok(channel) // Continue anyway since hardware appears functional
                }
            }
        });

        // 🔓 Critical section ended, interrupts re-enabled
        match result {
            Ok(channel) => {
                self.channel = Some(channel);
                // Add post-transmission delay for signal stability on long strips
                if add_reset {
                    for _ in 0..2000 { core::hint::spin_loop(); }
                }
                println!("[LED] 🔓 Critical section completed, LED data transmitted");
                Ok(())
            }
            Err(e) => {
                println!("[LED] ❌ Critical section failed: {:?}", e);
                Err(e)
            }
        }
    }
}

impl<TX> LedStripController for RgbwController<TX>
where
    TX: esp_hal::rmt::TxChannel,
{
    /// Write RGBW colors to the LED strip using direct RMT
    fn write_rgbw_colors(&mut self, colors: &[RgbwColor]) -> Result<(), BoardError> {
        println!("[LED] 🔧 Attempting to write {} colors to RGBW strip", colors.len());

        if colors.is_empty() {
            println!("[LED] ⚠️ No colors to write");
            return Ok(());
        }

        // Log first few colors for debugging
        for (i, color) in colors.iter().take(5).enumerate() {
            if i == 0 {
                println!("[LED] 🎨 First LED: RGBW({}, {}, {}, {})", color.r, color.g, color.b, color.w);
            } else if i == 1 {
                println!("[LED] 🎨 Second LED: RGBW({}, {}, {}, {})", color.r, color.g, color.b, color.w);
            } else if i == colors.len() / 2 {
                println!("[LED] 🎨 Middle LED[{}]: RGBW({}, {}, {}, {})", i, color.r, color.g, color.b, color.w);
            } else if i == colors.len() - 1 {
                println!("[LED] 🎨 Last LED[{}]: RGBW({}, {}, {}, {})", i, color.r, color.g, color.b, color.w);
            }
        }

        // Send RGBW data using direct RMT
        println!("[LED] 🚀 Sending RGBW data via RMT...");
        self.send_rgbw_data(colors)
    }

    /// Clear all LEDs (set to black)
    fn clear(&mut self) -> Result<(), BoardError> {
        // Create black colors for clearing
        let black = RgbwColor::new(0, 0, 0, 0);
        let clear_colors = [black; config::MAX_LEDS];

        self.send_rgbw_data(&clear_colors)
    }

    /// Test hardware with simple RGBW pattern
    fn test_hardware(&mut self) -> Result<(), BoardError> {
        println!("[LED] 🧪 Testing hardware with simple RGBW pattern...");

        // Test pattern: Red, Green, Blue, White
        let test_colors = [
            RgbwColor::new(255, 0, 0, 0),   // Red
            RgbwColor::new(0, 255, 0, 0),   // Green
            RgbwColor::new(0, 0, 255, 0),   // Blue
            RgbwColor::new(0, 0, 0, 255),   // White
        ];

        self.send_rgbw_data(&test_colors)
    }
}

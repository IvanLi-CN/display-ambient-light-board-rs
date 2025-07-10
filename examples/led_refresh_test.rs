//! RGBW LED Refresh Test Firmware - 500 LEDs Refresh Test
//!
//! Tests 500 RGBW LEDs with fixed color pattern refreshed every 500ms:
//! White, Yellow, Cyan, Green, Magenta, Red, Blue, Black (repeating)
//! This test checks if the LED driver has flickering issues with repeated refreshes.
//!
//! Hardware: SK6812-RGBW LEDs, GPIO4, G,R,B,W channel order

#![no_std]
#![no_main]

extern crate alloc;
use esp_alloc as _;

use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    rmt::{PulseCode, Rmt, TxChannelConfig, TxChannelCreator},
    time::Rate,
};
use esp_println::println;

// Add app descriptor for espflash compatibility
esp_bootloader_esp_idf::esp_app_desc!();

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    println!("Panic occurred: {:?}", _info);
    loop {}
}

#[derive(Clone, Copy)]
struct RgbwColor {
    r: u8,
    g: u8,
    b: u8,
    w: u8,
}

impl RgbwColor {
    const fn new(r: u8, g: u8, b: u8, w: u8) -> Self {
        Self { r, g, b, w }
    }
}

fn byte_to_pulses(byte: u8) -> [u32; 8] {
    let mut pulses = [0u32; 8];
    for i in 0..8 {
        let bit = (byte >> (7 - i)) & 1;
        if bit == 1 {
            pulses[i] = PulseCode::new(Level::High, 6, Level::Low, 6);
        } else {
            pulses[i] = PulseCode::new(Level::High, 3, Level::Low, 9);
        }
    }
    pulses
}

fn send_rgbw_data<T>(channel: T, colors: &[RgbwColor]) -> Result<T, esp_hal::rmt::Error>
where
    T: esp_hal::rmt::TxChannel,
{
    let total_pulses = colors.len() * 32 + 1;
    let mut pulses = alloc::vec::Vec::with_capacity(total_pulses);

    for color in colors {
        // Channel order: G,R,B,W
        for &byte in &[color.g, color.r, color.b, color.w] {
            let byte_pulses = byte_to_pulses(byte);
            pulses.extend_from_slice(&byte_pulses);
        }
    }

    pulses.push(PulseCode::new(Level::Low, 800, Level::Low, 0));

    let transaction = channel.transmit(&pulses)?;
    match transaction.wait() {
        Ok(channel) => Ok(channel),
        Err((err, channel)) => {
            println!("⚠️ RMT warning: {:?}", err);
            Ok(channel)
        }
    }
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(size: 128 * 1024);

    println!("🚀 500 LEDs Refresh Test - Every 500ms");

    let led_pin = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default())
        .into_peripheral_output();

    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(10)).unwrap();
    let tx_config = TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output_level(Level::Low)
        .with_idle_output(false)
        .with_carrier_modulation(false);

    let mut channel = rmt.channel0.configure(led_pin, tx_config).unwrap();

    // 8 colors: White, Yellow, Cyan, Green, Magenta, Red, Blue, Black
    // Note: Hardware uses G,R,B,W channel order, so RgbwColor::new(r,g,b,w) maps to actual G,R,B,W
    let colors = [
        RgbwColor::new(0, 0, 0, 255),   // White (using W channel)
        RgbwColor::new(255, 255, 0, 0), // Yellow (R+G)
        RgbwColor::new(0, 255, 255, 0), // Cyan (G+B)
        RgbwColor::new(0, 255, 0, 0),   // Green (G only)
        RgbwColor::new(255, 0, 255, 0), // Magenta (R+B)
        RgbwColor::new(255, 0, 0, 0),   // Red (R only)
        RgbwColor::new(0, 0, 255, 0),   // Blue (B only)
        RgbwColor::new(0, 0, 0, 0),     // Black (all off)
    ];

    println!("🌈 Generating 500 LEDs with 8-color cycle");

    let mut led_data = alloc::vec::Vec::with_capacity(500);
    for i in 0..500 {
        led_data.push(colors[i % colors.len()]);
    }

    let delay = Delay::new();
    let mut refresh_count = 0u32;

    println!("🔄 Starting 500ms refresh cycle...");

    loop {
        refresh_count += 1;
        println!("🔥 Refresh #{}: Sending data to 500 LEDs...", refresh_count);

        channel = match send_rgbw_data(channel, &led_data) {
            Ok(ch) => {
                println!(
                    "✅ Refresh #{}: 500 LEDs data sent successfully",
                    refresh_count
                );
                ch
            }
            Err(e) => {
                println!("❌ Refresh #{}: Failed: {:?}", refresh_count, e);
                loop {}
            }
        };

        // Wait 500ms before next refresh
        delay.delay_millis(500);
    }
}

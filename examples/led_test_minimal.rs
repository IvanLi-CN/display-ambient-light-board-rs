//! 最小LED测试程序 - 独立测试WS2812驱动功能
//!
//! 这个程序专门用于测试WS2812 LED驱动的基本功能，
//! 不包含WiFi、网络或其他复杂功能。

#![no_std]
#![no_main]

extern crate alloc;

use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    rmt::{PulseCode, Rmt, TxChannelConfig, TxChannelCreator},
    time::Rate,
};
use esp_println::println;

// Add app descriptor for espflash compatibility
esp_bootloader_esp_idf::esp_app_desc!();

/// RGBW颜色结构体
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

/// 将单个字节转换为WS2812时序脉冲
fn byte_to_pulses(byte: u8) -> [u32; 8] {
    let mut pulses = [0u32; 8];
    for i in 0..8 {
        let bit = (byte >> (7 - i)) & 1;
        if bit == 1 {
            // 1位: 高电平0.8us, 低电平0.45us (在10MHz时钟下: 8个周期高, 4个周期低)
            pulses[i] = PulseCode::new(Level::High, 8, Level::Low, 4);
        } else {
            // 0位: 高电平0.4us, 低电平0.85us (在10MHz时钟下: 4个周期高, 8个周期低)
            pulses[i] = PulseCode::new(Level::High, 4, Level::Low, 8);
        }
    }
    pulses
}

/// 发送RGBW数据到LED灯条
fn send_rgbw_data<T>(channel: &mut T, colors: &[RgbwColor]) -> Result<(), esp_hal::rmt::Error>
where
    T: esp_hal::rmt::TxChannel,
{
    // 计算需要的脉冲数量: 每个RGBW LED需要32个脉冲 (4字节 * 8位)
    let total_pulses = colors.len() * 32 + 1; // +1 for reset pulse
    let mut pulses = alloc::vec::Vec::with_capacity(total_pulses);

    for color in colors {
        // RGBW顺序: G, R, B, W (SK6812-RGBW标准顺序)
        for &byte in &[color.g, color.r, color.b, color.w] {
            let byte_pulses = byte_to_pulses(byte);
            pulses.extend_from_slice(&byte_pulses);
        }
    }

    // 添加复位脉冲 (低电平50us = 500个周期在10MHz)
    pulses.push(PulseCode::new(Level::Low, 500, Level::Low, 0));

    channel.transmit(&pulses)?.wait()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[esp_hal::main]
fn main() -> ! {
    let config = esp_hal::Config::default();
    let peripherals = esp_hal::init(config);

    println!("🚀 最小LED测试程序启动");
    println!("📍 使用GPIO4作为LED数据引脚");

    // 基本GPIO测试
    println!("🔧 测试GPIO4基本功能...");
    let mut gpio_test = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());
    for i in 1..=5 {
        gpio_test.set_high();
        println!("GPIO4 HIGH ({})", i);
        // 简单延时
        for _ in 0..1000000 {
            core::hint::spin_loop();
        }

        gpio_test.set_low();
        println!("GPIO4 LOW ({})", i);
        for _ in 0..1000000 {
            core::hint::spin_loop();
        }
    }
    println!("✅ GPIO4基本测试完成");

    // 转换引脚用于RMT
    let led_pin = gpio_test.into_peripheral_output();

    // 初始化RMT外设
    println!("🔧 初始化RMT外设...");
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(10)).unwrap();

    // 配置RMT通道用于RGBW LED控制
    let tx_config = TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output_level(Level::Low)
        .with_idle_output(false)
        .with_carrier_modulation(false);

    let mut channel = rmt.channel0.configure(led_pin, tx_config).unwrap();

    println!("✅ RMT通道配置完成，准备发送RGBW数据");

    let delay = Delay::new();

    // 定义RGBW测试颜色（正确的4字节格式）
    let red = RgbwColor::new(255, 0, 0, 0); // 纯红色
    let green = RgbwColor::new(0, 255, 0, 0); // 纯绿色
    let blue = RgbwColor::new(0, 0, 255, 0); // 纯蓝色
    let white = RgbwColor::new(0, 0, 0, 255); // 纯白色（使用W通道）
    let black = RgbwColor::new(0, 0, 0, 0); // 关闭

    println!("🧪 开始LED测试循环...");

    loop {
        // 测试1: 单个红色LED
        println!("🔴 测试红色LED...");
        let rgbw_colors = [red, black, black];
        let mut rgb_sequence = [RGB8::default(); 12];
        for (i, rgbw) in rgbw_colors.iter().enumerate() {
            let rgb_seq = rgbw.to_rgb_sequence();
            rgb_sequence[i * 4..(i + 1) * 4].copy_from_slice(&rgb_seq);
        }
        match led_adapter.write(rgb_sequence.iter().cloned()) {
            Ok(_) => println!("✅ 红色LED写入成功"),
            Err(e) => println!("❌ 红色LED写入失败: {:?}", e),
        }
        delay.delay_millis(2000);

        // 测试2: 单个绿色LED
        println!("🟢 测试绿色LED...");
        let rgbw_colors = [black, green, black];
        let mut rgb_sequence = [RGB8::default(); 12];
        for (i, rgbw) in rgbw_colors.iter().enumerate() {
            let rgb_seq = rgbw.to_rgb_sequence();
            rgb_sequence[i * 4..(i + 1) * 4].copy_from_slice(&rgb_seq);
        }
        match led_adapter.write(rgb_sequence.iter().cloned()) {
            Ok(_) => println!("✅ 绿色LED写入成功"),
            Err(e) => println!("❌ 绿色LED写入失败: {:?}", e),
        }
        delay.delay_millis(2000);

        // 测试3: 单个蓝色LED
        println!("🔵 测试蓝色LED...");
        let rgbw_colors = [black, black, blue];
        let mut rgb_sequence = [RGB8::default(); 12];
        for (i, rgbw) in rgbw_colors.iter().enumerate() {
            let rgb_seq = rgbw.to_rgb_sequence();
            rgb_sequence[i * 4..(i + 1) * 4].copy_from_slice(&rgb_seq);
        }
        match led_adapter.write(rgb_sequence.iter().cloned()) {
            Ok(_) => println!("✅ 蓝色LED写入成功"),
            Err(e) => println!("❌ 蓝色LED写入失败: {:?}", e),
        }
        delay.delay_millis(2000);

        // 测试4: 多色显示
        println!("🌈 测试多色显示...");
        let rgbw_colors = [red, green, blue];
        let mut rgb_sequence = [RGB8::default(); 12];
        for (i, rgbw) in rgbw_colors.iter().enumerate() {
            let rgb_seq = rgbw.to_rgb_sequence();
            rgb_sequence[i * 4..(i + 1) * 4].copy_from_slice(&rgb_seq);
        }
        match led_adapter.write(rgb_sequence.iter().cloned()) {
            Ok(_) => println!("✅ 多色LED写入成功"),
            Err(e) => println!("❌ 多色LED写入失败: {:?}", e),
        }
        delay.delay_millis(2000);

        // 测试5: 全白（使用W通道）
        println!("⚪ 测试全白LED（W通道）...");
        let rgbw_colors = [white, white, white];
        let mut rgb_sequence = [RGB8::default(); 12];
        for (i, rgbw) in rgbw_colors.iter().enumerate() {
            let rgb_seq = rgbw.to_rgb_sequence();
            rgb_sequence[i * 4..(i + 1) * 4].copy_from_slice(&rgb_seq);
        }
        match led_adapter.write(rgb_sequence.iter().cloned()) {
            Ok(_) => println!("✅ 全白LED写入成功"),
            Err(e) => println!("❌ 全白LED写入失败: {:?}", e),
        }
        delay.delay_millis(2000);

        // 测试6: 关闭所有LED
        println!("⚫ 关闭所有LED...");
        let rgbw_colors = [black, black, black];
        let mut rgb_sequence = [RGB8::default(); 12];
        for (i, rgbw) in rgbw_colors.iter().enumerate() {
            let rgb_seq = rgbw.to_rgb_sequence();
            rgb_sequence[i * 4..(i + 1) * 4].copy_from_slice(&rgb_seq);
        }
        match led_adapter.write(rgb_sequence.iter().cloned()) {
            Ok(_) => println!("✅ LED关闭成功"),
            Err(e) => println!("❌ LED关闭失败: {:?}", e),
        }
        delay.delay_millis(2000);

        println!("🔄 测试循环完成，重新开始...");
        delay.delay_millis(1000);
    }
}

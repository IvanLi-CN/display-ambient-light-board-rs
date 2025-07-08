# Board-RS: ESP32-C3 WiFi LED Controller

一个基于 ESP32-C3 的 WiFi 控制 LED 硬件通信桥，使用 Rust 嵌入式开发。

## 项目概述

Board-RS 是一个完整的 WiFi 启用的 LED 硬件通信桥，接收 UDP 数据包并将其转发到 WS2812 LED 灯带。该项目使用纯 esp-hal 方法，避免使用 ESP-IDF 组件。

## 功能特性

### ✅ 已实现功能

- **WiFi 连接管理**: 自动连接、DHCP 客户端、连接监控和自动重连
- **mDNS 服务发现**: 自动服务广告，便于设备发现
- **UDP 通信服务器**: 监听端口 23042，接收 LED 数据包
- **WS2812 LED 控制**: 支持 RGB/RGBW LED 灯带控制
- **集成测试套件**: 全面的系统集成测试
- **性能监控**: 实时性能指标和系统健康监控
- **错误恢复**: 强大的错误处理和恢复机制

### 🔧 技术规格

- **目标硬件**: ESP32-C3 (RISC-V)
- **开发语言**: Rust (no_std)
- **HAL**: esp-hal 1.0.0-beta.1
- **WiFi**: esp-wifi 0.14.1 (无 embassy-net)
- **LED 控制**: esp-hal-smartled + smart-leds
- **mDNS**: edge-mdns 0.6.0
- **内存管理**: esp-alloc (72KB 堆)

## Hardware Requirements

- ESP32-C3 development board
- WS2812/WS2812B LED strips
- Adequate power supply for LED strips
- WiFi network connectivity

## Protocol Support

Supports the ambient light hardware communication protocol:
- **Header**: 0x02
- **Offset**: 16-bit LED start position (big-endian)
- **Data**: RGB (3 bytes/LED) or RGBW (4 bytes/LED) color data
- **Port**: UDP 23042

## Build Requirements

- Rust with ESP32 target support
- ESP-IDF toolchain
- `espflash` for flashing firmware

## Quick Start

1. **Install Dependencies**:
   ```bash
   # Install Rust ESP32 toolchain
   cargo install espup
   espup install
   
   # Install flashing tool
   cargo install espflash
   ```

2. **Configure WiFi Credentials**:
   ```bash
   # Copy the example environment file
   cp .env.example .env

   # Edit .env and set your WiFi credentials
   # WIFI_SSID=your_wifi_network_name
   # WIFI_PASSWORD=your_wifi_password
   ```

   Alternatively, you can set environment variables directly:
   ```bash
   export WIFI_SSID="your_wifi_network_name"
   export WIFI_PASSWORD="your_wifi_password"
   ```

3. **Build Firmware**:
   ```bash
   cargo build --release
   ```

3. **Flash to Device**:
   ```bash
   cargo run --release
   ```

4. **Configure WiFi**: Update WiFi credentials in the source code before building.

## Project Structure

```
board-rs/
├── src/                     # Core source code
│   ├── main.rs             # Main application entry point
│   ├── lib.rs              # Library modules
│   ├── led_control.rs      # LED control and color processing
│   ├── wifi.rs             # WiFi management
│   ├── udp_server.rs       # UDP communication server
│   └── mdns.rs             # mDNS service discovery
├── examples/               # Hardware test examples
│   ├── led_refresh_test.rs # LED stability test
│   └── led_test_minimal.rs # Basic LED functionality test
├── docs/                   # Technical documentation
│   ├── ARCHITECTURE.md     # System architecture overview
│   ├── COLOR_DATA_PROCESSING.md # Color data flow analysis
│   └── README.md           # Documentation index
├── tools/                  # Testing and utility scripts
│   ├── test_udp.py         # UDP communication test
│   └── README.md           # Tools documentation
├── tests/                  # Unit tests
│   └── hello_test.rs       # Basic test suite
├── Cargo.toml              # Project dependencies
├── build.rs                # Build script
├── rust-toolchain.toml     # Rust toolchain specification
└── README.md               # This file
```

## Configuration

### WiFi Settings
Update the WiFi credentials in `src/bin/main.rs`:
```rust
const WIFI_SSID: &str = "your_wifi_network";
const WIFI_PASSWORD: &str = "your_wifi_password";
```

### LED Configuration
Configure the LED data pin and strip parameters:
```rust
const LED_DATA_PIN: u8 = 2;  // GPIO pin for WS2812 data
const MAX_LEDS: usize = 1000; // Maximum supported LEDs
```

## Development

### Building
```bash
# Debug build
cargo build

# Release build (recommended for deployment)
cargo build --release
```

### Flashing
```bash
# Flash and monitor
cargo run --release

# Flash only
espflash flash target/riscv32imc-unknown-none-elf/release/board-rs
```

### Monitoring
```bash
# Monitor serial output
espflash monitor
```

## Testing

### Network Discovery
Test mDNS discovery from desktop:
```bash
# Linux/macOS
avahi-browse -t _ambient_light._udp.local.

# Windows
dns-sd -B _ambient_light._udp
```

### UDP Communication
Use the provided test script for easy LED testing:
```bash
# Test LED functionality with Python script
python3 tools/test_udp.py <board_ip>

# Or use manual UDP commands
echo -ne '\x02\x00\x00\xFF\x00\x00\x00\xFF\x00\x00\x00\xFF' | nc -u <board_ip> 23042
```

## Troubleshooting

### WiFi Connection Issues
- Verify SSID and password are correct
- Check WiFi network compatibility (2.4GHz required)
- Monitor serial output for connection status

### LED Control Issues
- Verify WS2812 wiring and power supply
- Check GPIO pin configuration
- Test with simple color patterns

### Network Discovery Issues
- Ensure mDNS is enabled on the network
- Check firewall settings for UDP port 23042
- Verify board IP address assignment

## Performance

- **Update Rate**: Supports high-frequency LED updates
- **LED Count**: Tested with up to 1000 LEDs per strip
- **Latency**: Minimal processing delay (~1ms)
- **Memory Usage**: Optimized for ESP32-C3 constraints

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Documentation

- **[系统架构概览](docs/ARCHITECTURE.md)** - 完整的系统架构、模块设计和数据流分析
- **[颜色数据处理流程](docs/COLOR_DATA_PROCESSING.md)** - 详细的RGB到RGBW颜色转换和硬件输出流程
- **[中断保护机制](docs/COLOR_DATA_PROCESSING.md#中断保护机制)** - LED闪烁问题解决方案和关键段保护实现

## Related Projects

- [Desktop Ambient Light Application](../desktop/) - Desktop application for screen color capture and LED control
- [Hardware Protocol Documentation](../docs/hardware-protocol.md) - Detailed communication protocol specification

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test thoroughly on hardware
5. Submit a pull request

## Support

For issues and questions:
- Check the troubleshooting section above
- Review the detailed project plan in `PLAN.md`
- Open an issue in the project repository

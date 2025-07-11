# Board-RS: ESP32-C3 Atmosphere Light LED Controller

A high-performance ESP32-C3 based universal LED driver board for atmosphere lighting systems, built with Rust embedded development.

## 🚀 Project Status: **FULLY OPERATIONAL**

✅ All systems working perfectly:

- WiFi connectivity with automatic DHCP configuration
- mDNS service discovery (`_atmosphere_light._udp.local.:23042`)
- UDP data reception and processing
- LED hardware control (500 RGBW LEDs supported)
- Desktop integration with real-time atmosphere lighting
- Performance: < 10ms latency for data transmission

## Overview

Board-RS is a complete WiFi-enabled LED hardware communication bridge that receives UDP data packets and forwards them directly to RGBW LED strips. This project uses a pure esp-hal approach, avoiding ESP-IDF components for optimal performance and reliability.

## Key Features

### ✅ Core Functionality

- **Universal LED Driver**: Acts as a passthrough for desktop-processed LED data
- **WiFi Management**: Automatic connection, DHCP client, and connection monitoring
- **mDNS Service Discovery**: Automatic service advertisement for seamless device discovery
- **UDP Communication Server**: Listens on port 23042 for LED data packets
- **RGBW LED Control**: Direct support for SK6812 RGBW LED strips (G,R,B,W channel order)
- **Real-time Performance**: Optimized for low-latency atmosphere lighting applications
- **Status Indication**: Visual feedback via first 3 LEDs with breathing effects
- **Robust Error Handling**: Comprehensive error recovery and logging

### 🔧 Technical Specifications

- **Target Hardware**: ESP32-C3 (RISC-V architecture)
- **Development Language**: Rust (no_std embedded)
- **HAL**: esp-hal (pure Rust, no ESP-IDF)
- **Networking**: embassy-net with esp-wifi
- **LED Control**: RMT peripheral with SK6812 timing
- **Memory Management**: esp-alloc with optimized heap usage

## Hardware Requirements

- **ESP32-C3 development board** (RISC-V architecture)
- **RGBW LED strips** (SK6812 compatible, G,R,B,W channel order)
- **Adequate power supply** for LED strips (5V recommended)
- **WiFi network connectivity** (2.4GHz)
- **GPIO4 connection** for LED data line

## Protocol Support

Supports the atmosphere light hardware communication protocol:

- **Service**: `_atmosphere_light._udp.local.`
- **Port**: UDP 23042
- **Header**: 0x02 (LED data packet identifier)
- **Format**: Offset (2 bytes) + Raw RGBW data stream
- **Data**: Direct RGBW values (4 bytes/LED: G,R,B,W)
- **Processing**: ESP32 acts as universal passthrough driver

## Build Requirements

- **Rust toolchain** with ESP32 target support
- **espup** for ESP32 Rust toolchain management
- **espflash** for firmware flashing and monitoring

## Quick Start

### 1. Install Dependencies

```bash
# Install Rust ESP32 toolchain
cargo install espup
espup install

# Install flashing tool
cargo install espflash
```

### 2. Configure WiFi Credentials

Create a `.env` file in the project root:

```bash
# Copy the example atmosphere file
cp .env.example .env
```

Edit `.env` and set your WiFi credentials:

```env
WIFI_SSID=your_wifi_network_name
WIFI_PASSWORD=your_wifi_password
```

Alternatively, set atmosphere variables directly:

```bash
export WIFI_SSID="your_wifi_network_name"
export WIFI_PASSWORD="your_wifi_password"
```

### 3. Build and Flash

```bash
# Build and flash firmware with monitoring
cargo run --release

# Or build and flash separately
cargo build --release
espflash flash target/riscv32imc-unknown-none-elf/release/board-rs
```

### 4. Verify Operation

The device will automatically:

- Connect to WiFi and obtain IP via DHCP
- Start mDNS service advertisement
- Begin listening for UDP packets on port 23042
- Display status via first 3 LEDs (white blinking = NetworkReady)

## Project Structure

```text
board-rs/
├── src/                     # Core source code
│   ├── main.rs             # Main application entry point
│   ├── lib.rs              # Library modules and error types
│   ├── led_control.rs      # LED control and RGBW data processing
│   ├── wifi.rs             # WiFi management with DHCP
│   ├── udp_server.rs       # UDP communication server
│   └── mdns.rs             # mDNS service discovery
├── docs/                   # Technical documentation
│   ├── ARCHITECTURE.md     # System architecture overview
│   ├── COLOR_DATA_PROCESSING.md # Data flow analysis
│   └── README.md           # Documentation index (Chinese)
├── .cargo/                 # Cargo configuration
│   └── config.toml         # Build configuration and WiFi credentials
├── Cargo.toml              # Project dependencies
├── build.rs                # Build script
├── rust-toolchain.toml     # Rust toolchain specification
└── README.md               # This file
```

## Configuration

### WiFi Settings

WiFi credentials are configured via atmosphere variables in `.cargo/config.toml`:

```toml
[env]
WIFI_SSID = "your_wifi_network_name"
WIFI_PASSWORD = "your_wifi_password"
```

### Hardware Configuration

- **LED Data Pin**: GPIO4 (hardcoded for SK6812 RGBW strips)
- **LED Count**: Supports up to 500 RGBW LEDs
- **Channel Order**: G,R,B,W (Green, Red, Blue, White)
- **Timing**: SK6812 protocol (1-bit: 600ns high + 600ns low, 0-bit: 300ns high + 900ns low)

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
avahi-browse -t _atmosphere_light._udp.local.

# Windows
dns-sd -B _atmosphere_light._udp
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

## Performance Metrics

- **LED Support**: Up to 500 RGBW LEDs per strip
- **Data Transmission**: < 10ms latency for real-time atmosphere lighting
- **Update Rate**: Supports high-frequency LED updates (60+ FPS)
- **Memory Usage**: Optimized for ESP32-C3 constraints (72KB heap)
- **Network Performance**: Stable UDP communication with chunked data support
- **Power Efficiency**: Low-power WiFi management with automatic reconnection

## Documentation

For detailed technical information, see the documentation in the `docs/` directory:

- **[ARCHITECTURE.md](docs/ARCHITECTURE.md)** - Complete system architecture and module design
- **[COLOR_DATA_PROCESSING.md](docs/COLOR_DATA_PROCESSING.md)** - Data flow analysis and processing details
- **[README.md](docs/README.md)** - Documentation index and quick navigation

## Related Projects

This ESP32 firmware is designed to work with desktop atmosphere lighting applications that support:

- mDNS service discovery
- UDP communication protocol with 0x02 header
- Raw RGBW data stream transmission
- Chunked data support for large LED counts

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes with proper testing
4. Test thoroughly on ESP32-C3 hardware
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to the branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

## License

This project is licensed under the GNU General Public License v3.0 - see the [LICENSE](LICENSE) file for details.

## Support

For issues and questions:

- Check the [troubleshooting section](#troubleshooting) above
- Review the [documentation](docs/) for detailed technical information
- Open an issue in the project repository with hardware details and logs

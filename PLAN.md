# ESP32-C3 Atmosphere Light Hardware Board Project Plan

## Project Overview

This project implements an embedded Rust firmware for ESP32-C3 microcontroller that acts as a WiFi-enabled LED hardware communication bridge. The board receives UDP packets containing LED color data and directly forwards them to WS2812 LED strips.

## Hardware Specifications

- **MCU**: ESP32-C3 (RISC-V single-core, 160MHz)
- **Atmosphere**: no-std embedded Rust
- **Connectivity**: WiFi 802.11 b/g/n
- **LED Output**: WS2812/WS2812B LED strips
- **Power**: 3.3V operation

## Core Functionality

### 1. WiFi Network Connection
- Connect to configured WiFi network
- Obtain IP address via DHCP
- Maintain stable connection with auto-reconnect

### 2. mDNS Service Discovery

- Advertise service as `_atmosphere_light._udp.local.`
- Allow desktop applications to discover the board automatically
- Provide service information including IP address and port

### 3. UDP Communication Server

- Listen on port **23042** for incoming UDP packets
- Process LED color data packets according to protocol specification
- No acknowledgment required (fire-and-forget protocol)

### 4. WS2812 LED Strip Control

- Direct data forwarding from UDP to WS2812 controller
- Support for both RGB (3 bytes/LED) and RGBW (4 bytes/LED) formats
- Hardware acts as simple UDP-to-WS2812 bridge without data processing

## Communication Protocol

### Packet Format

```
Byte 0: Header (0x02)
Byte 1: Offset High (upper 8 bits of LED start position)
Byte 2: Offset Low (lower 8 bits of LED start position)
Byte 3+: LED Color Data (variable length)
```

### LED Data Formats

- **RGB LEDs**: 3 bytes per LED `[R][G][B]`
- **RGBW LEDs**: 4 bytes per LED `[R][G][B][W]`
- **Value Range**: 0-255 for all color components

### Packet Processing Logic

1. **Validation**: Check minimum 3 bytes and header (0x02)
2. **Extract Offset**: Parse 16-bit LED start position (big-endian)
3. **Forward Data**: Send color data directly to WS2812 controller
4. **No Type Logic**: Hardware doesn't distinguish RGB/RGBW types

### Example Packets

**RGB Example** (3 LEDs: Red, Green, Blue at position 0):

```
02 00 00 FF 00 00 00 FF 00 00 00 FF
```

**RGBW Example** (2 LEDs: White, Warm White at position 10):

```
02 00 0A FF FF FF FF FF C8 96 C8
```

## Technical Architecture

### Software Components

1. **WiFi Manager**
   - Network connection establishment
   - Credential management
   - Connection monitoring and recovery

2. **mDNS Responder**
   - Service advertisement
   - Query response handling
   - Service information broadcasting

3. **UDP Server**
   - Socket creation and binding
   - Packet reception and parsing
   - Protocol validation

4. **WS2812 Driver**
   - SPI/RMT-based LED control
   - Timing-critical data transmission
   - Buffer management

5. **Main Controller**
   - Component coordination
   - Error handling
   - System monitoring

### Memory Management

- **no-std Atmosphere**: No heap allocation
- **Static Buffers**: Pre-allocated packet and LED data buffers
- **Stack Usage**: Careful stack size management for embedded constraints

### Error Handling
- **Network Errors**: WiFi disconnection recovery
- **Protocol Errors**: Invalid packet handling
- **Hardware Errors**: LED strip communication failures
- **System Errors**: Watchdog timer and panic handling

## Development Requirements

### Rust Dependencies
- `esp-hal`: ESP32-C3 hardware abstraction layer
- `esp-wifi`: WiFi networking stack
- `esp-alloc`: Memory allocation support
- `embedded-svc`: Service abstractions
- `heapless`: Collections without allocation

### Build Configuration
- **Target**: `riscv32imc-unknown-none-elf`
- **Optimization**: Size-optimized builds (`opt-level = "s"`)
- **Features**: `unstable-hal`, `alloc`, `wifi`

### Hardware Connections
- **LED Data Pin**: GPIO pin for WS2812 data signal
- **Power Supply**: Adequate current for LED strips
- **WiFi Antenna**: On-board or external antenna

## Implementation Phases

### Phase 1: Basic Infrastructure
- [x] Project setup and build configuration
- [x] Basic ESP32-C3 initialization
- [ ] GPIO and peripheral setup
- [ ] Serial debugging output

### Phase 2: WiFi Connectivity
- [ ] WiFi driver integration
- [ ] Network connection establishment
- [ ] DHCP client implementation
- [ ] Connection monitoring

### Phase 3: mDNS Service
- [ ] mDNS responder implementation
- [ ] Service advertisement setup
- [ ] Query handling
- [ ] Service discovery testing

### Phase 4: UDP Communication
- [ ] UDP socket creation
- [ ] Packet reception loop
- [ ] Protocol parsing and validation
- [ ] Error handling

### Phase 5: LED Control
- [ ] WS2812 driver implementation
- [ ] Data forwarding logic
- [ ] Timing optimization
- [ ] Performance testing

### Phase 6: Integration & Testing
- [ ] End-to-end testing with desktop application
- [ ] Performance optimization
- [ ] Error recovery testing
- [ ] Documentation completion

## Configuration Management

### WiFi Credentials
- Compile-time configuration or
- Runtime configuration via serial interface
- Secure storage considerations

### Network Settings
- Static IP vs DHCP
- DNS server configuration
- Network timeout settings

### LED Configuration
- Maximum LED count support
- Data pin assignment
- Timing parameters

## Performance Considerations

### Network Performance
- **Packet Rate**: Support high-frequency updates
- **Latency**: Minimize processing delay
- **Throughput**: Handle multiple concurrent strips

### LED Performance
- **Update Rate**: Smooth animation support
- **Color Accuracy**: Precise timing for WS2812
- **Strip Length**: Support for long LED strips

### Memory Efficiency
- **Buffer Sizes**: Optimal packet and LED buffers
- **Stack Usage**: Minimize stack consumption
- **Code Size**: Fit within flash constraints

## Testing Strategy

### Unit Testing
- Protocol parsing functions
- Data validation logic
- Buffer management

### Integration Testing
- WiFi connection stability
- mDNS service discovery
- UDP packet handling
- LED output verification

### Performance Testing
- Network throughput measurement
- LED update rate testing
- Memory usage profiling
- Power consumption analysis

## Documentation

### Code Documentation
- Inline code comments
- API documentation
- Architecture overview

### User Documentation
- Setup and configuration guide
- Troubleshooting guide
- Protocol specification

### Development Documentation
- Build instructions
- Debugging guide
- Testing procedures

## Future Enhancements

### Advanced Features
- Multiple LED strip support
- Configuration web interface
- Firmware update over WiFi
- Status LED indicators

### Performance Optimizations
- DMA-based LED updates
- Packet batching
- Network buffer optimization
- Power management

### Monitoring & Diagnostics
- Network statistics
- Error reporting
- Performance metrics
- Remote debugging

## Security Considerations

### Network Security
- WiFi security protocols (WPA2/WPA3)
- UDP packet validation
- Rate limiting protection

### System Security
- Secure boot implementation
- Flash encryption
- Debug interface protection

## Compliance & Standards

### Regulatory Compliance
- FCC/CE certification requirements
- WiFi alliance certification
- EMC/EMI considerations

### Industry Standards
- IEEE 802.11 WiFi standards
- WS2812 protocol compliance
- UDP/IP protocol standards

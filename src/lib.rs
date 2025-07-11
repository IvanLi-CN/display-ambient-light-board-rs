#![no_std]

//! ESP32-C3 Ambient Light Hardware Board Library
//!
//! This library provides modules for implementing a WiFi-enabled LED hardware
//! communication bridge that receives UDP packets and forwards them to WS2812 LED strips.

extern crate alloc;

pub mod led_control;
pub mod state_machine;
pub mod udp_server;
pub mod wifi;

/// Project version information
pub const VERSION: &str = "0.1.0-dev";

/// Default configuration constants
pub mod config {
    /// Default UDP port for LED data communication
    pub const UDP_PORT: u16 = 23042;

    /// Default LED data GPIO pin
    pub const LED_DATA_PIN: u8 = 4;

    /// Maximum supported LEDs per strip
    pub const MAX_LEDS: usize = 1000;

    /// mDNS service name
    pub const MDNS_SERVICE_NAME: &str = "_ambient_light._udp.local.";

    /// Protocol header byte for LED data packets
    pub const PROTOCOL_HEADER: u8 = 0x02;

    /// Protocol header byte for connection check packets
    pub const CONNECTION_CHECK_HEADER: u8 = 0x01;

    /// WiFi configuration
    /// Read from environment variables at compile time
    pub const WIFI_SSID: &str = env!("WIFI_SSID");
    pub const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");

    /// WiFi connection timeout in milliseconds
    pub const WIFI_CONNECT_TIMEOUT_MS: u32 = 10000;

    /// WiFi reconnection interval in milliseconds
    pub const WIFI_RECONNECT_INTERVAL_MS: u32 = 5000;
}

/// Error types for the atmosphere light board
#[derive(Debug, Clone, Copy)]
pub enum BoardError {
    /// WiFi connection error
    WiFiError,
    /// UDP server error
    UdpError,
    /// LED control error
    LedError,
    /// Protocol parsing error
    ProtocolError,
    /// System error
    SystemError,
    /// mDNS service error
    MdnsError,
}

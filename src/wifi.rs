//! WiFi module for ESP32-C3 board
//!
//! Handles WiFi network connection using esp-wifi 0.14.1 with embassy-net DHCP

use crate::{BoardError, config};
use esp_wifi::wifi::{WifiController, ClientConfiguration, AuthMethod};
use esp_println::println;
use alloc::string::{String, ToString};
use heapless::Vec;
use embassy_net::Stack;

/// DHCP configuration information
#[derive(Debug, Clone)]
pub struct DhcpInfo {
    pub ip_address: [u8; 4],
    pub subnet_mask: [u8; 4],
    pub gateway: Option<[u8; 4]>,
    pub dns_servers: Vec<[u8; 4], 3>,
}

/// WiFi manager for handling network connectivity with real DHCP
pub struct WiFiManager<'a> {
    controller: WifiController<'a>,
    is_connected: bool,
    stack: Option<Stack<'a>>, // Embassy-net stack for real DHCP
}

impl<'a> WiFiManager<'a> {
    /// Create a new WiFi manager instance
    pub fn new(controller: WifiController<'a>) -> Self {
        Self {
            controller,
            is_connected: false,
            stack: None,
        }
    }

    /// Set the embassy-net stack for real DHCP functionality
    pub fn set_stack(&mut self, stack: Stack<'a>) {
        self.stack = Some(stack);
    }

    /// Connect to WiFi network
    pub fn connect(&mut self, ssid: &str, password: &str) -> Result<(), BoardError> {
        println!("[WIFI] Connecting to WiFi network: {}", ssid);

        let client_config = ClientConfiguration {
            ssid: ssid.try_into().map_err(|_| BoardError::WiFiError)?,
            password: password.try_into().map_err(|_| BoardError::WiFiError)?,
            auth_method: AuthMethod::WPA2Personal,
            ..Default::default()
        };

        self.controller.set_configuration(&esp_wifi::wifi::Configuration::Client(client_config))
            .map_err(|_| BoardError::WiFiError)?;

        self.controller.start().map_err(|_| BoardError::WiFiError)?;
        self.controller.connect().map_err(|_| BoardError::WiFiError)?;

        // Wait for connection
        let mut attempts = 0;
        while !self.controller.is_connected().unwrap_or(false) && attempts < 50 {
            attempts += 1;
            // Simple delay
            for _ in 0..100000 {
                core::hint::spin_loop();
            }
        }

        if self.controller.is_connected().unwrap_or(false) {
            self.is_connected = true;
            println!("[WIFI] Successfully connected to WiFi network");

            // Try to get DHCP IP address
            self.update_dhcp_ip();

            Ok(())
        } else {
            println!("[WIFI] Failed to connect to WiFi network after {} attempts", attempts);
            Err(BoardError::WiFiError)
        }
    }

    /// Update DHCP IP address using real embassy-net stack
    fn update_dhcp_ip(&mut self) {
        // Real DHCP implementation using embassy-net stack
        if self.is_connected {
            if let Some(ref stack) = self.stack {
                if let Some(config) = stack.config_v4() {
                    let ip = config.address.address();
                    println!("[WIFI] Real DHCP IP address: {}", ip);
                } else {
                    println!("[WIFI] DHCP configuration not yet available");
                }
            } else {
                println!("[WIFI] Embassy-net stack not set - cannot get real DHCP IP");
            }
        }
    }

    /// Get current IP address from real DHCP
    pub fn get_ip_address(&self) -> Option<[u8; 4]> {
        if !self.is_connected {
            return None;
        }

        // Get real IP address from embassy-net stack
        if let Some(ref stack) = self.stack {
            if let Some(config) = stack.config_v4() {
                let ip = config.address.address();
                let octets = ip.octets();
                println!("[WIFI] Real DHCP assigned IP address: {}.{}.{}.{}",
                    octets[0], octets[1], octets[2], octets[3]);
                return Some(octets);
            } else {
                println!("[WIFI] DHCP configuration not yet available");
            }
        } else {
            println!("[WIFI] Embassy-net stack not set - cannot get real DHCP IP");
        }

        None
    }

    /// Get detailed DHCP configuration information
    pub fn get_dhcp_info(&self) -> Option<DhcpInfo> {
        if let Some(ref stack) = self.stack {
            if let Some(config) = stack.config_v4() {
                let ip = config.address.address().octets();
                let subnet_mask = config.address.prefix_len();

                // Convert prefix length to subnet mask
                let mask_value = (!0u32) << (32 - subnet_mask);
                let mask = [
                    (mask_value >> 24) as u8,
                    (mask_value >> 16) as u8,
                    (mask_value >> 8) as u8,
                    mask_value as u8,
                ];

                let mut dns_servers = Vec::new();
                // Add DNS servers from config if available
                for dns in config.dns_servers.iter() {
                    let _ = dns_servers.push(dns.octets());
                }

                // If no DNS servers, add default
                if dns_servers.is_empty() {
                    let _ = dns_servers.push([8, 8, 8, 8]); // Google DNS as fallback
                }

                return Some(DhcpInfo {
                    ip_address: ip,
                    subnet_mask: mask,
                    gateway: config.gateway.map(|gw| gw.octets()),
                    dns_servers,
                });
            }
        }
        None
    }

    /// Print detailed DHCP information
    pub fn print_dhcp_info(&self) {
        if let Some(info) = self.get_dhcp_info() {
            println!("[DHCP] === DHCP Configuration ===");
            println!("[DHCP] IP Address: {}.{}.{}.{}",
                info.ip_address[0], info.ip_address[1], info.ip_address[2], info.ip_address[3]);

            if let Some(gateway) = info.gateway {
                println!("[DHCP] Gateway: {}.{}.{}.{}",
                    gateway[0], gateway[1], gateway[2], gateway[3]);
            }

            println!("[DHCP] Subnet Mask: {}.{}.{}.{}",
                info.subnet_mask[0], info.subnet_mask[1], info.subnet_mask[2], info.subnet_mask[3]);

            for (i, dns) in info.dns_servers.iter().enumerate() {
                println!("[DHCP] DNS Server {}: {}.{}.{}.{}",
                    i + 1, dns[0], dns[1], dns[2], dns[3]);
            }
            println!("[DHCP] === End Configuration ===");
        } else {
            println!("[DHCP] No DHCP configuration available");
        }
    }

    /// Check if WiFi is connected
    pub fn is_connected(&self) -> bool {
        self.is_connected && self.controller.is_connected().unwrap_or(false)
    }

    /// Get WiFi controller for advanced operations
    pub fn get_controller(&mut self) -> &mut WifiController<'a> {
        &mut self.controller
    }

    /// Monitor WiFi connection status
    pub fn monitor_connection(&mut self) -> Result<(), BoardError> {
        let current_status = self.controller.is_connected().unwrap_or(false);

        if self.is_connected && !current_status {
            println!("[WIFI] WiFi connection lost!");
            self.is_connected = false;
            // Note: Embassy-net stack will handle IP cleanup automatically
        } else if !self.is_connected && current_status {
            println!("[WIFI] WiFi connection restored!");
            self.is_connected = true;

            // Update DHCP IP when connection is restored
            self.update_dhcp_ip();
        }

        Ok(())
    }
}

/// Create WiFi configuration from environment variables
pub fn create_wifi_config() -> (String, String) {
    (config::WIFI_SSID.to_string(), config::WIFI_PASSWORD.to_string())
}
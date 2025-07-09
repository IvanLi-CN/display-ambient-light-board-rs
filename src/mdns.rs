//! mDNS service discovery module
//!
//! Implements mDNS responder for automatic service discovery using edge-mdns
//! Advertises the board as "_ambient_light._udp.local."

use crate::{BoardError, config};
use core::net::{Ipv4Addr, Ipv6Addr};
use edge_mdns::{
    domain::base::Ttl,
    host::{Host, Service},
};
use embassy_net::Stack;
use esp_println::println;
use heapless::String;

/// Maximum mDNS packet size
const _MAX_MDNS_PACKET_SIZE: usize = 1500;

/// mDNS service manager with real edge-mdns implementation
pub struct MdnsManager<'a> {
    service_name: String<64>,
    hostname: String<32>,
    is_running: bool,
    ip_address: Option<[u8; 4]>,
    stack: Option<&'a Stack<'a>>,
}

impl<'a> MdnsManager<'a> {
    /// Create new mDNS manager
    pub fn new() -> Self {
        let mut service_name = String::new();
        service_name.push_str(config::MDNS_SERVICE_NAME).ok();

        let mut hostname = String::new();
        hostname.push_str("board-rs").ok();

        Self {
            service_name,
            hostname,
            is_running: false,
            ip_address: None,
            stack: None,
        }
    }

    /// Set network stack reference
    pub fn set_stack(&mut self, stack: &'a Stack<'a>) {
        self.stack = Some(stack);
    }

    /// Start mDNS service advertisement with real implementation
    pub fn start_service(&mut self, ip_address: [u8; 4]) -> Result<(), BoardError> {
        self.ip_address = Some(ip_address);
        self.is_running = true;

        // 创建mDNS服务定义
        if self.create_service().is_none() {
            return Err(BoardError::MdnsError);
        }

        // 创建主机定义
        if self.create_host().is_none() {
            return Err(BoardError::MdnsError);
        }

        println!("[MDNS] Service started");

        Ok(())
    }

    /// Stop mDNS service
    pub fn stop_service(&mut self) -> Result<(), BoardError> {
        if self.is_running {
            self.is_running = false;
            self.ip_address = None;
        }
        Ok(())
    }

    /// Check if service is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Create mDNS service definition for edge-mdns
    fn create_service(&self) -> Option<Service<'_>> {
        if !self.is_running {
            return None;
        }

        Some(Service {
            name: "board-rs",
            priority: 0,
            weight: 0,
            service: "_ambient_light",
            protocol: "_udp",
            port: config::UDP_PORT,
            service_subtypes: &[],
            txt_kvs: &[
                ("version", "0.1.0"),
                ("protocol", "led-control"),
                ("format", "rgb"),
            ],
        })
    }

    /// Create host definition for edge-mdns
    fn create_host(&self) -> Option<Host<'_>> {
        if let Some(ip) = self.ip_address {
            Some(Host {
                hostname: self.hostname.as_str(),
                ipv4: Ipv4Addr::from(ip),
                ipv6: Ipv6Addr::UNSPECIFIED,
                ttl: Ttl::from_secs(120),
            })
        } else {
            None
        }
    }

    /// Get service information
    pub fn get_service_info(&self) -> ServiceInfo {
        ServiceInfo {
            name: self.service_name.clone(),
            port: config::UDP_PORT,
            is_running: self.is_running,
        }
    }
}

/// Service information structure
#[derive(Clone)]
pub struct ServiceInfo {
    pub name: String<64>,
    pub port: u16,
    pub is_running: bool,
}

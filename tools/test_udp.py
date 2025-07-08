#!/usr/bin/env python3
"""
Simple UDP test script for board-rs LED controller

This script sends test LED data to the ESP32-C3 board for verification.
Usage: python3 tools/test_udp.py [board_ip]
"""

import socket
import struct
import time
import sys

def send_led_data(board_ip: str, port: int = 23042):
    """Send test LED data to the board"""
    
    print(f"🚀 Testing board-rs LED controller at {board_ip}:{port}")
    
    # Create UDP socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(5.0)
    
    esp32_addr = (board_ip, port)
    
    # Test 1: Basic RGB colors
    print("\n📡 Test 1: Basic RGB colors")
    header = 0x02  # LED data header
    offset = 0     # Start at LED 0
    
    rgb_data = [
        255, 0, 0,    # LED 0: Red
        0, 255, 0,    # LED 1: Green  
        0, 0, 255,    # LED 2: Blue
    ]
    
    packet = struct.pack('!BH', header, offset) + bytes(rgb_data)
    
    try:
        sock.sendto(packet, esp32_addr)
        print(f"✅ Sent RGB test: {len(rgb_data)//3} LEDs")
        time.sleep(2)
        
        # Test 2: Mixed colors
        print("\n📡 Test 2: Mixed colors")
        rgb_data2 = [
            255, 255, 0,  # LED 0: Yellow
            255, 0, 255,  # LED 1: Magenta
            0, 255, 255,  # LED 2: Cyan
            255, 255, 255, # LED 3: White
        ]
        
        packet2 = struct.pack('!BH', header, offset) + bytes(rgb_data2)
        sock.sendto(packet2, esp32_addr)
        print(f"✅ Sent mixed colors: {len(rgb_data2)//3} LEDs")
        time.sleep(2)
        
        # Test 3: Turn off LEDs
        print("\n📡 Test 3: Turn off LEDs")
        rgb_data3 = [0, 0, 0] * 4  # 4 LEDs, all black
        
        packet3 = struct.pack('!BH', header, offset) + bytes(rgb_data3)
        sock.sendto(packet3, esp32_addr)
        print(f"✅ Sent turn-off command: {len(rgb_data3)//3} LEDs")
        
        print("\n🎉 All tests completed successfully!")
        
    except socket.timeout:
        print("❌ Timeout: No response from board")
    except Exception as e:
        print(f"❌ Error: {e}")
    finally:
        sock.close()

def discover_board():
    """Try to discover board via mDNS (simplified)"""
    print("🔍 Trying to discover board...")
    print("💡 Tip: Check your router's DHCP client list for 'board-rs' device")
    return None

def main():
    """Main function"""
    if len(sys.argv) > 1:
        board_ip = sys.argv[1]
    else:
        board_ip = input("Enter board IP address (or press Enter to discover): ").strip()
        
        if not board_ip:
            discovered_ip = discover_board()
            if discovered_ip:
                board_ip = discovered_ip
            else:
                print("❌ Could not discover board. Please provide IP address manually.")
                return
    
    # Validate IP format
    try:
        socket.inet_aton(board_ip)
    except socket.error:
        print(f"❌ Invalid IP address: {board_ip}")
        return
    
    send_led_data(board_ip)

if __name__ == "__main__":
    main()

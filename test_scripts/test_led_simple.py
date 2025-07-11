#!/usr/bin/env python3
"""
Simple test script to send LED data to ESP32 for testing
"""

import socket
import time

# ESP32 configuration
ESP32_IP = "192.168.31.182"
ESP32_PORT = 23042

def send_led_data(colors):
    """Send LED data to ESP32"""
    try:
        # Create UDP socket
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.settimeout(5.0)
        
        # Create LED packet: 0x02 + offset (0x00 0x00) + color data
        packet = bytearray([0x02, 0x00, 0x00])  # Header + offset 0
        
        # Add color data (RGBW format: G, R, B, W)
        for color in colors:
            packet.extend(color)
        
        print(f"Sending LED packet: {packet.hex()}")
        sock.sendto(packet, (ESP32_IP, ESP32_PORT))
        print("✅ LED data sent successfully")
        
    except Exception as e:
        print(f"❌ Error: {e}")
    finally:
        sock.close()

def main():
    """Main function"""
    print("ESP32 LED Test")
    print("=" * 30)
    
    # Test 1: Single bright white LED
    print("\nTest 1: Single bright white LED")
    white_led = [(255, 255, 255, 255)]  # G, R, B, W
    send_led_data(white_led)
    time.sleep(2)
    
    # Test 2: Single red LED
    print("\nTest 2: Single red LED")
    red_led = [(0, 255, 0, 0)]  # G, R, B, W
    send_led_data(red_led)
    time.sleep(2)
    
    # Test 3: Single green LED
    print("\nTest 3: Single green LED")
    green_led = [(255, 0, 0, 0)]  # G, R, B, W
    send_led_data(green_led)
    time.sleep(2)
    
    # Test 4: Single blue LED
    print("\nTest 4: Single blue LED")
    blue_led = [(0, 0, 255, 0)]  # G, R, B, W
    send_led_data(blue_led)
    time.sleep(2)
    
    # Test 5: Turn off
    print("\nTest 5: Turn off LED")
    off_led = [(0, 0, 0, 0)]  # G, R, B, W
    send_led_data(off_led)
    
    print("\n✅ LED test completed")

if __name__ == "__main__":
    main()

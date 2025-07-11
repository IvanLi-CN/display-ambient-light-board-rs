#!/usr/bin/env python3
"""
Simple test script to send 0x01 connection check message to ESP32
"""

import socket
import time

# ESP32 configuration
ESP32_IP = "192.168.31.182"  # From the log output
ESP32_PORT = 23042

def send_connection_check():
    """Send a 0x01 connection check message to ESP32"""
    try:
        # Create UDP socket
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.settimeout(5.0)  # 5 second timeout
        
        # Send 0x01 message
        message = bytes([0x01])
        print(f"Sending 0x01 connection check to {ESP32_IP}:{ESP32_PORT}")
        sock.sendto(message, (ESP32_IP, ESP32_PORT))
        
        # Wait for response
        try:
            response, addr = sock.recvfrom(1024)
            print(f"Received response from {addr}: {response.hex()}")
            if response == message:
                print("✅ Connection check successful!")
                return True
            else:
                print("❌ Unexpected response")
                return False
        except socket.timeout:
            print("❌ No response received (timeout)")
            return False
            
    except Exception as e:
        print(f"❌ Error: {e}")
        return False
    finally:
        sock.close()

def main():
    """Main function"""
    print("ESP32 Connection Test")
    print("=" * 30)
    
    # Send connection check
    success = send_connection_check()
    
    if success:
        print("\n✅ ESP32 should now be in Operational state")
        print("Check the LED strip for breathing effect!")
    else:
        print("\n❌ Failed to establish connection")

if __name__ == "__main__":
    main()

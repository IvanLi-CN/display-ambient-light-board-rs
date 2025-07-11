#!/usr/bin/env python3
"""
RGBW LED通道顺序测试脚本
用于确定ESP32 RGBW LED的实际通道顺序
"""

import socket
import time

# ESP32配置
ESP32_IP = "192.168.31.182"
ESP32_PORT = 23042

# LED配置
LED_COUNT = 10  # 只测试前10个LED
BYTES_PER_LED = 4  # RGBW: 4个字节

def send_connection_check():
    """发送连接检查消息 (0x01)"""
    return bytes([0x01])

def send_led_data(led_data, offset=0):
    """发送LED数据 (0x02 + 偏移量 + 数据)"""
    packet = bytearray([0x02])  # LED数据包头
    # 添加16位偏移量 (big-endian)
    packet.extend([(offset >> 8) & 0xFF, offset & 0xFF])  # 偏移量高字节, 偏移量低字节
    packet.extend(led_data)
    return bytes(packet)

def create_test_pattern(channel_index, brightness=255):
    """创建测试模式 - 只点亮指定通道"""
    led_data = bytearray()
    
    for i in range(LED_COUNT):
        # 创建4字节的LED数据，只有指定通道有值
        channels = [0, 0, 0, 0]
        channels[channel_index] = brightness
        led_data.extend(channels)
    
    return led_data

def main():
    print("RGBW LED通道顺序测试")
    print(f"目标: {ESP32_IP}:{ESP32_PORT}")
    print(f"测试前{LED_COUNT}个LED")
    print()
    
    # 创建UDP socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    
    try:
        # 发送连接检查消息
        connection_check = send_connection_check()
        sock.sendto(connection_check, (ESP32_IP, ESP32_PORT))
        print("发送连接检查消息 (0x01)")
        time.sleep(1)
        
        # 测试每个通道
        test_patterns = [
            (0, "第1个字节 (索引0)"),
            (1, "第2个字节 (索引1)"),
            (2, "第3个字节 (索引2)"),
            (3, "第4个字节 (索引3)"),
        ]
        
        for channel_index, description in test_patterns:
            print(f"\n测试 {description}:")
            print("观察LED显示的颜色...")
            
            # 创建测试数据
            test_data = create_test_pattern(channel_index)
            led_packet = send_led_data(test_data)
            
            # 发送数据并保持5秒
            for _ in range(10):  # 发送10次，每次0.5秒
                sock.sendto(led_packet, (ESP32_IP, ESP32_PORT))
                time.sleep(0.5)
            
            input("按回车键继续下一个测试...")
        
        # 关闭所有LED
        print("\n关闭所有LED...")
        off_data = create_test_pattern(0, 0)  # 所有通道都是0
        led_packet = send_led_data(off_data)
        sock.sendto(led_packet, (ESP32_IP, ESP32_PORT))
        
    except KeyboardInterrupt:
        print("\n测试停止")
    except Exception as e:
        print(f"错误: {e}")
    finally:
        sock.close()
        print("UDP socket已关闭")

if __name__ == "__main__":
    main()

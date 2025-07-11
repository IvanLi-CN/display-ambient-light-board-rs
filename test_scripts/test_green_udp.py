#!/usr/bin/env python3
"""
UDP测试脚本 - 发送10fps全绿色LED数据
测试氛围灯模式下是否会忽略传入数据并继续显示呼吸效果
"""

import socket
import time
import struct

# ESP32配置
ESP32_IP = "192.168.31.182"
ESP32_PORT = 23042

# LED配置
LED_COUNT = 60  # 只测试前60个LED
BYTES_PER_LED = 4  # RGBW: G, R, B, W

def create_green_led_data():
    """创建全绿色LED数据"""
    led_data = bytearray()

    for i in range(LED_COUNT):
        # RGBW格式: G, R, B, W (根据硬件配置)
        # 全绿色: G=255, R=0, B=0, W=0
        led_data.extend([255, 0, 0, 0])  # G, R, B, W

    return led_data

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

def main():
    print(f"开始UDP测试 - 目标: {ESP32_IP}:{ESP32_PORT}")
    print(f"发送10fps全绿色数据到{LED_COUNT}个LED")
    print("按Ctrl+C停止测试")
    
    # 创建UDP socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    
    try:
        # 创建全绿色LED数据
        green_data = create_green_led_data()
        print(f"LED数据大小: {len(green_data)} bytes")
        
        # 首先发送连接检查消息
        connection_check = send_connection_check()
        sock.sendto(connection_check, (ESP32_IP, ESP32_PORT))
        print("发送连接检查消息 (0x01)")
        
        # 等待一秒让ESP32进入Operational状态
        time.sleep(1)
        
        frame_count = 0
        start_time = time.time()
        
        while True:
            # 发送LED数据包
            led_packet = send_led_data(green_data)
            sock.sendto(led_packet, (ESP32_IP, ESP32_PORT))
            
            frame_count += 1
            current_time = time.time()
            elapsed = current_time - start_time
            
            if frame_count % 50 == 0:  # 每5秒打印一次状态
                fps = frame_count / elapsed
                print(f"已发送 {frame_count} 帧, 平均FPS: {fps:.1f}")
            
            # 10fps = 100ms间隔
            time.sleep(0.5)
            
    except KeyboardInterrupt:
        print("\n测试停止")
        elapsed = time.time() - start_time
        fps = frame_count / elapsed if elapsed > 0 else 0
        print(f"总共发送 {frame_count} 帧, 平均FPS: {fps:.1f}")
    
    except Exception as e:
        print(f"错误: {e}")
    
    finally:
        sock.close()
        print("UDP socket已关闭")

if __name__ == "__main__":
    main()

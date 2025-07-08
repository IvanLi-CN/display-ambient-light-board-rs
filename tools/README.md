# Board-RS 测试工具

本目录包含用于测试和验证 board-rs 功能的工具脚本。

## 🧪 测试脚本

### test_udp.py
简单的UDP测试脚本，用于验证ESP32-C3 LED控制器功能。

**功能**：
- 发送基本RGB颜色测试
- 发送混合颜色测试  
- 发送关闭LED命令
- 验证UDP通信正常

**使用方法**：
```bash
# 指定IP地址
python3 tools/test_udp.py 192.168.1.100

# 交互式输入IP
python3 tools/test_udp.py
```

**测试序列**：
1. **基本RGB**: 红、绿、蓝色LED
2. **混合颜色**: 黄、洋红、青、白色LED
3. **关闭LED**: 所有LED设为黑色

## 📋 使用前提

1. **ESP32-C3已连接WiFi**: 确保设备已成功连接到网络
2. **获取设备IP**: 通过路由器DHCP客户端列表查找'board-rs'设备
3. **网络连通性**: 确保测试设备与ESP32在同一网络

## 🔍 故障排除

### 常见问题

**"Timeout: No response from board"**
- 检查IP地址是否正确
- 确认ESP32已连接WiFi
- 检查防火墙设置

**"Invalid IP address"**
- 确保IP格式正确 (例: 192.168.1.100)
- 避免使用域名或主机名

**LED不亮或颜色错误**
- 检查LED硬件连接 (GPIO4)
- 确认LED电源供应充足
- 查看ESP32串口输出调试信息

## 🚀 扩展测试

如需更复杂的测试，可以修改 `test_udp.py` 脚本：

```python
# 测试更多LED
rgb_data = [255, 0, 0] * 10  # 10个红色LED

# 测试不同偏移量
offset = 5  # 从第5个LED开始

# 测试动画效果
for i in range(10):
    # 发送渐变色彩
    pass
```

## 📚 相关文档

- [颜色数据处理流程](../docs/COLOR_DATA_PROCESSING.md)
- [系统架构](../docs/ARCHITECTURE.md)
- [主项目README](../README.md)

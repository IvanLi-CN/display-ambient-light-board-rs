# Board-RS 架构文档

## 系统概览

Board-RS 是一个基于 ESP32-C3 的 WiFi LED 控制器，使用 Rust 嵌入式开发，实现了从桌面应用到硬件LED的完整数据流处理。

## 🏗️ 系统架构

```
桌面应用 ──UDP──> ESP32-C3 ──RMT──> SK6812-RGBW LED
    │                │              │
    │                │              └─ 硬件输出 (G,R,B,W)
    │                │
    │                └─ 数据处理流程:
    │                   1. UDP接收
    │                   2. 协议解析  
    │                   3. RGB→RGBW转换
    │                   4. 帧缓冲
    │                   5. 中断保护传输
    │
    └─ RGB数据 (R,G,B)
```

## 📦 核心模块

### 1. 网络层 (`src/wifi.rs`, `src/udp_server.rs`)
- **WiFi管理**: 自动连接、DHCP、重连机制
- **mDNS服务**: 设备发现 (`_ambient_light._udp.local.`)
- **UDP服务器**: 端口23042，接收LED数据包

### 2. 协议层 (`src/udp_server.rs`)
- **包格式**: `[Header(0x02)] [Offset(2字节)] [RGB数据]`
- **数据验证**: 协议头检查、包长度验证
- **错误处理**: 无效包过滤、连接检查

### 3. LED控制层 (`src/led_control.rs`)
- **颜色转换**: RGB → RGBW (添加白色通道)
- **帧缓冲**: 完整帧收集后再输出
- **硬件驱动**: RMT外设控制SK6812-RGBW

### 4. 硬件抽象层
- **RMT外设**: 精确时序控制 (10MHz)
- **GPIO控制**: LED数据引脚 (GPIO4)
- **中断保护**: 关键段保护传输完整性

## 🔄 数据流处理

### 输入数据格式
```
桌面端: [255, 0, 0] (红色RGB)
UDP包: [0x02, 0x00, 0x00, 255, 0, 0]
```

### 内部处理
```rust
// 1. UDP解析
let r = chunk[0];  // 255
let g = chunk[1];  // 0  
let b = chunk[2];  // 0

// 2. RGBW转换
let color = RgbwColor { r: 255, g: 0, b: 0, w: 0 };

// 3. 硬件输出重排序 (G,R,B,W)
output = [0, 255, 0, 0]  // 绿=0, 红=255, 蓝=0, 白=0
```

### 输出结果
```
LED显示: 红色 ✅
```

## 🛡️ 关键技术特性

### 中断保护机制
```rust
critical_section::with(|_| {
    // LED数据传输期间禁用中断
    // 防止embassy任务调度干扰RMT时序
    channel.transmit(&pulses)?.wait()
});
```

**解决问题**: 消除间歇性LED闪烁

### 帧缓冲系统
- **分包接收**: 支持大型LED阵列的分包传输
- **完整性保证**: 只有完整帧才会输出到硬件
- **状态管理**: `WaitingForFrame` → `CollectingFrame` → `FrameComplete`

### 异步任务架构
```rust
// Embassy异步任务
├── main_task()      // 主控制逻辑
├── net_task()       // 网络栈处理
├── udp_server_task() // UDP数据接收
└── mdns_task()      // mDNS服务广告
```

## 📊 性能指标

| 指标 | 数值 | 说明 |
|------|------|------|
| 最大LED数量 | 1000 | 受内存限制 |
| 更新延迟 | <1ms | UDP接收到LED输出 |
| 内存使用 | 72KB堆 | esp-alloc配置 |
| 网络端口 | 23042 | UDP监听端口 |
| 时钟频率 | 10MHz | RMT外设频率 |

## 🔧 硬件配置

### ESP32-C3引脚分配
- **LED数据**: GPIO4
- **WiFi**: 内置天线
- **调试**: USB-JTAG

### LED时序参数 (SK6812-RGBW)
- **1位**: 6高/6低周期 (600ns/600ns)
- **0位**: 3高/9低周期 (300ns/900ns)  
- **复位**: 2000低周期 (200μs)

## 🚀 部署流程

1. **编译**: `cargo build --release`
2. **烧录**: `espflash flash target/riscv32imc-unknown-none-elf/release/board-rs`
3. **监控**: `espflash monitor`
4. **测试**: 发送UDP包验证功能

## 🐛 故障排除

### 常见问题
- **LED闪烁**: 已通过中断保护解决
- **WiFi连接**: 检查SSID/密码配置
- **颜色错误**: 确认RGB→RGBW转换正确

### 调试日志
```
[LED] 🔒 Starting critical section for LED transmission
[LED] ✅ LED transmission completed successfully in critical section
[UDP] ✅ Parsed 3 RGB LEDs from packet
[WIFI] Real DHCP assigned IP address: 192.168.31.182
```

## 📚 相关文档

- **[颜色数据处理详解](COLOR_DATA_PROCESSING.md)**: 完整的数据转换流程
- **[协议规范](../desktop/docs/)**: 桌面端通信协议
- **[硬件连接指南](../README.md#hardware-requirements)**: 硬件连接说明

## 🔮 未来扩展

- **多LED条支持**: 并行控制多个LED条
- **效果引擎**: 内置动画和过渡效果
- **OTA更新**: 无线固件更新功能
- **Web配置**: 浏览器配置界面

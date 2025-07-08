# 颜色数据处理流程文档

## 概述

本文档详细说明了 board-rs 项目中从桌面端发送RGB数据到ESP32-C3硬件输出RGBW LED的完整数据处理流程。

## 🎨 颜色数据处理流程

### 1. 桌面端数据格式

桌面端应用程序发送标准RGB格式的数据：

```python
# 示例：test_udp.py
rgb_data = [
    255, 0, 0,    # LED 0: 红色 (R=255, G=0, B=0)
    0, 255, 0,    # LED 1: 绿色 (R=0, G=255, B=0)  
    0, 0, 255,    # LED 2: 蓝色 (R=0, G=0, B=255)
]
```

**数据特征**：
- **格式**：每个LED使用3字节 (RGB)
- **顺序**：`R, G, B` (红, 绿, 蓝)
- **范围**：每个通道 0-255

### 2. UDP协议封装

数据通过UDP协议发送到ESP32：

```
UDP包结构：
[Header(1字节)] [Offset(2字节)] [RGB数据(3*N字节)]
     0x02           0x0000         R,G,B,R,G,B,...
```

### 3. ESP32接收与解析

#### 3.1 UDP服务器接收 (`src/udp_server.rs`)

```rust
// 解析UDP包头
let offset = u16::from_be_bytes([data[1], data[2]]);
let led_data = &data[3..];  // 提取RGB数据部分
```

#### 3.2 颜色格式转换 (`src/led_control.rs`)

```rust
fn parse_colors(&self, data: &[u8]) -> Result<Vec<RgbwColor, {config::MAX_LEDS}>, BoardError> {
    // RGB格式检测 (3字节/LED)
    if data.len() % 3 == 0 {
        for chunk in data.chunks_exact(3) {
            let r = chunk[0];  // 红色通道
            let g = chunk[1];  // 绿色通道  
            let b = chunk[2];  // 蓝色通道
            let color = RgbwColor::from_rgb(r, g, b);  // 转换为RGBW
        }
    }
}
```

#### 3.3 RGBW结构体创建

```rust
impl RgbwColor {
    /// 从RGB转换为RGBW (白色通道设为0)
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, w: 0 }  // 保持原始RGB顺序，添加W=0
    }
}
```

### 4. 硬件输出重排序

#### 4.1 SK6812-RGBW硬件要求

SK6812-RGBW LED硬件要求特定的数据顺序：
- **硬件期望顺序**：`G, R, B, W` (绿, 红, 蓝, 白)
- **原因**：这是SK6812-RGBW芯片的内部寄存器顺序

#### 4.2 RMT输出重排序 (`src/led_control.rs`)

```rust
fn send_rgbw_batch(&mut self, colors: &[RgbwColor], add_reset: bool) -> Result<(), BoardError> {
    for color in colors {
        // 关键：重排序为SK6812-RGBW硬件要求的G,R,B,W顺序
        for &byte in &[color.g, color.r, color.b, color.w] {
            let byte_pulses = byte_to_pulses(byte);
            pulses.extend_from_slice(&byte_pulses);
        }
    }
}
```

## 🔄 完整数据流转换示例

### 示例：发送红色LED

1. **桌面端发送**：
   ```python
   rgb_data = [255, 0, 0]  # 红色
   ```

2. **UDP包格式**：
   ```
   [0x02] [0x00, 0x00] [255, 0, 0]
   ```

3. **ESP32解析**：
   ```rust
   let r = 255;  // chunk[0]
   let g = 0;    // chunk[1] 
   let b = 0;    // chunk[2]
   let color = RgbwColor { r: 255, g: 0, b: 0, w: 0 };
   ```

4. **硬件输出**：
   ```rust
   // 输出顺序：G, R, B, W
   [0, 255, 0, 0]  // 绿=0, 红=255, 蓝=0, 白=0
   ```

5. **LED显示结果**：红色LED点亮 ✅

## 🛡️ 中断保护机制

为了解决间歇性LED闪烁问题，实现了关键段保护：

```rust
// 关键段保护LED数据传输
let result = critical_section::with(|_| {
    println!("[LED] 🔒 Starting critical section for LED transmission");
    let transaction = channel.transmit(&pulses).map_err(|_| BoardError::LedError)?;
    
    match transaction.wait() {
        Ok(channel) => {
            println!("[LED] ✅ LED transmission completed successfully in critical section");
            Ok(channel)
        }
        Err((err, channel)) => {
            println!("[LED] ⚠️ RMT warning in critical section: {:?}", err);
            Ok(channel)
        }
    }
});
```

**保护原理**：
- 使用 `critical_section::with()` 禁用中断
- 防止embassy异步任务调度干扰RMT传输
- 确保LED数据传输的原子性和时序完整性

## 📊 性能特征

- **数据转换开销**：最小化，仅在输出时重排序
- **内存使用**：帧缓冲最大支持1000个LED
- **传输延迟**：关键段保护增加约几微秒延迟
- **稳定性**：中断保护消除了间歇性闪烁问题

## 🔧 调试信息

系统提供详细的调试日志：

```
[UDP] ✅ Parsed 3 RGB LEDs from packet
[LED] 🚀 Sending complete LED strip data: 3 LEDs
[LED] 🔒 Starting critical section for LED transmission
[LED] ✅ LED transmission completed successfully in critical section
[LED] 🔓 Critical section completed, LED data transmitted
```

## 📝 注意事项

1. **颜色顺序**：桌面端使用标准RGB顺序，ESP32自动处理硬件重排序
2. **数据完整性**：必须发送完整帧数据，不支持部分更新
3. **中断保护**：LED传输期间会短暂禁用中断，确保时序准确性
4. **帧缓冲**：支持分包传输，自动组装完整帧后再输出到LED

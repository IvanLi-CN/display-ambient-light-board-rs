# Board-RS 技术文档

本目录包含 Board-RS 项目的详细技术文档。

## 📚 文档索引

### 🏗️ [系统架构概览](ARCHITECTURE.md)
完整的系统架构设计、模块分析和数据流处理概览。包含：
- 系统整体架构图
- 核心模块功能说明
- 异步任务设计
- 性能指标和硬件配置

### 🎨 [颜色数据处理流程](COLOR_DATA_PROCESSING.md)
从桌面端RGB数据到ESP32硬件RGBW输出的完整处理流程。包含：
- 数据格式转换详解
- UDP协议解析过程
- RGB到RGBW颜色映射
- 硬件输出重排序机制
- 中断保护实现原理

## 🔍 快速导航

| 需求 | 推荐文档 |
|------|----------|
| 了解整体架构 | [ARCHITECTURE.md](ARCHITECTURE.md) |
| 理解颜色处理 | [COLOR_DATA_PROCESSING.md](COLOR_DATA_PROCESSING.md) |
| 解决LED闪烁问题 | [COLOR_DATA_PROCESSING.md#中断保护机制](COLOR_DATA_PROCESSING.md#中断保护机制) |
| 调试数据转换 | [COLOR_DATA_PROCESSING.md#完整数据流转换示例](COLOR_DATA_PROCESSING.md#完整数据流转换示例) |
| 性能优化 | [ARCHITECTURE.md#性能指标](ARCHITECTURE.md#性能指标) |

## 🛠️ 开发者指南

### 新手入门
1. 先阅读 [ARCHITECTURE.md](ARCHITECTURE.md) 了解整体设计
2. 查看 [COLOR_DATA_PROCESSING.md](COLOR_DATA_PROCESSING.md) 理解数据流
3. 参考主 [README.md](../README.md) 进行环境搭建

### 问题排查
- LED显示异常 → 查看颜色数据处理流程
- 网络连接问题 → 参考架构文档的网络层说明
- 性能问题 → 查看性能指标和优化建议

### 代码贡献
- 修改前请先理解相关模块的架构设计
- 确保变更符合现有的数据流处理模式
- 测试时注意中断保护机制的影响

## 📝 文档维护

这些文档与代码同步维护，如有不一致请及时更新。

### 更新原则
- 架构变更时更新 ARCHITECTURE.md
- 数据处理逻辑变更时更新 COLOR_DATA_PROCESSING.md
- 新增重要技术特性时添加相应文档

### 贡献指南
欢迎提交文档改进建议和补充内容。

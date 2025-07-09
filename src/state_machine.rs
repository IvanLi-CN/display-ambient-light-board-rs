//! 系统状态机模块
//!
//! 管理ESP32固件的所有系统状态，包括网络连接、服务通信、LED渲染等

use crate::led_control::LedStatus;
use esp_println::println;

/// 系统状态枚举 - 简化版本
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemState {
    // 初始化状态
    SystemInit,

    // 网络连接状态
    WiFiConnecting,
    DHCPRequesting,
    NetworkReady,

    // 服务状态
    UDPStarting,
    UDPListening,

    // 运行状态
    Operational,
    UDPTimeout, // 长时间未收到0x01消息

    // 错误状态
    WiFiError,
    DHCPError,
    UDPError,

    // 恢复状态
    Reconnecting,
}

/// 系统事件枚举 - 简化版本
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemEvent {
    // 系统事件
    SystemStarted,

    // 网络事件
    WiFiConnected,
    WiFiDisconnected,
    DHCPSuccess,
    DHCPFailed,

    // UDP事件
    UDPServerStarted,
    UDPServerFailed,
    ConnectionCheckReceived, // 收到0x01消息
    UDPTimeout,              // 长时间未收到0x01消息

    // 数据事件
    LEDDataReceived,

    // 错误和恢复事件
    WiFiConnectionFailed,
    RecoveryRequested,
    StateTimeout,
}

/// 状态转换结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateTransition {
    /// 保持当前状态
    Stay,
    /// 转换到新状态
    Transition(SystemState),
    /// 转换到新状态并重置重试计数
    TransitionWithReset(SystemState),
}

/// 状态机需要执行的动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// 更新LED状态显示
    UpdateLEDStatus(LedStatus),
    /// 启动WiFi连接
    StartWiFiConnection,
    /// 启动DHCP请求
    StartDHCPRequest,
    /// 启动网络服务
    StartNetworkServices,
    /// 启动UDP服务器
    StartUDPServer,
    /// 启动mDNS服务
    StartMDNSService,
    /// 处理LED数据
    ProcessLEDData,
    /// 监控网络连接
    MonitorConnection,
    /// 重启服务
    RestartServices,
    /// 系统恢复
    SystemRecover,
    /// 记录错误
    LogError(SystemState),
    /// 重置重试计数
    ResetRetryCount,
}

/// 错误上下文信息
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorContext {
    pub error_state: SystemState,
    pub error_count: u32,
    pub last_good_state: SystemState,
}

/// 系统状态机
pub struct SystemStateMachine {
    current_state: SystemState,
    previous_state: Option<SystemState>,
    state_entry_time: u64,
    retry_count: u32,
    error_context: Option<ErrorContext>,
    max_retries: u32,
    mdns_started: bool, // Track if mDNS has been started
}

impl SystemStateMachine {
    /// 创建新的状态机实例
    pub fn new() -> Self {
        Self {
            current_state: SystemState::SystemInit,
            previous_state: None,
            state_entry_time: 0,
            retry_count: 0,
            error_context: None,
            max_retries: 3,
            mdns_started: false,
        }
    }

    /// 获取当前状态
    pub fn get_current_state(&self) -> SystemState {
        self.current_state
    }

    /// 获取上一个状态
    pub fn get_previous_state(&self) -> Option<SystemState> {
        self.previous_state
    }

    /// 获取重试次数
    pub fn get_retry_count(&self) -> u32 {
        self.retry_count
    }

    /// 获取对应的LED状态
    pub fn get_led_status(&self) -> LedStatus {
        match self.current_state {
            SystemState::SystemInit => LedStatus::Starting,
            SystemState::WiFiConnecting => LedStatus::WiFiConnecting,
            SystemState::DHCPRequesting => LedStatus::DHCPRequesting,
            SystemState::NetworkReady => LedStatus::NetworkReady,
            SystemState::UDPStarting => LedStatus::UDPServerBinding,
            SystemState::UDPListening => LedStatus::UDPServerListening,
            SystemState::Operational => LedStatus::Operational,
            SystemState::UDPTimeout => LedStatus::ServiceError,
            SystemState::WiFiError => LedStatus::WiFiError,
            SystemState::DHCPError => LedStatus::NetworkError,
            SystemState::UDPError => LedStatus::ServiceError,
            SystemState::Reconnecting => LedStatus::Reconnecting,
        }
    }

    /// 处理系统事件
    pub fn handle_event(&mut self, event: SystemEvent) -> StateTransition {
        let transition = self.get_state_transition(self.current_state, event);

        match transition {
            StateTransition::Transition(new_state) => {
                self.transition_to_state(new_state);
            }
            StateTransition::TransitionWithReset(new_state) => {
                self.retry_count = 0;
                self.transition_to_state(new_state);
            }
            StateTransition::Stay => {
                // 保持当前状态，可能需要更新重试计数
            }
        }

        transition
    }

    /// 状态机更新，返回需要执行的动作
    pub fn update(&mut self) -> alloc::vec::Vec<Action> {
        let mut actions = alloc::vec::Vec::new();

        // 根据当前状态生成相应的动作
        match self.current_state {
            SystemState::SystemInit => {
                actions.push(Action::UpdateLEDStatus(LedStatus::Starting));
            }

            SystemState::WiFiConnecting => {
                actions.push(Action::UpdateLEDStatus(LedStatus::WiFiConnecting));
                actions.push(Action::StartWiFiConnection);
            }

            SystemState::DHCPRequesting => {
                actions.push(Action::UpdateLEDStatus(LedStatus::DHCPRequesting));
                actions.push(Action::StartDHCPRequest);
            }

            SystemState::NetworkReady => {
                actions.push(Action::UpdateLEDStatus(LedStatus::NetworkReady));
                actions.push(Action::StartNetworkServices);
            }

            SystemState::UDPStarting => {
                actions.push(Action::StartUDPServer);
            }

            SystemState::UDPListening => {
                // Start mDNS service only once when first entering this state
                if !self.mdns_started {
                    actions.push(Action::StartMDNSService);
                }
                actions.push(Action::MonitorConnection);
            }

            SystemState::Operational => {
                actions.push(Action::MonitorConnection);
                actions.push(Action::ProcessLEDData);
            }

            SystemState::UDPTimeout => {
                actions.push(Action::UpdateLEDStatus(LedStatus::ServiceError));
                actions.push(Action::LogError(self.current_state));
                if self.retry_count < self.max_retries {
                    actions.push(Action::RestartServices);
                }
            }

            // 错误状态处理
            SystemState::WiFiError => {
                actions.push(Action::UpdateLEDStatus(LedStatus::WiFiError));
                actions.push(Action::LogError(self.current_state));
                if self.retry_count < self.max_retries {
                    actions.push(Action::SystemRecover);
                }
            }

            SystemState::DHCPError => {
                actions.push(Action::UpdateLEDStatus(LedStatus::NetworkError));
                actions.push(Action::LogError(self.current_state));
                if self.retry_count < self.max_retries {
                    actions.push(Action::SystemRecover);
                }
            }

            SystemState::UDPError => {
                actions.push(Action::UpdateLEDStatus(LedStatus::ServiceError));
                actions.push(Action::LogError(self.current_state));
                if self.retry_count < self.max_retries {
                    actions.push(Action::RestartServices);
                }
            }

            SystemState::Reconnecting => {
                actions.push(Action::UpdateLEDStatus(LedStatus::Reconnecting));
                actions.push(Action::SystemRecover);
            }
        }

        actions
    }

    /// 内部状态转换逻辑
    fn transition_to_state(&mut self, new_state: SystemState) {
        if new_state != self.current_state {
            // Only print critical state changes
            match new_state {
                SystemState::Operational => println!("[STATE] System operational"),
                SystemState::WiFiError | SystemState::DHCPError | SystemState::UDPError => {
                    println!("[STATE] Error state: {:?}", new_state);
                }
                SystemState::UDPListening => {
                    // Reset mDNS flag when entering UDPListening state
                    self.mdns_started = false;
                }
                _ => {} // Silent for normal transitions
            }

            self.previous_state = Some(self.current_state);
            self.current_state = new_state;
            self.state_entry_time = 0; // 在实际实现中应该使用真实时间
        }
    }

    /// 获取状态转换规则 - 简化版本
    fn get_state_transition(
        &self,
        current_state: SystemState,
        event: SystemEvent,
    ) -> StateTransition {
        match (current_state, event) {
            // 系统启动流程
            (SystemState::SystemInit, SystemEvent::SystemStarted) => {
                StateTransition::Transition(SystemState::WiFiConnecting)
            }

            // WiFi连接流程 - WiFi连接成功后进行DHCP
            (SystemState::WiFiConnecting, SystemEvent::WiFiConnected) => {
                StateTransition::TransitionWithReset(SystemState::DHCPRequesting)
            }
            (SystemState::WiFiConnecting, SystemEvent::WiFiConnectionFailed) => {
                if self.retry_count < self.max_retries {
                    StateTransition::Stay // 继续重试
                } else {
                    StateTransition::Transition(SystemState::WiFiError)
                }
            }
            (SystemState::WiFiConnecting, SystemEvent::StateTimeout) => {
                StateTransition::Transition(SystemState::WiFiError)
            }

            // DHCP流程 - 获取IP后启动UDP服务
            (SystemState::DHCPRequesting, SystemEvent::DHCPSuccess) => {
                StateTransition::TransitionWithReset(SystemState::NetworkReady)
            }
            (SystemState::DHCPRequesting, SystemEvent::DHCPFailed) => {
                if self.retry_count < self.max_retries {
                    StateTransition::Stay // 继续重试DHCP
                } else {
                    StateTransition::Transition(SystemState::DHCPError)
                }
            }
            (SystemState::DHCPRequesting, SystemEvent::StateTimeout) => {
                StateTransition::Transition(SystemState::DHCPError)
            }

            // 网络就绪后启动UDP服务
            (SystemState::NetworkReady, SystemEvent::UDPServerStarted) => {
                StateTransition::Transition(SystemState::UDPStarting)
            }

            // UDP服务启动
            (SystemState::UDPStarting, SystemEvent::UDPServerStarted) => {
                StateTransition::TransitionWithReset(SystemState::UDPListening)
            }
            (SystemState::UDPStarting, SystemEvent::UDPServerFailed) => {
                StateTransition::Transition(SystemState::UDPError)
            }

            // UDP监听状态 - 收到0x01消息表示正常
            (SystemState::UDPListening, SystemEvent::ConnectionCheckReceived) => {
                StateTransition::TransitionWithReset(SystemState::Operational)
            }
            (SystemState::UDPListening, SystemEvent::UDPTimeout) => {
                StateTransition::Transition(SystemState::UDPTimeout)
            }

            // 正常运行状态
            (SystemState::Operational, SystemEvent::LEDDataReceived) => {
                StateTransition::Stay // 处理LED数据但保持运行状态
            }
            (SystemState::Operational, SystemEvent::ConnectionCheckReceived) => {
                StateTransition::Stay // 重置UDP超时计时器
            }
            (SystemState::Operational, SystemEvent::UDPTimeout) => {
                StateTransition::Transition(SystemState::UDPTimeout)
            }

            // UDP超时处理
            (SystemState::UDPTimeout, SystemEvent::ConnectionCheckReceived) => {
                StateTransition::TransitionWithReset(SystemState::Operational)
            }

            // WiFi断开处理 - 需要重置DHCP
            (_, SystemEvent::WiFiDisconnected) => {
                StateTransition::Transition(SystemState::Reconnecting)
            }

            // 重连流程
            (SystemState::Reconnecting, SystemEvent::WiFiConnected) => {
                StateTransition::Transition(SystemState::DHCPRequesting) // WiFi重连后重新DHCP
            }

            // 错误恢复
            (SystemState::WiFiError, SystemEvent::RecoveryRequested) => {
                StateTransition::Transition(SystemState::WiFiConnecting)
            }
            (SystemState::DHCPError, SystemEvent::RecoveryRequested) => {
                StateTransition::Transition(SystemState::DHCPRequesting)
            }
            (SystemState::UDPError, SystemEvent::RecoveryRequested) => {
                StateTransition::Transition(SystemState::UDPStarting)
            }
            (SystemState::UDPTimeout, SystemEvent::RecoveryRequested) => {
                StateTransition::Transition(SystemState::UDPStarting)
            }

            // 默认情况：保持当前状态
            _ => StateTransition::Stay,
        }
    }

    /// 检查是否需要重试
    pub fn should_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// 增加重试计数
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }

    /// 重置重试计数
    pub fn reset_retry_count(&mut self) {
        self.retry_count = 0;
    }

    /// 设置错误上下文
    pub fn set_error_context(&mut self, error_state: SystemState) {
        let last_good_state = self.previous_state.unwrap_or(SystemState::SystemInit);
        self.error_context = Some(ErrorContext {
            error_state,
            error_count: self.retry_count,
            last_good_state,
        });
    }

    /// 获取错误上下文
    pub fn get_error_context(&self) -> Option<ErrorContext> {
        self.error_context
    }

    /// 清除错误上下文
    pub fn clear_error_context(&mut self) {
        self.error_context = None;
    }

    /// 检查是否处于错误状态
    pub fn is_error_state(&self) -> bool {
        matches!(
            self.current_state,
            SystemState::WiFiError
                | SystemState::DHCPError
                | SystemState::UDPError
                | SystemState::UDPTimeout
        )
    }

    /// 检查是否处于运行状态
    pub fn is_operational(&self) -> bool {
        matches!(
            self.current_state,
            SystemState::Operational | SystemState::UDPListening
        )
    }

    /// 强制转换到指定状态（用于紧急情况）
    pub fn force_transition(&mut self, new_state: SystemState) {
        self.transition_to_state(new_state);
        self.retry_count = 0;
        self.error_context = None;
    }

    /// 标记 mDNS 服务已启动
    pub fn mark_mdns_started(&mut self) {
        self.mdns_started = true;
    }
}

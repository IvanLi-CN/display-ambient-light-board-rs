//! 状态机功能测试程序
//!
//! 这个测试程序验证状态机的各种状态转换和事件处理

#![no_std]
#![no_main]

extern crate alloc;

use board_rs::state_machine::{Action, SystemEvent, SystemState, SystemStateMachine};
use esp_hal::clock::CpuClock;
use esp_println::println;

// Add app descriptor for espflash compatibility
esp_bootloader_esp_idf::esp_app_desc!();

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[esp_hal::main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let _peripherals = esp_hal::init(config);

    // Initialize heap allocator
    esp_alloc::heap_allocator!(size: 32 * 1024);

    println!("=== 状态机功能测试 ===");

    // 创建状态机实例
    let mut state_machine = SystemStateMachine::new();

    // 测试初始状态
    println!("\n1. 测试初始状态");
    assert_eq!(state_machine.get_current_state(), SystemState::SystemInit);
    println!("✅ 初始状态正确: {:?}", state_machine.get_current_state());

    // 测试系统启动流程
    println!("\n2. 测试系统启动流程");

    // 系统启动 - 直接进入WiFi连接状态
    state_machine.handle_event(SystemEvent::SystemStarted);
    assert_eq!(
        state_machine.get_current_state(),
        SystemState::WiFiConnecting
    );
    println!(
        "✅ 系统启动事件处理正确: {:?}",
        state_machine.get_current_state()
    );

    // 测试WiFi连接流程
    println!("\n3. 测试WiFi连接流程");

    // WiFi连接成功 - 进入DHCP请求状态
    state_machine.handle_event(SystemEvent::WiFiConnected);
    assert_eq!(
        state_machine.get_current_state(),
        SystemState::DHCPRequesting
    );
    println!(
        "✅ WiFi连接事件处理正确: {:?}",
        state_machine.get_current_state()
    );

    // DHCP成功 - 进入网络就绪状态
    state_machine.handle_event(SystemEvent::DHCPSuccess);
    assert_eq!(state_machine.get_current_state(), SystemState::NetworkReady);
    println!(
        "✅ DHCP成功事件处理正确: {:?}",
        state_machine.get_current_state()
    );

    // 测试UDP服务启动流程
    println!("\n4. 测试UDP服务启动流程");

    // UDP服务器启动
    state_machine.handle_event(SystemEvent::UDPServerStarted);
    assert_eq!(state_machine.get_current_state(), SystemState::UDPStarting);
    println!(
        "✅ UDP服务器启动事件处理正确: {:?}",
        state_machine.get_current_state()
    );

    // UDP服务器启动完成
    state_machine.handle_event(SystemEvent::UDPServerStarted);
    assert_eq!(state_machine.get_current_state(), SystemState::UDPListening);
    println!(
        "✅ UDP服务器监听事件处理正确: {:?}",
        state_machine.get_current_state()
    );

    // 收到连接检查消息(0x01)
    state_machine.handle_event(SystemEvent::ConnectionCheckReceived);
    assert_eq!(state_machine.get_current_state(), SystemState::Operational);
    println!(
        "✅ 连接检查事件处理正确: {:?}",
        state_machine.get_current_state()
    );

    // 测试数据处理流程
    println!("\n5. 测试数据处理流程");

    // 接收LED数据
    state_machine.handle_event(SystemEvent::LEDDataReceived);
    assert_eq!(state_machine.get_current_state(), SystemState::Operational);
    println!(
        "✅ LED数据接收事件处理正确: {:?}",
        state_machine.get_current_state()
    );

    // 测试UDP超时处理
    println!("\n6. 测试UDP超时处理");

    // UDP超时（长时间未收到0x01消息）
    state_machine.handle_event(SystemEvent::UDPTimeout);
    assert_eq!(state_machine.get_current_state(), SystemState::UDPTimeout);
    println!(
        "✅ UDP超时事件处理正确: {:?}",
        state_machine.get_current_state()
    );

    // 收到连接检查消息，恢复正常
    state_machine.handle_event(SystemEvent::ConnectionCheckReceived);
    assert_eq!(state_machine.get_current_state(), SystemState::Operational);
    println!(
        "✅ UDP超时恢复事件处理正确: {:?}",
        state_machine.get_current_state()
    );

    // 测试错误处理
    println!("\n7. 测试错误处理");

    // WiFi断开连接
    state_machine.handle_event(SystemEvent::WiFiDisconnected);
    assert_eq!(state_machine.get_current_state(), SystemState::Reconnecting);
    println!(
        "✅ WiFi断开事件处理正确: {:?}",
        state_machine.get_current_state()
    );

    // 重新连接成功 - 需要重新DHCP
    state_machine.handle_event(SystemEvent::WiFiConnected);
    assert_eq!(
        state_machine.get_current_state(),
        SystemState::DHCPRequesting
    );
    println!(
        "✅ 重新连接事件处理正确: {:?}",
        state_machine.get_current_state()
    );

    // 测试动作生成
    println!("\n8. 测试动作生成");

    // 强制转换到WiFi连接状态
    state_machine.force_transition(SystemState::WiFiConnecting);
    let actions = state_machine.update();

    println!("状态 {:?} 生成的动作:", state_machine.get_current_state());
    for action in &actions {
        match action {
            Action::UpdateLEDStatus(status) => {
                println!("  - 更新LED状态: {:?}", status);
            }
            Action::StartWiFiConnection => {
                println!("  - 启动WiFi连接");
            }
            _ => {
                println!("  - 其他动作: {:?}", action);
            }
        }
    }

    // 测试LED状态映射
    println!("\n9. 测试LED状态映射");

    let test_states = [
        SystemState::SystemInit,
        SystemState::WiFiConnecting,
        SystemState::DHCPRequesting,
        SystemState::NetworkReady,
        SystemState::UDPListening,
        SystemState::Operational,
        SystemState::UDPTimeout,
        SystemState::WiFiError,
        SystemState::DHCPError,
        SystemState::UDPError,
        SystemState::Reconnecting,
    ];

    for state in test_states.iter() {
        state_machine.force_transition(*state);
        let led_status = state_machine.get_led_status();
        println!("系统状态 {:?} -> LED状态 {:?}", state, led_status);
    }

    println!("\n=== 所有测试通过! ===");
    println!("状态机功能验证完成，系统工作正常。");

    // 保持程序运行
    loop {
        // 简单的延迟
        for _ in 0..1000000 {
            unsafe {
                core::ptr::read_volatile(&0u32);
            }
        }
    }
}

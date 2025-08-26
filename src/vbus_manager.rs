use alloc::sync::Arc;
use embassy_stm32::gpio::Output;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::Timer;

use crate::{button::InputEvent, power_output::PowerOutput, InputSubscriber};

/// VBUS 电压阈值 (5.5V)
const VBUS_VOLTAGE_THRESHOLD: f64 = 5.5;

/// VBUS 管理器状态
#[derive(Debug, Clone, Copy, PartialEq, defmt::Format)]
pub enum VbusState {
    Disabled, // VBUS 输出关闭
    Enabled,  // VBUS 输出开启
}

impl Default for VbusState {
    fn default() -> Self {
        Self::Disabled
    }
}

/// VBUS LED 颜色状态
#[derive(Debug, Clone, Copy, PartialEq, defmt::Format)]
pub enum VbusLedColor {
    Green, // 绿色 LED (电压 < 5.5V)
    Red,   // 红色 LED (电压 >= 5.5V)
}

/// VBUS LED 显示模式
#[derive(Debug, Clone, Copy, PartialEq, defmt::Format)]
pub enum VbusLedMode {
    Blinking, // 闪烁 (VBUS 关闭时)
    Solid,    // 常亮 (VBUS 开启时)
}

/// VBUS 管理器上下文
pub struct VbusManagerContext<'d> {
    pub input_rx: Arc<Mutex<CriticalSectionRawMutex, InputSubscriber<'d>>>,
    pub vbus_output: PowerOutput<'d>, // PB7 VBUS 开关控制 (使用现有的 PowerOutput)
    pub vbus_led_pin: Arc<Mutex<CriticalSectionRawMutex, Output<'d>>>, // PB5 双色 LED 控制
}

/// VBUS 管理器
pub struct VbusManager<'d> {
    context: VbusManagerContext<'d>,
    pub vbus_state: VbusState,
    current_vbus_voltage: f64,
    current_vin_voltage: f64,
    led_color: VbusLedColor,
    led_mode: VbusLedMode,
    led_blink_state: bool,  // LED 闪烁状态
    led_blink_counter: u32, // LED 闪烁计数器
    tick_counter: u32,      // 用于定期状态报告
}

impl<'d> VbusManager<'d> {
    pub fn new(context: VbusManagerContext<'d>) -> Self {
        Self {
            context,
            vbus_state: VbusState::default(),
            current_vbus_voltage: 0.0,
            current_vin_voltage: 0.0,
            led_color: VbusLedColor::Green,
            led_mode: VbusLedMode::Blinking,
            led_blink_state: false,
            led_blink_counter: 0,
            tick_counter: 0,
        }
    }

    pub async fn init(&mut self) {
        // 初始化为关闭状态
        self.set_vbus_state(VbusState::Disabled).await;
        // 初始化 LED 状态（绿色，熄灭）
        self.set_led_hardware_off().await;
        defmt::info!("VbusManager initialized in Disabled state");
    }

    /// 检查并处理VBUS重置信号
    async fn check_vbus_reset(&mut self) {
        // 检查是否有VBUS重置信号
        if let Some(mut reset_rx) = crate::shared::VBUS_RESET_CHANNEL.receiver() {
            if let Some(reset_signal) = reset_rx.try_get() {
                if reset_signal {
                    defmt::info!("VBUS reset signal received - forcing VBUS to Disabled");
                    self.set_vbus_state(VbusState::Disabled).await;
                    // 清除重置信号
                    crate::shared::VBUS_RESET_CHANNEL.sender().send(false);
                }
            }
        }
    }

    /// 更新电压信息（由外部调用）
    pub fn update_voltages(&mut self, vbus_voltage: f64, vin_voltage: f64) {
        self.current_vbus_voltage = vbus_voltage;
        self.current_vin_voltage = vin_voltage;
    }

    /// 设置 VBUS 开关状态
    async fn set_vbus_state(&mut self, new_state: VbusState) {
        if self.vbus_state != new_state {
            defmt::info!(
                "VBUS state changing from {:?} to {:?}",
                self.vbus_state,
                new_state
            );
            self.vbus_state = new_state;

            // 更新硬件状态
            self.update_vbus_hardware().await;

            // 发送状态到共享通道
            let vbus_enabled = matches!(new_state, VbusState::Enabled);
            crate::shared::VBUS_STATE_CHANNEL
                .sender()
                .send(vbus_enabled);
        }
    }

    /// 更新 VBUS 硬件开关状态
    async fn update_vbus_hardware(&mut self) {
        match self.vbus_state {
            VbusState::Disabled => {
                self.context.vbus_output.set_off().await;
                defmt::info!("VBUS output DISABLED (PB7 = LOW)");
            }
            VbusState::Enabled => {
                self.context.vbus_output.set_on().await;
                defmt::info!("VBUS output ENABLED (PB7 = HIGH)");
            }
        }
    }

    /// 切换 VBUS 开关状态
    pub async fn toggle_vbus(&mut self) {
        let new_state = match self.vbus_state {
            VbusState::Disabled => VbusState::Enabled,
            VbusState::Enabled => VbusState::Disabled,
        };
        self.set_vbus_state(new_state).await;
    }

    /// 处理按键事件
    async fn handle_button_event(&mut self, event: InputEvent) {
        match event {
            InputEvent::Click => {
                defmt::info!("VBUS: Short press detected - toggling VBUS state");
                self.toggle_vbus().await;
            }
            _ => {
                // 其他事件由 PowerManager 处理，这里忽略
                defmt::debug!("VBUS: Ignoring button event: {:?}", event);
            }
        }
    }

    /// 主循环 tick
    pub async fn tick(&mut self) {
        // 处理按键输入
        let event = {
            let mut input_rx = self.context.input_rx.lock().await;
            input_rx.try_next_message_pure()
        };

        if let Some(event) = event {
            self.handle_button_event(event).await;
        }

        // 电压数据由外部通过 update_voltages 方法更新

        // 检查VBUS重置信号
        self.check_vbus_reset().await;

        // 更新 LED 状态
        self.update_led_display().await;

        // 定期状态报告（每10秒一次）
        self.tick_counter += 1;
        if self.tick_counter % 500 == 0 {
            // 500 * 20ms = 10秒
            defmt::info!(
                "VbusManager status: State={:?}, VBUS={}V, VIN={}V, LED={:?}/{:?}, Tick={}",
                self.vbus_state,
                self.current_vbus_voltage,
                self.current_vin_voltage,
                self.led_color,
                self.led_mode,
                self.tick_counter
            );
        }

        // 添加小延迟
        Timer::after_millis(20).await; // 50Hz更新频率
    }

    /// 更新 LED 显示状态
    async fn update_led_display(&mut self) {
        // 确定 LED 颜色
        let new_led_color = if self.current_vbus_voltage < VBUS_VOLTAGE_THRESHOLD {
            VbusLedColor::Green
        } else {
            VbusLedColor::Red
        };

        // 确定 LED 模式
        let new_led_mode = match self.vbus_state {
            VbusState::Disabled => VbusLedMode::Blinking,
            VbusState::Enabled => VbusLedMode::Solid,
        };

        // 更新 LED 颜色状态
        if self.led_color != new_led_color {
            defmt::info!(
                "VBUS LED color changing from {:?} to {:?} (voltage: {}V)",
                self.led_color,
                new_led_color,
                self.current_vbus_voltage
            );
            self.led_color = new_led_color;
        }

        // 更新 LED 模式状态
        if self.led_mode != new_led_mode {
            defmt::info!(
                "VBUS LED mode changing from {:?} to {:?} (VBUS state: {:?})",
                self.led_mode,
                new_led_mode,
                self.vbus_state
            );
            self.led_mode = new_led_mode;
        }

        // 处理 LED 显示逻辑
        self.update_led_hardware().await;
    }

    /// 更新 LED 硬件显示
    async fn update_led_hardware(&mut self) {
        match self.led_mode {
            VbusLedMode::Solid => {
                // 常亮模式
                self.set_led_hardware_color(self.led_color).await;
            }
            VbusLedMode::Blinking => {
                // 闪烁模式
                self.led_blink_counter += 1;
                if self.led_blink_counter >= 25 {
                    // 25 * 20ms = 500ms，切换闪烁状态
                    self.led_blink_state = !self.led_blink_state;
                    self.led_blink_counter = 0;
                }

                if self.led_blink_state {
                    self.set_led_hardware_color(self.led_color).await;
                } else {
                    self.set_led_hardware_off().await;
                }
            }
        }
    }

    /// 设置 LED 硬件颜色
    async fn set_led_hardware_color(&mut self, color: VbusLedColor) {
        let mut vbus_led_pin = self.context.vbus_led_pin.lock().await;
        match color {
            VbusLedColor::Green => {
                // 绿色 LED: PB5 输出低电平
                vbus_led_pin.set_low();
            }
            VbusLedColor::Red => {
                // 红色 LED: PB5 输出高电平
                vbus_led_pin.set_high();
            }
        }
    }

    /// 设置 LED 硬件为熄灭状态
    async fn set_led_hardware_off(&mut self) {
        // 根据硬件连接方式，这里使用绿色状态（低电平）作为"熄灭"状态
        // 实际硬件可能需要不同的控制方式
        let mut vbus_led_pin = self.context.vbus_led_pin.lock().await;
        vbus_led_pin.set_low();
    }
}

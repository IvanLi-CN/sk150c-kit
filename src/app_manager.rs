use alloc::sync::Arc;
use embassy_stm32::{
    gpio::Output, peripherals::TIM1, timer::simple_pwm::SimplePwm, timer::Channel,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::Timer;
use embedded_hal_02::Pwm;

use crate::{button::InputEvent, InputSubscriber};

/// 全局系统状态
#[derive(Debug, Clone, Copy, PartialEq, defmt::Format)]
pub enum SystemState {
    Standby, // 待机状态：VIN_EN=LOW, VBUS_EN=LOW, 电源LED呼吸
    Working, // 工作状态：VIN_EN=HIGH, VBUS_EN可切换, 电源LED根据VBUS状态
}

/// 电源LED状态
#[derive(Debug, Clone, Copy, PartialEq, defmt::Format)]
pub enum PowerLedState {
    Off,       // LED 熄灭
    Breathing, // LED 呼吸效果（VIN 关闭时）
    SolidOn,   // LED 常亮（VIN + VBUS 都开启时）
}

impl Default for SystemState {
    fn default() -> Self {
        Self::Standby
    }
}

impl Default for PowerLedState {
    fn default() -> Self {
        Self::Off
    }
}

/// 电源管理器上下文
pub struct PowerManagerContext<'d> {
    pub input_rx: Arc<Mutex<CriticalSectionRawMutex, InputSubscriber<'d>>>,
    pub power_switch: Arc<Mutex<CriticalSectionRawMutex, Output<'d>>>, // PA15 控制电源开关
    pub led_pwm: Arc<Mutex<CriticalSectionRawMutex, SimplePwm<'d, TIM1>>>, // PA8 PWM 控制LED
}

/// 全局系统管理器
pub struct PowerManager<'d> {
    context: PowerManagerContext<'d>,
    pub system_state: SystemState,
    led_state: PowerLedState,
    current_vin_voltage: f64,
    current_vbus_voltage: f64,
    current_vbus_enabled: bool,
    breathing_counter: u32, // 呼吸效果计数器
    tick_counter: u32,      // 用于定期状态报告
}

impl<'d> PowerManager<'d> {
    pub fn new(context: PowerManagerContext<'d>) -> Self {
        Self {
            context,
            system_state: SystemState::default(),
            led_state: PowerLedState::default(),
            current_vin_voltage: 0.0,
            current_vbus_voltage: 0.0,
            current_vbus_enabled: false,
            breathing_counter: 0,
            tick_counter: 0,
        }
    }

    pub async fn init(&mut self) {
        // 初始化为待机状态
        self.set_system_state(SystemState::Standby).await;
        defmt::info!("PowerManager initialized in Standby state");
    }

    /// 更新电压信息（仅用于监控和LED显示）
    pub fn update_voltages(&mut self, vin_voltage: f64, vbus_voltage: f64, vbus_enabled: bool) {
        self.current_vin_voltage = vin_voltage;
        self.current_vbus_voltage = vbus_voltage;
        self.current_vbus_enabled = vbus_enabled;
    }

    /// 切换系统状态（由按键触发）
    pub async fn toggle_system_state(&mut self) {
        let new_state = match self.system_state {
            SystemState::Standby => SystemState::Working,
            SystemState::Working => SystemState::Standby,
        };

        defmt::info!(
            "System state toggling from {:?} to {:?}",
            self.system_state,
            new_state
        );

        // 关键修复：当从Standby切换到Working时，需要重置VBUS状态
        if self.system_state == SystemState::Standby && new_state == SystemState::Working {
            defmt::info!("VIN re-enabled: Broadcasting VBUS reset signal");
            // 立即更新本地VBUS状态，确保LED逻辑正确
            self.current_vbus_enabled = false;
            // 发送VBUS重置信号到共享通道
            crate::shared::VBUS_RESET_CHANNEL.sender().send(true);
        }

        self.set_system_state(new_state).await;
    }

    /// 设置系统状态
    async fn set_system_state(&mut self, new_state: SystemState) {
        if self.system_state != new_state {
            defmt::info!(
                "System state changing from {:?} to {:?}",
                self.system_state,
                new_state
            );
            self.system_state = new_state;

            // 同步更新硬件状态
            self.update_hardware_state().await;
        }
    }

    /// 更新硬件状态（LED和电源开关）
    async fn update_hardware_state(&mut self) {
        // 更新VIN开关状态 (PA15 - VIN_EN)
        // 根据硬件指南：高电平导通，低电平关断
        match self.system_state {
            SystemState::Standby => {
                // 待机状态：VIN关闭，PA15输出低电平（关断）
                {
                    let mut power_switch = self.context.power_switch.lock().await;
                    power_switch.set_low();
                }
                defmt::info!("VIN_EN (PA15) = LOW - Standby mode, VIN disabled");
            }
            SystemState::Working => {
                // 工作状态：VIN开启，PA15输出高电平（导通）
                {
                    let mut power_switch = self.context.power_switch.lock().await;
                    power_switch.set_high();
                }
                defmt::info!("VIN_EN (PA15) = HIGH - Working mode, VIN enabled");
            }
        }

        // 更新LED状态
        self.update_led_state().await;
    }

    /// 设置LED的PWM占空比
    async fn set_led_duty(&mut self, duty_percent: u8) {
        let mut pwm = self.context.led_pwm.lock().await;
        let max_duty = pwm.get_max_duty();
        // 计算实际占空比值，注意开漏输出是反向的（100% - duty_percent）
        let actual_duty = max_duty * (100 - duty_percent as u32) / 100;
        pwm.set_duty(Channel::Ch1, actual_duty);
        // LED占空比已设置，不再打印日志以减少输出
    }

    /// 更新LED状态
    async fn update_led_state(&mut self) {
        // 根据系统状态和VBUS状态确定LED状态
        let new_led_state = match self.system_state {
            SystemState::Standby => PowerLedState::Breathing,
            SystemState::Working => {
                if self.current_vbus_enabled {
                    PowerLedState::SolidOn
                } else {
                    PowerLedState::Off
                }
            }
        };

        // 如果LED状态发生变化，更新状态
        if self.led_state != new_led_state {
            defmt::info!(
                "🔄 LED state changing from {:?} to {:?} (VBUS_EN={})",
                self.led_state,
                new_led_state,
                self.current_vbus_enabled
            );
            self.led_state = new_led_state;
        }
    }

    /// 更新LED显示
    async fn update_led_display(&mut self) {
        match self.led_state {
            PowerLedState::Off => {
                // LED熄灭
                self.set_led_duty(0).await;
            }
            PowerLedState::SolidOn => {
                // LED常亮
                self.set_led_duty(100).await;
            }
            PowerLedState::Breathing => {
                // 呼吸效果：3秒周期 (150 * 20ms = 3000ms)
                self.breathing_counter += 1;
                if self.breathing_counter >= 150 {
                    self.breathing_counter = 0;
                }

                // 简化的呼吸效果：三角波
                let brightness = if self.breathing_counter < 75 {
                    // 上升阶段：0% -> 100%
                    (self.breathing_counter as f32 / 75.0) * 100.0
                } else {
                    // 下降阶段：100% -> 0%
                    ((150 - self.breathing_counter) as f32 / 75.0) * 100.0
                };
                self.set_led_duty(brightness as u8).await;
            }
        }
    }

    pub async fn tick(&mut self) {
        // 处理按键输入
        let event = {
            let mut input_rx = self.context.input_rx.lock().await;
            input_rx.try_next_message_pure()
        };

        if let Some(event) = event {
            defmt::info!("Button event received: {:?}", event);
            match event {
                InputEvent::LongReleased => {
                    defmt::info!("Power button long press released - toggling system state");
                    // PB8长按释放，切换系统状态
                    self.toggle_system_state().await;
                }
                _ => {
                    defmt::info!("Other button event: {:?}, ignoring", event);
                }
            }
        }

        // 每个tick都更新LED状态，确保状态同步
        self.update_led_state().await;

        // 更新LED显示
        self.update_led_display().await;

        // 定期状态报告（每5秒一次）
        self.tick_counter += 1;
        if self.tick_counter % 250 == 0 {
            // 250 * 20ms = 5秒
            defmt::info!(
                "PowerManager status: State={:?}, LED={:?}, VIN={}V, VBUS={}V, VBUS_EN={}, Tick={}",
                self.system_state,
                self.led_state,
                self.current_vin_voltage,
                self.current_vbus_voltage,
                self.current_vbus_enabled,
                self.tick_counter
            );
        }

        // 添加小延迟
        Timer::after_millis(20).await; // 50Hz更新频率，确保呼吸灯平滑
    }
}

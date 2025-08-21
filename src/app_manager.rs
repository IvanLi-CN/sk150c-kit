use alloc::sync::Arc;
use embassy_stm32::{
    gpio::{Level, Output},
    peripherals::TIM1,
    timer::simple_pwm::SimplePwm,
    timer::Channel,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::Timer;
use embedded_hal_02::Pwm;

use crate::{button::InputEvent, InputSubscriber};

/// 电源管理状态
#[derive(Debug, Clone, Copy, PartialEq, defmt::Format)]
pub enum PowerState {
    Standby, // 待机状态：呼吸灯 + 电源开关断开
    Working, // 工作状态：LED熄灭 + 电源开关闭合
}

impl Default for PowerState {
    fn default() -> Self {
        Self::Standby
    }
}

/// 电源管理器上下文
pub struct PowerManagerContext<'d> {
    pub input_rx: Arc<Mutex<CriticalSectionRawMutex, InputSubscriber<'d>>>,
    pub power_switch: Arc<Mutex<CriticalSectionRawMutex, Output<'d>>>, // PA15 控制电源开关
    pub led_pwm: Arc<Mutex<CriticalSectionRawMutex, SimplePwm<'d, TIM1>>>, // PA8 PWM 控制LED
}

/// 电源管理器
pub struct PowerManager<'d> {
    context: PowerManagerContext<'d>,
    state: PowerState,
    breathing_phase: f32, // 呼吸灯相位 (0.0 to 2π)
    tick_counter: u32,    // 用于定期状态报告
}

impl<'d> PowerManager<'d> {
    pub fn new(context: PowerManagerContext<'d>) -> Self {
        Self {
            context,
            state: PowerState::default(),
            breathing_phase: 0.0,
            tick_counter: 0,
        }
    }

    pub async fn init(&mut self) {
        // 初始化为待机状态
        self.set_state(PowerState::Standby).await;
        defmt::info!("PowerManager initialized in Standby state");
    }

    /// 设置电源管理状态
    async fn set_state(&mut self, new_state: PowerState) {
        if self.state != new_state {
            defmt::info!(
                "Power state changing from {:?} to {:?}",
                self.state,
                new_state
            );
            self.state = new_state;

            // 同步更新硬件状态
            self.update_hardware_state().await;
        }
    }

    /// 更新硬件状态（LED和电源开关）
    async fn update_hardware_state(&mut self) {
        match self.state {
            PowerState::Standby => {
                // 待机状态：电源开关关断（低电平）
                {
                    let mut power_switch = self.context.power_switch.lock().await;
                    power_switch.set_low();
                }
                defmt::info!("Power switch OFF (standby mode)");
            }
            PowerState::Working => {
                // 工作状态：电源开关导通（高电平），LED熄灭
                {
                    let mut power_switch = self.context.power_switch.lock().await;
                    power_switch.set_high();
                }
                self.set_led_duty(0).await; // LED熄灭
                defmt::info!("Power switch ON (working mode), LED OFF");
            }
        }
    }

    /// 设置LED的PWM占空比
    async fn set_led_duty(&mut self, duty_percent: u8) {
        let mut pwm = self.context.led_pwm.lock().await;
        let max_duty = pwm.get_max_duty();
        // 计算实际占空比值，注意开漏输出是反向的（100% - duty_percent）
        let actual_duty = max_duty * (100 - duty_percent as u32) / 100;
        pwm.set_duty(Channel::Ch1, actual_duty as u32);
        // LED占空比已设置，不再打印日志以减少输出
    }

    /// 更新呼吸灯效果（仅在待机状态下调用）
    async fn update_breathing_led(&mut self) {
        if self.state == PowerState::Standby {
            // 3秒周期的呼吸灯效果
            // 使用正弦波计算占空比 (0-100%)
            let duty_percent = ((libm::sinf(self.breathing_phase) + 1.0) * 50.0) as u8;
            self.set_led_duty(duty_percent).await;

            // 更新相位，3秒完成一个周期
            // 假设每20ms调用一次，3秒 = 3000ms = 150次调用
            self.breathing_phase += 2.0 * core::f32::consts::PI / 150.0;
            if self.breathing_phase >= 2.0 * core::f32::consts::PI {
                self.breathing_phase = 0.0;
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
                InputEvent::SingleLongReleased => {
                    defmt::info!("Power button long press released - switching state");
                    // PB8长按释放，切换电源状态
                    let new_state = match self.state {
                        PowerState::Standby => PowerState::Working,
                        PowerState::Working => PowerState::Standby,
                    };
                    defmt::info!(
                        "Triggering power state change from {:?} to {:?}",
                        self.state,
                        new_state
                    );
                    self.set_state(new_state).await;
                }
                _ => {
                    defmt::info!("Other button event: {:?}, ignoring", event);
                }
            }
        }

        // 更新呼吸灯效果（仅在待机状态）
        self.update_breathing_led().await;

        // 定期状态报告（每5秒一次）
        self.tick_counter += 1;
        if self.tick_counter % 250 == 0 {
            // 250 * 20ms = 5秒
            defmt::info!(
                "PowerManager status: State={:?}, Phase={}, Tick={}",
                self.state,
                self.breathing_phase,
                self.tick_counter
            );
        }

        // 添加小延迟
        Timer::after_millis(20).await; // 50Hz更新频率，确保呼吸灯平滑
    }
}

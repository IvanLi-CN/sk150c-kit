use alloc::{sync::Arc, vec::Vec};
use core::cmp::min;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::{with_timeout, Duration, Ticker};
use uom::si::{electric_current::milliampere, electric_potential::millivolt};
use usbpd::protocol_layer::message::units::{ElectricCurrent, ElectricPotential};

use crate::{
    button::{ButtonCode, InputEvent},
    config_manager::{Config, ConfigAgent},
    power::{self, PdoType, TargetPower},
    power_output::PowerOutput,
    InputSubscriber,
};

#[derive(defmt::Format)]
pub enum AppMode {
    Monitor,
    MainMenu(u8), // 主菜单，参数是光标位置
    PDOs,         // PDO列表页面
    Setting,
    VoltageSetting,
    ProtectSetting,
    About,
}

impl Default for AppMode {
    fn default() -> Self {
        Self::Monitor
    }
}

#[derive(Clone)]
pub struct AppContext<'d> {
    pub config: Arc<ConfigAgent<'d>>,
    pub input_rx: Arc<Mutex<CriticalSectionRawMutex, InputSubscriber<'d>>>,
    pub output: Arc<PowerOutput<'d>>,
    pub sink: Arc<power::SinkAgent<'d>>,
}

pub struct AppManager<'d> {
    context: Mutex<CriticalSectionRawMutex, AppContext<'d>>,
    mode: AppMode,
}

impl<'d> AppManager<'d> {
    pub fn new(context: AppContext<'d>) -> Self {
        Self {
            context: Mutex::new(context),
            mode: AppMode::default(),
        }
    }

    pub async fn init(&mut self) {
        self.mode = AppMode::Monitor;

        let ctx = self.context.lock().await;
        let config = ctx.config.snapshot().await;

        let initial_request = TargetPower {
            voltage: config.target_voltage,
            current: config.target_current,
        };

        match with_timeout(Duration::from_secs(2), ctx.sink.request(initial_request)).await {
            Ok(Ok(_)) => {
                // Request successful
                defmt::info!("Initial power request successful");
            }
            Ok(Err(err)) => {
                // Request failed, but we got a response
                defmt::error!("Initial power request failed: {}", err);
                ctx.output.set_off().await;
                ctx.sink.request(Default::default()).await.ok(); // Request 5V, ignore result
            }
            Err(_) => {
                // Request timed out, likely not a PD source
                defmt::warn!("Initial power request timed out. Assuming non-PD source.");
                ctx.output.set_off().await;
            }
        }

        defmt::info!("AppManager initialized successfully");
    }

    pub async fn tick(&mut self) {
        // 简化的 tick 实现，只处理基本的按键输入
        let ctx = self.context.lock().await;
        let mut input_rx = ctx.input_rx.lock().await;

        if let Some(event) = input_rx.try_next_message_pure() {
            match event {
                InputEvent::SingleClick(btn_code) => {
                    defmt::info!("Button clicked: {:?}", btn_code);
                    match btn_code {
                        ButtonCode::BtnRB => {
                            // 切换输出状态
                            ctx.output.toggle().await;
                            defmt::info!("Output toggled to: {}", ctx.output.get_state().await);
                        }
                        _ => {
                            defmt::info!("Button {:?} pressed but not handled", btn_code);
                        }
                    }
                }
                InputEvent::SingleHolding(btn_code) => {
                    defmt::info!("Button holding: {:?}", btn_code);
                }
                InputEvent::SingleLongReleased(btn_code) => {
                    defmt::info!("Button long released: {:?}", btn_code);
                }
                InputEvent::DualLongReleased(btn_code1, btn_code2) => {
                    defmt::info!(
                        "Dual button long released: {:?}, {:?}",
                        btn_code1,
                        btn_code2
                    );
                }
            }
        }

        // 添加小延迟
        embassy_time::Timer::after_millis(10).await;
    }
}

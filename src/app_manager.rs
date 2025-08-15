use alloc::sync::Arc;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};

use crate::{
    button::{ButtonCode, InputEvent},
    config_manager::ConfigAgent,
    power,
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

        // 使用简化的"贪婪"策略，自动请求最高电压
        // 不需要手动请求，USB PD 策略引擎会自动处理
        defmt::info!(
            "Using simplified greedy USB PD strategy - will automatically request highest voltage"
        );

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

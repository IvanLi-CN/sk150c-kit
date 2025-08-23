use defmt_rtt as _;
use embassy_stm32::gpio::Output;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Receiver};
use embassy_time::Instant;

/// 风扇管理器状态
#[derive(Debug, Clone, Copy, PartialEq)]
enum FanManagerState {
    StartupTest,     // 启动测试阶段（前5秒）
    NormalOperation, // 正常运行阶段
}

/// 风扇管理器
///
/// 负责根据温度自动控制风扇开关，实现5°C滞回控制：
/// - 启动前5秒：风扇测试运行
/// - 温度 ≥ 50°C 时启动风扇
/// - 温度 ≤ 45°C 时停止风扇
/// - 5°C滞回防止频繁开关
pub struct FanManager<'d> {
    fan_pin: Output<'d>,
    temperature_rx: Receiver<'d, CriticalSectionRawMutex, f64, 1>,
    current_temperature: f64,
    fan_enabled: bool,
    tick_counter: u32,
    state: FanManagerState,
    startup_time: Instant,
}

impl<'d> FanManager<'d> {
    /// 风扇启动温度阈值 (°C)
    const HIGH_TEMP_THRESHOLD: f64 = 50.0;

    /// 风扇停止温度阈值 (°C)
    const LOW_TEMP_THRESHOLD: f64 = 45.0;

    /// 温度异常检测阈值 (°C) - 超过此温度可能是传感器故障
    const TEMP_ANOMALY_THRESHOLD: f64 = 100.0;

    /// 创建新的风扇管理器
    ///
    /// # 参数
    /// - `fan_pin`: 风扇控制GPIO引脚 (PB10)
    /// - `temperature_rx`: 温度数据接收器
    pub fn new(
        mut fan_pin: Output<'d>,
        temperature_rx: Receiver<'d, CriticalSectionRawMutex, f64, 1>,
    ) -> Self {
        defmt::info!("🌀 Fan Manager initialized");
        defmt::info!("   High temp threshold: {}°C", Self::HIGH_TEMP_THRESHOLD);
        defmt::info!("   Low temp threshold: {}°C", Self::LOW_TEMP_THRESHOLD);
        defmt::info!("   Starting 5-second fan test...");

        // 启动测试：立即启动风扇
        fan_pin.set_high();

        Self {
            fan_pin,
            temperature_rx,
            current_temperature: 25.0, // 假设初始室温
            fan_enabled: true,         // 启动测试期间风扇开启
            tick_counter: 0,
            state: FanManagerState::StartupTest,
            startup_time: Instant::now(),
        }
    }

    /// 执行一次风扇管理检查
    ///
    /// 应该每5秒调用一次，与ADC采样频率同步
    pub async fn tick(&mut self) {
        self.tick_counter += 1;

        match self.state {
            FanManagerState::StartupTest => {
                // 启动测试阶段：检查是否已经运行了5秒
                let elapsed = Instant::now().duration_since(self.startup_time);
                if elapsed.as_secs() >= 5 {
                    // 5秒测试完成，切换到正常运行模式
                    defmt::info!(
                        "🌀 Fan test completed after {} seconds, switching to normal operation",
                        elapsed.as_secs()
                    );
                    self.state = FanManagerState::NormalOperation;
                    self.fan_pin.set_low(); // 关闭风扇
                    self.fan_enabled = false;
                    defmt::info!("🛑 Fan DISABLED after startup test");
                } else {
                    // 测试仍在进行中
                    defmt::info!("🌀 Fan test running... elapsed: {}s", elapsed.as_secs());
                }
            }
            FanManagerState::NormalOperation => {
                // 正常运行阶段：根据温度控制风扇
                if let Some(temperature) = self.temperature_rx.try_get() {
                    self.current_temperature = temperature;

                    // 检查温度异常
                    if temperature > Self::TEMP_ANOMALY_THRESHOLD {
                        defmt::warn!(
                            "⚠️ Temperature anomaly detected: {}°C (>{}°C)",
                            temperature,
                            Self::TEMP_ANOMALY_THRESHOLD
                        );
                        // 温度异常时保持当前风扇状态，不做改变
                        return;
                    }

                    // 更新风扇状态
                    self.update_fan_state(temperature).await;
                }

                // 定期状态报告（每分钟一次，即12个5秒周期）
                if self.tick_counter % 12 == 0 {
                    defmt::info!(
                        "🌡️ Temperature: {}°C, Fan: {}",
                        self.current_temperature,
                        if self.fan_enabled { "ON" } else { "OFF" }
                    );
                }
            }
        }
    }

    /// 根据温度更新风扇状态
    ///
    /// 实现5°C滞回控制逻辑
    async fn update_fan_state(&mut self, temperature: f64) {
        let should_enable = if self.fan_enabled {
            // 风扇当前开启，只有温度降到45°C以下才关闭
            temperature > Self::LOW_TEMP_THRESHOLD
        } else {
            // 风扇当前关闭，温度达到50°C以上才开启
            temperature >= Self::HIGH_TEMP_THRESHOLD
        };

        // 只有状态发生变化时才更新硬件和日志
        if should_enable != self.fan_enabled {
            self.fan_enabled = should_enable;

            if should_enable {
                self.fan_pin.set_high();
                defmt::info!(
                    "🌀 Fan ENABLED at {}°C (threshold: {}°C)",
                    temperature,
                    Self::HIGH_TEMP_THRESHOLD
                );
            } else {
                self.fan_pin.set_low();
                defmt::info!(
                    "🛑 Fan DISABLED at {}°C (threshold: {}°C)",
                    temperature,
                    Self::LOW_TEMP_THRESHOLD
                );
            }
        }
    }

    /// 获取当前风扇状态
    pub fn is_fan_enabled(&self) -> bool {
        self.fan_enabled
    }

    /// 获取当前温度
    pub fn current_temperature(&self) -> f64 {
        self.current_temperature
    }
}

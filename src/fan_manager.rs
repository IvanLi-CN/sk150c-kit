use crate::shared::{
    CURRENT_FAN_RPM, FAN_MAX_DETECTION_TIME_MS, FAN_PULSES_PER_REVOLUTION, FAN_TIMER_FREQ_HZ,
    MAX_FAN_RPM,
};
use defmt_rtt as _;
use embassy_stm32::{
    gpio::Output, gpio::Pull, peripherals::TIM3, time::Hertz, timer::pwm_input::PwmInput, Peri,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Receiver};
use embassy_time::{Instant, Timer};

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

/// 计算风扇转速 (RPM)
///
/// # 参数
/// - `period_ticks`: PWM 输入测量的周期计数
///
/// # 返回
/// 转速值 (RPM)，如果无信号则返回 0
fn calculate_rpm(period_ticks: u32) -> u32 {
    if period_ticks == 0 {
        return 0;
    }

    // 计算信号频率 (Hz)
    let signal_freq = FAN_TIMER_FREQ_HZ / period_ticks;

    // 转换为 RPM：频率 * 60 / 每转脉冲数
    let rpm = (signal_freq * 60) / FAN_PULSES_PER_REVOLUTION;

    // 合理性检查：风扇转速通常在 0-10000 RPM 范围内
    if rpm > 10000 {
        defmt::warn!("⚠️ Abnormal fan speed detected: {} RPM, ignoring", rpm);
        return 0;
    }

    rpm
}

/// 风扇转速采样任务
///
/// 此任务负责：
/// 1. 初始化 PWM 输入功能
/// 2. 前5秒进行最高转速检测
/// 3. 持续采样并输出转速数据
pub async fn fan_speed_sampling_task(
    tim3: Peri<'static, TIM3>,
    fan_touch_pin: Peri<
        'static,
        impl embassy_stm32::timer::TimerPin<TIM3, embassy_stm32::timer::Ch1>,
    >,
) {
    defmt::info!("🌀 Starting fan speed sampling task");

    // 创建 PWM 输入实例
    let mut pwm_input =
        PwmInput::new_ch1(tim3, fan_touch_pin, Pull::Up, Hertz::hz(FAN_TIMER_FREQ_HZ));

    // 启用 PWM 输入
    pwm_input.enable();
    defmt::info!("🌀 PWM input enabled for fan speed measurement");

    let start_time = Instant::now();
    let mut max_rpm_detected = 0u32;
    let mut sample_count = 0u32;
    let mut log_counter = 0u32;

    loop {
        // 获取周期计数并计算转速
        let period_ticks = pwm_input.get_period_ticks();
        let current_rpm = calculate_rpm(period_ticks);

        sample_count += 1;

        // 检查是否在最高转速检测期间（前5秒）
        let elapsed_ms = Instant::now().duration_since(start_time).as_millis();
        let is_max_detection_phase = elapsed_ms < FAN_MAX_DETECTION_TIME_MS;

        if is_max_detection_phase {
            // 最高转速检测阶段
            if current_rpm > max_rpm_detected {
                max_rpm_detected = current_rpm;
                defmt::info!("🌀 New max RPM detected: {} RPM", max_rpm_detected);
            }
        } else if sample_count > 0 && elapsed_ms >= FAN_MAX_DETECTION_TIME_MS {
            // 检测阶段刚结束，保存最高转速（只执行一次）
            static mut MAX_RPM_SAVED: bool = false;
            if unsafe { !MAX_RPM_SAVED } {
                unsafe {
                    MAX_RPM_SAVED = true;
                }
                // 保存最高转速到全局变量
                *MAX_FAN_RPM.lock().await = max_rpm_detected;
                defmt::info!(
                    "🌀 Max RPM detection completed: {} RPM (detected in {}ms)",
                    max_rpm_detected,
                    elapsed_ms
                );
            }
        }

        // 更新当前转速到全局变量
        CURRENT_FAN_RPM.sender().send(current_rpm);

        // 每秒输出一次转速日志（10个100ms周期）
        log_counter += 1;
        if log_counter >= 10 {
            log_counter = 0;
            if is_max_detection_phase {
                defmt::info!(
                    "🌀 Fan RPM: {} (Max detection phase: {}ms remaining)",
                    current_rpm,
                    FAN_MAX_DETECTION_TIME_MS - elapsed_ms
                );
            } else {
                defmt::info!("🌀 Fan RPM: {}", current_rpm);
            }
        }

        // 100ms 采样间隔
        Timer::after_millis(100).await;
    }
}

/// 获取检测到的最高风扇转速
///
/// # 返回
/// 最高转速值 (RPM)
pub async fn get_max_fan_rpm() -> u32 {
    *MAX_FAN_RPM.lock().await
}

/// 获取当前风扇转速
///
/// # 返回
/// 当前转速值 (RPM)
pub fn get_current_fan_rpm() -> u32 {
    CURRENT_FAN_RPM.try_get().unwrap_or(0)
}

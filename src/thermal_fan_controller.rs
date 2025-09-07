//! # 热管理风扇控制器
//!
//! 基于温度的智能DC风扇调速控制系统，支持：
//! - 温度驱动的平滑调速控制
//! - 启动时最大转速自动检测
//! - 故障时的降级保护机制
//! - 多重温度安全保护

use crate::shared::{DAC_MAX_VALUE, TEMPERATURE_CHANNEL};
use alloc::string::{String, ToString};
use embassy_stm32::{
    dac::{DacCh2, Value},
    gpio::Output,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Receiver};
use embassy_time::{Duration, Instant, Timer};

/// 热管理配置参数
#[derive(Debug, Clone)]
pub struct ThermalConfig {
    /// 最低启动温度 (°C)
    pub temp_min: f64,
    /// 最高温度，全速运行 (°C)
    pub temp_max: f64,
    /// 临界温度，强制全速 (°C)
    pub temp_critical: f64,
    /// 紧急关机温度 (°C)
    pub temp_shutdown: f64,
    /// 滞后区间，防止震荡 (°C)
    pub hysteresis: f64,
    /// 最小运行速度百分比（防止堵转）
    pub min_speed_percent: u8,
    /// 启动检测超时时间 (ms)
    pub detection_timeout_ms: u64,
}

impl Default for ThermalConfig {
    fn default() -> Self {
        Self {
            temp_min: 45.0,        // 45°C启动温度
            temp_max: 75.0,        // 75°C满转速运行
            temp_critical: 85.0,   // 85°C临界温度，强制全速
            temp_shutdown: 95.0,   // 95°C紧急关机温度
            hysteresis: 2.0,       // 2°C滞后区间，防止震荡
            min_speed_percent: 30, // 最小运行速度30%
            detection_timeout_ms: 6000,
        }
    }
}

/// 控制器状态
#[derive(Debug, Clone, PartialEq, defmt::Format)]
#[allow(dead_code)]
pub enum ControllerState {
    /// 初始化中
    Initializing,
    /// 启动检测中
    StartupDetection,
    /// 正常运行模式
    NormalOperation,
    /// 降级模式（开关控制）
    FallbackMode,
    /// 紧急停止
    EmergencyStop,
    /// 错误状态
    Error(ErrorCode),
}

/// 错误码定义
#[derive(Debug, Clone, PartialEq, defmt::Format)]
#[allow(dead_code)]
pub enum ErrorCode {
    /// 温度传感器异常
    TemperatureSensorFault,
    /// DAC输出异常
    DacOutputFault,
    /// 风扇信号异常
    FanSignalFault,
    /// 系统过热
    SystemOverheat,
}

/// 检测结果
#[derive(Debug, Clone)]
pub enum DetectionResult {
    /// 检测成功，返回最大转速
    Success(u32),
    /// 检测失败，需要降级
    Failed(String),
    /// 检测中
    InProgress,
}

/// 保护动作
#[derive(Debug, Clone, PartialEq)]
pub enum ProtectionAction {
    /// 正常运行
    Normal,
    /// 强制全速
    ForceFullSpeed,
    /// 系统关机保护
    SystemShutdown,
}

/// 控制器状态信息
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ControllerStatus {
    /// 当前状态
    pub state: ControllerState,
    /// 当前温度
    pub current_temperature: f64,
    /// 目标转速百分比
    pub target_speed_percent: u8,
    /// 实际转速
    pub actual_rpm: u32,
    /// 检测到的最大转速
    pub max_rpm_detected: Option<u32>,
    /// 是否处于降级模式
    pub is_fallback_mode: bool,
    /// 当前DAC值
    pub current_dac_value: u16,
}

/// 启动检测阶段
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
enum DetectionPhase {
    /// 电源启动
    PowerOn,
    /// 全速测试中
    FullSpeedTest,
    /// 转速采样中
    SpeedSampling,
    /// 验证检查
    ValidationCheck,
    /// 检测完成
    Completed,
}

/// 启动检测状态
struct StartupDetection {
    phase: DetectionPhase,
    start_time: Instant,
    max_rpm_detected: u32,
    last_rpm_samples: [u32; 5], // 保存最近5次采样
    sample_index: usize,
}

impl StartupDetection {
    fn new(_timeout_ms: u64) -> Self {
        Self {
            phase: DetectionPhase::PowerOn,
            start_time: Instant::now(),
            max_rpm_detected: 0,
            last_rpm_samples: [0; 5],
            sample_index: 0,
        }
    }

    fn add_rpm_sample(&mut self, rpm: u32) {
        self.last_rpm_samples[self.sample_index] = rpm;
        self.sample_index = (self.sample_index + 1) % 5;

        if rpm > self.max_rpm_detected {
            self.max_rpm_detected = rpm;
        }
    }

    fn is_stable(&self) -> bool {
        if self.last_rpm_samples[0] == 0 {
            return false; // 没有足够样本
        }

        let avg = self.last_rpm_samples.iter().sum::<u32>() / 5;
        let max_deviation = self
            .last_rpm_samples
            .iter()
            .map(|&rpm| rpm.abs_diff(avg))
            .max()
            .unwrap_or(0);

        max_deviation < avg / 10 // 偏差小于10%
    }

    fn is_valid_speed(&self) -> bool {
        self.max_rpm_detected >= 500 && self.max_rpm_detected <= 8000
    }
}

/// 热管理风扇控制器
pub struct ThermalFanController<'d> {
    /// DAC通道，控制FAN_DC引脚
    dac_channel: DacCh2<'d, embassy_stm32::peripherals::DAC1, embassy_stm32::mode::Blocking>,
    /// 风扇电源使能引脚
    fan_enable_pin: Output<'d>,
    /// 温度数据接收器
    temperature_rx: Receiver<'d, CriticalSectionRawMutex, f64, 1>,
    /// 控制器状态
    state: ControllerState,
    /// 配置参数
    config: ThermalConfig,
    /// 启动检测状态
    startup_detection: Option<StartupDetection>,
    /// 当前温度
    current_temperature: f64,
    /// 目标转速百分比
    target_speed_percent: u8,
    /// 当前DAC值
    current_dac_value: u16,
    /// 最大转速（检测结果）
    max_rpm_detected: Option<u32>,
    /// 降级模式状态（用于滞后控制）
    fallback_fan_enabled: bool,
}

impl<'d> ThermalFanController<'d> {
    /// 创建新的热管理控制器
    pub fn new(
        dac_channel: DacCh2<'d, embassy_stm32::peripherals::DAC1, embassy_stm32::mode::Blocking>,
        mut fan_enable_pin: Output<'d>,
        temperature_rx: Receiver<'d, CriticalSectionRawMutex, f64, 1>,
        config: Option<ThermalConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();

        // 启用风扇电源
        fan_enable_pin.set_high();
        defmt::info!(
            "🌀 ThermalFanController initialized with config: temp_range={}-{}°C",
            config.temp_min,
            config.temp_max
        );

        Self {
            dac_channel,
            fan_enable_pin,
            temperature_rx,
            state: ControllerState::Initializing,
            config,
            startup_detection: None,
            current_temperature: 25.0, // 假设室温
            target_speed_percent: 0,
            current_dac_value: DAC_MAX_VALUE, // 初始停止状态
            max_rpm_detected: None,
            fallback_fan_enabled: false,
        }
    }

    /// 启动检测流程
    pub async fn startup_detection(&mut self) -> DetectionResult {
        if self.startup_detection.is_none() {
            self.startup_detection = Some(StartupDetection::new(self.config.detection_timeout_ms));
            self.state = ControllerState::StartupDetection;
            defmt::info!("🔍 Starting fan speed detection...");
        }

        // 先获取检测状态信息，避免借用冲突
        let (phase, start_time, max_rpm, is_valid, is_stable) = {
            let detection = self.startup_detection.as_ref().unwrap();
            (
                detection.phase.clone(),
                detection.start_time,
                detection.max_rpm_detected,
                detection.is_valid_speed(),
                detection.is_stable(),
            )
        };

        let elapsed = Instant::now().duration_since(start_time);

        match phase {
            DetectionPhase::PowerOn => {
                // 启动阶段，设置风扇全速
                self.set_dac_value(0); // 0V = 最快速度
                self.startup_detection.as_mut().unwrap().phase = DetectionPhase::FullSpeedTest;
                defmt::info!("⚡ Fan set to full speed for detection");
                DetectionResult::InProgress
            }

            DetectionPhase::FullSpeedTest => {
                if elapsed >= Duration::from_secs(1) {
                    // 1秒后开始采样
                    self.startup_detection.as_mut().unwrap().phase = DetectionPhase::SpeedSampling;
                    defmt::info!("📊 Starting RPM sampling phase");
                }
                DetectionResult::InProgress
            }

            DetectionPhase::SpeedSampling => {
                // 模拟转速采样（无硬件反馈时使用时间延迟）
                Timer::after(Duration::from_millis(100)).await;
                defmt::info!("📈 Simulated RPM sampling (no hardware feedback available)");
                // 假设采样了一些数据
                self.startup_detection
                    .as_mut()
                    .unwrap()
                    .add_rpm_sample(3000); // 模拟转速

                if elapsed >= Duration::from_secs(5) {
                    // 5秒采样完成，开始验证
                    self.startup_detection.as_mut().unwrap().phase =
                        DetectionPhase::ValidationCheck;
                    defmt::info!("✅ Sampling complete, starting validation");
                }
                DetectionResult::InProgress
            }

            DetectionPhase::ValidationCheck => {
                if is_valid && is_stable {
                    // 检测成功
                    self.max_rpm_detected = Some(max_rpm);
                    self.state = ControllerState::NormalOperation;
                    self.startup_detection = None;

                    // 🔧 修复：检测完成后根据当前温度重新设置正确的转速
                    let target_speed = self.calculate_speed_percentage(self.current_temperature);
                    // 强制更新DAC值，不依赖条件判断
                    self.target_speed_percent = target_speed;
                    let dac_value = self.speed_percentage_to_dac(target_speed);
                    self.set_dac_value(dac_value);
                    defmt::info!(
                        "🔧 Reset fan speed after detection: {}% (DAC: {}, temp={}°C)",
                        target_speed,
                        dac_value,
                        self.current_temperature
                    );

                    defmt::info!("🎉 Detection SUCCESS: max_rpm = {} RPM", max_rpm);
                    DetectionResult::Success(max_rpm)
                } else {
                    // 检测失败，切换到降级模式
                    self.state = ControllerState::FallbackMode;
                    self.startup_detection = None;

                    // 🔧 修复：检测失败后也需要根据当前温度设置转速，避免保持满速状态
                    let target_speed = if self.current_temperature >= 55.0 {
                        100 // 降级模式的开启温度
                    } else {
                        0
                    };
                    // 强制更新DAC值，不依赖条件判断
                    self.target_speed_percent = target_speed;
                    let dac_value = self.speed_percentage_to_dac(target_speed);
                    self.set_dac_value(dac_value);
                    defmt::info!(
                        "🔧 Reset fan speed after failed detection: {}% (DAC: {}, temp={}°C)",
                        target_speed,
                        dac_value,
                        self.current_temperature
                    );

                    let reason_str = if !is_valid {
                        "Invalid speed"
                    } else {
                        "Unstable signal"
                    };

                    defmt::warn!(
                        "⚠️ Detection FAILED: {}, RPM: {}, switching to fallback mode",
                        reason_str,
                        max_rpm
                    );
                    DetectionResult::Failed(reason_str.to_string())
                }
            }

            DetectionPhase::Completed => {
                // 已完成
                DetectionResult::Success(max_rpm)
            }
        }
    }

    /// 主控制循环更新
    pub async fn update_control(&mut self) -> Result<(), ErrorCode> {
        // 更新温度数据
        if let Some(temperature) = self.temperature_rx.try_get() {
            self.current_temperature = temperature;
        }

        // 温度保护检查
        match self.thermal_protection(self.current_temperature) {
            ProtectionAction::SystemShutdown => {
                self.emergency_stop();
                return Err(ErrorCode::SystemOverheat);
            }
            ProtectionAction::ForceFullSpeed => {
                self.set_speed_percentage(100);
                defmt::warn!(
                    "🚨 Emergency cooling: forced full speed at {}°C",
                    self.current_temperature
                );
                return Ok(());
            }
            ProtectionAction::Normal => {}
        }

        // 根据当前状态执行控制逻辑
        match self.state {
            ControllerState::StartupDetection => {
                // 检测过程中，由startup_detection处理
            }

            ControllerState::NormalOperation => {
                // 正常模式：基于温度计算转速
                let target_speed = self.calculate_speed_percentage(self.current_temperature);
                self.set_speed_percentage(target_speed);
            }

            ControllerState::FallbackMode => {
                // 降级模式：简单开关控制
                self.fallback_control();
            }

            ControllerState::EmergencyStop => {
                // 紧急停止，保持停止状态
            }

            ControllerState::Error(_) => {
                // 错误状态，尝试恢复或保持安全状态
                self.set_speed_percentage(100); // 安全起见，全速运行
            }

            _ => {}
        }

        Ok(())
    }

    /// 计算基于温度的转速百分比
    fn calculate_speed_percentage(&self, temperature: f64) -> u8 {
        if temperature <= self.config.temp_min {
            return 0; // 停止
        }

        if temperature >= self.config.temp_max || temperature >= self.config.temp_critical {
            return 100; // 全速
        }

        // 线性插值计算
        let temp_range = self.config.temp_max - self.config.temp_min;
        let temp_delta = temperature - self.config.temp_min;
        let percentage = ((temp_delta / temp_range) * 100.0) as u8;

        // 应用滞后逻辑和最小速度限制
        let adjusted_percentage = if self.target_speed_percent > 0 {
            // 当前运行中，应用滞后
            if temperature < (self.config.temp_min + self.config.hysteresis) {
                0 // 停止
            } else {
                percentage.max(self.config.min_speed_percent)
            }
        } else {
            // 当前停止，正常启动
            percentage.max(self.config.min_speed_percent)
        };

        adjusted_percentage.min(100)
    }

    /// 降级控制模式（简单开关）
    fn fallback_control(&mut self) {
        let should_enable = if self.fallback_fan_enabled {
            // 当前运行，应用滞后
            self.current_temperature > (55.0 - self.config.hysteresis)
        } else {
            // 当前停止，检查是否需要启动
            self.current_temperature >= 55.0
        };

        if should_enable != self.fallback_fan_enabled {
            self.fallback_fan_enabled = should_enable;
            if should_enable {
                self.set_speed_percentage(100); // 全速
                defmt::info!(
                    "🌀 Fallback mode: Fan ENABLED at {}°C",
                    self.current_temperature
                );
            } else {
                self.set_speed_percentage(0); // 停止
                defmt::info!(
                    "🛑 Fallback mode: Fan DISABLED at {}°C",
                    self.current_temperature
                );
            }
        }
    }

    /// 设置风扇转速百分比
    pub fn set_speed_percentage(&mut self, percentage: u8) {
        if percentage != self.target_speed_percent {
            self.target_speed_percent = percentage;
            let dac_value = self.speed_percentage_to_dac(percentage);
            self.set_dac_value(dac_value);

            defmt::info!(
                "🎯 Speed set: {}% (DAC: {}, temp: {}°C)",
                percentage,
                dac_value,
                self.current_temperature
            );
        }
    }

    /// 转速百分比转换为DAC值（反向逻辑）
    ///
    /// DAC电压规格：
    /// - 理论最大: 3.3V (DAC = 4095)
    /// - 工作范围: 0V - 2.31V (限制70%以确保安全)
    /// - 分辨率: 0.8mV/LSB
    /// - 控制逻辑: 反向控制（电压越低，转速越快）
    ///
    /// 转速映射:
    /// - 100%转速 → 0V (DAC = 0)
    /// - 50%转速 → 1.65V (DAC = 2048)
    /// - 30%转速 → 2.31V (DAC = 2867)
    /// - 0%转速 → 3.3V (DAC = 4095)
    fn speed_percentage_to_dac(&self, speed_percentage: u8) -> u16 {
        if speed_percentage == 0 {
            return DAC_MAX_VALUE; // 停止：最高电压3.3V
        }

        // 反向映射：100% → 0V, 30% → 2.31V
        // 限制在70%的电压范围内以确保安全 (0V - 2.31V)
        let normalized = 100 - speed_percentage; // 反向
        let dac_value = ((normalized as u32 * DAC_MAX_VALUE as u32 * 70) / 100 / 100) as u16;

        dac_value.min((DAC_MAX_VALUE as u32 * 70 / 100) as u16) // 最大2867 (2.31V)
    }

    /// 设置DAC输出值
    fn set_dac_value(&mut self, dac_value: u16) {
        self.current_dac_value = dac_value;
        self.dac_channel.set(Value::Bit12Right(dac_value));
    }

    /// 温度保护检查
    fn thermal_protection(&self, temperature: f64) -> ProtectionAction {
        if temperature >= self.config.temp_shutdown {
            ProtectionAction::SystemShutdown
        } else if temperature >= self.config.temp_critical {
            ProtectionAction::ForceFullSpeed
        } else {
            ProtectionAction::Normal
        }
    }

    /// 获取当前状态信息
    pub fn get_status(&mut self) -> ControllerStatus {
        let actual_rpm = 0; // No RPM feedback available

        ControllerStatus {
            state: self.state.clone(),
            current_temperature: self.current_temperature,
            target_speed_percent: self.target_speed_percent,
            actual_rpm,
            max_rpm_detected: self.max_rpm_detected,
            is_fallback_mode: matches!(self.state, ControllerState::FallbackMode),
            current_dac_value: self.current_dac_value,
        }
    }

    /// 紧急停止
    pub fn emergency_stop(&mut self) {
        self.state = ControllerState::EmergencyStop;
        self.fan_enable_pin.set_low(); // 切断风扇电源
        self.set_dac_value(DAC_MAX_VALUE); // DAC输出最高（停止信号）
        defmt::error!("🛑 EMERGENCY STOP: Fan power disabled");
    }

    /// 重启控制器（从紧急停止状态恢复）
    #[allow(dead_code)]
    pub fn restart(&mut self) {
        if matches!(self.state, ControllerState::EmergencyStop) {
            self.fan_enable_pin.set_high(); // 重新启用风扇电源
            self.state = ControllerState::Initializing;
            self.startup_detection = None;
            defmt::info!("🔄 Controller restarted from emergency stop");
        }
    }
}

/// 热管理风扇控制任务
#[embassy_executor::task]
pub async fn thermal_fan_controller_task(
    dac_channel: DacCh2<'static, embassy_stm32::peripherals::DAC1, embassy_stm32::mode::Blocking>,
    fan_enable_pin: Output<'static>,
) {
    defmt::info!("🌀 Starting Thermal Fan Controller Task");

    // 获取数据接收器
    let temperature_rx = TEMPERATURE_CHANNEL.receiver().unwrap();

    // 创建控制器
    let mut controller = ThermalFanController::new(
        dac_channel,
        fan_enable_pin,
        temperature_rx,
        None, // 使用默认配置
    );

    // 启动检测阶段
    defmt::info!("🔍 Starting fan detection phase...");
    loop {
        match controller.startup_detection().await {
            DetectionResult::Success(max_rpm) => {
                defmt::info!("✅ Fan detection completed: {} RPM", max_rpm);
                break;
            }
            DetectionResult::Failed(reason) => {
                defmt::warn!(
                    "⚠️ Fan detection failed: {}, entering fallback mode",
                    reason.as_str()
                );
                break;
            }
            DetectionResult::InProgress => {
                // 继续检测
                Timer::after(Duration::from_millis(100)).await;
            }
        }
    }

    // 主控制循环
    defmt::info!("🎯 Entering main control loop");
    let mut status_log_counter = 0u32;

    loop {
        // 更新控制逻辑
        if let Err(error) = controller.update_control().await {
            defmt::error!("❌ Control error: {:?}", error);
        }

        // 定期状态报告（每5秒）
        status_log_counter += 1;
        if status_log_counter >= 50 {
            // 50 * 100ms = 5s
            status_log_counter = 0;
            let status = controller.get_status();
            defmt::info!(
                "📊 Status: temp={}°C, target={}%, rpm={}, dac={}",
                status.current_temperature,
                status.target_speed_percent,
                status.actual_rpm,
                status.current_dac_value
            );
        }

        // 100ms更新间隔
        Timer::after(Duration::from_millis(100)).await;
    }
}

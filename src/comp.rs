use embassy_time::Timer;

use crate::power_output::PowerOutput;
use crate::shared::ADC_PUBSUB;

/// 软件欠压保护配置
#[derive(Debug, Clone)]
pub struct UvpConfig {
    /// 欠压保护阈值（单位：V）
    pub threshold_voltage: f64,
    /// 是否启用自动恢复
    pub auto_recovery: bool,
    /// 恢复延迟时间（单位：ms）
    pub recovery_delay_ms: u32,
}

impl Default for UvpConfig {
    fn default() -> Self {
        Self {
            threshold_voltage: 5.0,
            auto_recovery: true,
            recovery_delay_ms: 1000,
        }
    }
}

/// 软件欠压保护检查
/// 基于ADC读取的电压值进行软件判断
pub fn check_undervoltage_software(voltage: f64, threshold: f64) -> bool {
    voltage < threshold
}

/// 软件欠压保护任务
/// 监控ADC电压并在检测到欠压时触发保护
#[embassy_executor::task]
pub async fn undervoltage_protection_task(
    mut power_output: PowerOutput<'static>,
    config: UvpConfig,
) {
    defmt::info!("启动软件欠压保护任务");
    defmt::info!("欠压阈值: {}V", config.threshold_voltage);
    defmt::info!("自动恢复: {}", config.auto_recovery);
    defmt::info!("恢复延迟: {}ms", config.recovery_delay_ms);

    let mut subscriber = ADC_PUBSUB.subscriber().unwrap();
    let mut protection_active = false;

    loop {
        // 等待ADC数据
        if let embassy_sync::pubsub::WaitResult::Message(adc_data) = subscriber.next_message().await
        {
            let (voltage, _current) = adc_data;

            // 检查欠压条件
            let is_undervoltage = check_undervoltage_software(voltage, config.threshold_voltage);

            if is_undervoltage && !protection_active {
                // 触发欠压保护
                defmt::warn!(
                    "🚨 检测到欠压: {}V < {}V",
                    voltage,
                    config.threshold_voltage
                );

                // 关闭输出
                power_output.set_off().await;
                protection_active = true;

                defmt::warn!("欠压保护已激活，输出已关闭");
            } else if !is_undervoltage && protection_active && config.auto_recovery {
                // 电压恢复正常，准备自动恢复
                defmt::info!(
                    "电压恢复正常: {}V >= {}V",
                    voltage,
                    config.threshold_voltage
                );
                defmt::info!("等待{}ms后自动恢复输出", config.recovery_delay_ms);

                // 等待恢复延迟
                Timer::after_millis(config.recovery_delay_ms as u64).await;

                // 重新检查电压（确保在延迟期间电压仍然正常）
                if let Some(embassy_sync::pubsub::WaitResult::Message(adc_data)) =
                    subscriber.try_next_message()
                {
                    let (current_voltage, _) = adc_data;
                    if current_voltage >= config.threshold_voltage {
                        // 恢复输出
                        power_output.set_on().await;
                        protection_active = false;

                        defmt::info!("✅ 欠压保护已恢复，输出已重新启用");
                    } else {
                        defmt::warn!(
                            "恢复期间电压仍然过低: {}V < {}V",
                            current_voltage,
                            config.threshold_voltage
                        );
                    }
                }
            }

            // 定期输出状态信息
            static mut COUNTER: u32 = 0;
            unsafe {
                COUNTER += 1;
                if COUNTER % 100 == 0 {
                    if protection_active {
                        defmt::warn!(
                            "🔒 欠压保护激活中 - 电压: {}V, 阈值: {}V",
                            voltage,
                            config.threshold_voltage
                        );
                    } else {
                        defmt::debug!(
                            "✅ 电压正常 - 当前: {}V, 阈值: {}V",
                            voltage,
                            config.threshold_voltage
                        );
                    }
                }
            }
        }

        // 短暂延迟避免过度占用CPU
        Timer::after_millis(10).await;
    }
}

/// 检查欠压保护功能（用于测试）
pub fn check_undervoltage_protection() -> bool {
    false
}

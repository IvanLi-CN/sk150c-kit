use embassy_time::{Duration, Instant};

/// 时间提供者抽象接口
/// 用于抽象时间相关操作，支持在测试中模拟时间流逝
pub trait TimeProvider: Send + Sync {
    /// 获取当前时间
    fn now(&self) -> Instant;

    /// 异步等待直到指定时间点
    async fn sleep_until(&self, deadline: Instant);

    /// 异步等待指定时长
    async fn sleep_for(&self, duration: Duration) {
        let deadline = self.now() + duration;
        self.sleep_until(deadline).await;
    }
}

/// 按键引脚抽象接口
/// 用于抽象按键硬件操作，支持在测试中模拟按键状态
pub trait ButtonPin: Send + Sync {
    /// 异步等待按键变为高电平（按下）
    async fn wait_for_high(&self);

    /// 异步等待按键变为低电平（释放）
    async fn wait_for_low(&self);

    /// 检查按键当前是否为高电平（是否按下）
    fn is_high(&self) -> bool;

    /// 检查按键当前是否为低电平（是否释放）
    fn is_low(&self) -> bool {
        !self.is_high()
    }
}

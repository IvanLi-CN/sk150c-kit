use alloc::sync::Arc;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant};

use super::traits::{ButtonPin, TimeProvider};
use alloc::collections::VecDeque;

/// Mock时间提供者，用于测试中精确控制时间流逝
#[derive(Clone)]
pub struct MockTimeProvider {
    current_time: Arc<Mutex<CriticalSectionRawMutex, Instant>>,
    // 用于通知等待中的定时器
    timer_signal: Arc<Signal<CriticalSectionRawMutex, ()>>,
    // 覆盖下一次 now() 的返回值，用于事件回放时刻对齐
    next_now_override: Arc<Mutex<CriticalSectionRawMutex, Option<Instant>>>,
}

impl MockTimeProvider {
    pub fn new() -> Self {
        Self {
            current_time: Arc::new(Mutex::new(Instant::from_millis(0))),
            timer_signal: Arc::new(Signal::new()),
            next_now_override: Arc::new(Mutex::new(None)),
        }
    }

    /// 推进时间（用于测试）
    pub async fn advance_time(&self, duration: Duration) {
        {
            let mut time = self.current_time.lock().await;
            *time += duration;
        }
        // 通知所有等待的定时器
        self.timer_signal.signal(());
    }

    // 原本提供 set_time/time_handle 等辅助接口，已移除未使用代码

    pub fn override_next_now(&self, ts: Instant) {
        if let Ok(mut ov) = self.next_now_override.try_lock() {
            *ov = Some(ts);
        }
    }
}

impl TimeProvider for MockTimeProvider {
    fn now(&self) -> Instant {
        // 优先返回一次性覆盖值（用于事件回放）
        if let Ok(mut ov) = self.next_now_override.try_lock()
            && let Some(ts) = ov.take()
        {
            return ts;
        }
        // 使用try_lock避免在同步上下文中阻塞
        match self.current_time.try_lock() {
            Ok(time) => *time,
            Err(_) => Instant::from_millis(0), // 默认值
        }
    }

    async fn sleep_until(&self, deadline: Instant) {
        loop {
            let current = {
                let time = self.current_time.lock().await;
                *time
            };

            if current >= deadline {
                break;
            }

            // 等待时间推进信号
            self.timer_signal.wait().await;
        }
    }
}

/// Mock按键引脚，用于测试中模拟按键状态
#[derive(Clone)]
pub struct MockButtonPin {
    state: Arc<Mutex<CriticalSectionRawMutex, bool>>, // true = high, false = low
    // 事件队列：(state, timestamp)
    events: Arc<Mutex<CriticalSectionRawMutex, VecDeque<(bool, Instant)>>>,
    // 用于通知等待状态变化的任务
    high_signal: Arc<Signal<CriticalSectionRawMutex, ()>>,
    low_signal: Arc<Signal<CriticalSectionRawMutex, ()>>,
    // 可选：与时间提供者共享的时钟，用于对齐下一次 now()
    time_provider: Option<Arc<MockTimeProvider>>,
}

impl MockButtonPin {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(false)), // 默认为低电平（未按下）
            events: Arc::new(Mutex::new(VecDeque::new())),
            high_signal: Arc::new(Signal::new()),
            low_signal: Arc::new(Signal::new()),
            time_provider: None,
        }
    }

    pub fn with_time(provider: Arc<MockTimeProvider>) -> Self {
        Self {
            time_provider: Some(provider),
            ..Self::new()
        }
    }

    /// 设置按键为高电平（按下）
    pub async fn set_high(&self) {
        let ts = self
            .time_provider
            .as_ref()
            .map(|p| p.now())
            .unwrap_or(Instant::from_millis(0));
        {
            let mut state = self.state.lock().await;
            *state = true;
        }
        {
            let mut q = self.events.lock().await;
            q.push_back((true, ts));
        }
        self.high_signal.signal(());
    }

    /// 设置按键为低电平（释放）
    pub async fn set_low(&self) {
        let ts = self
            .time_provider
            .as_ref()
            .map(|p| p.now())
            .unwrap_or(Instant::from_millis(0));
        {
            let mut state = self.state.lock().await;
            *state = false;
        }
        {
            let mut q = self.events.lock().await;
            q.push_back((false, ts));
        }
        self.low_signal.signal(());
    }

    // 原本提供 get_state 便捷方法，已移除未使用代码

    /// 便捷方法：检查是否为低电平（未按下）
    pub fn is_low(&self) -> bool {
        !self.is_high()
    }
}

impl ButtonPin for MockButtonPin {
    async fn wait_for_high(&self) {
        loop {
            // 优先消费事件队列，支持“先操作后poll”的测试顺序
            {
                let mut q = self.events.lock().await;
                if let Some(&(evt, ts)) = q.front()
                    && evt
                {
                    q.pop_front();
                    if let Some(tp) = &self.time_provider {
                        tp.override_next_now(ts);
                    }
                    return;
                }
            }

            // 其次检查即时状态（适配实时等待场景）
            {
                let state = self.state.lock().await;
                if *state {
                    return;
                }
            }

            // 等待高电平信号
            self.high_signal.wait().await;
        }
    }

    async fn wait_for_low(&self) {
        loop {
            // 优先消费事件队列
            {
                let mut q = self.events.lock().await;
                if let Some(&(evt, ts)) = q.front()
                    && !evt
                {
                    q.pop_front();
                    if let Some(tp) = &self.time_provider {
                        tp.override_next_now(ts);
                    }
                    return;
                }
            }

            // 其次检查即时状态
            {
                let state = self.state.lock().await;
                if !*state {
                    return;
                }
            }

            // 等待低电平信号
            self.low_signal.wait().await;
        }
    }

    fn is_high(&self) -> bool {
        match self.state.try_lock() {
            Ok(state) => *state,
            Err(_) => false, // 默认为低电平
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embassy_time::Duration;

    #[tokio::test]
    async fn test_mock_time_provider() {
        let provider = MockTimeProvider::new();
        let start_time = provider.now();

        provider.advance_time(Duration::from_millis(100)).await;
        let end_time = provider.now();

        assert_eq!(end_time - start_time, Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_mock_button_pin() {
        let pin = MockButtonPin::new();

        assert!(!pin.is_high());
        assert!(pin.is_low());

        pin.set_high().await;
        assert!(pin.is_high());
        assert!(!pin.is_low());

        pin.set_low().await;
        assert!(!pin.is_high());
        assert!(pin.is_low());
    }
}

use alloc::sync::Arc;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant};

use super::traits::{ButtonPin, TimeProvider};

/// Mock时间提供者，用于测试中精确控制时间流逝
#[derive(Clone)]
pub struct MockTimeProvider {
    current_time: Arc<Mutex<CriticalSectionRawMutex, Instant>>,
    // 用于通知等待中的定时器
    timer_signal: Arc<Signal<CriticalSectionRawMutex, ()>>,
}

impl MockTimeProvider {
    pub fn new() -> Self {
        Self {
            current_time: Arc::new(Mutex::new(Instant::from_millis(0))),
            timer_signal: Arc::new(Signal::new()),
        }
    }

    /// 推进时间（用于测试）
    pub async fn advance_time(&self, duration: Duration) {
        {
            let mut time = self.current_time.lock().await;
            *time = *time + duration;
        }
        // 通知所有等待的定时器
        self.timer_signal.signal(());
    }

    /// 设置绝对时间（用于测试）
    pub async fn set_time(&self, time: Instant) {
        {
            let mut current = self.current_time.lock().await;
            *current = time;
        }
        self.timer_signal.signal(());
    }
}

impl TimeProvider for MockTimeProvider {
    fn now(&self) -> Instant {
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
    // 用于通知等待状态变化的任务
    high_signal: Arc<Signal<CriticalSectionRawMutex, ()>>,
    low_signal: Arc<Signal<CriticalSectionRawMutex, ()>>,
}

impl MockButtonPin {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(false)), // 默认为低电平（未按下）
            high_signal: Arc::new(Signal::new()),
            low_signal: Arc::new(Signal::new()),
        }
    }

    /// 设置按键为高电平（按下）
    pub async fn set_high(&self) {
        {
            let mut state = self.state.lock().await;
            *state = true;
        }
        self.high_signal.signal(());
    }

    /// 设置按键为低电平（释放）
    pub async fn set_low(&self) {
        {
            let mut state = self.state.lock().await;
            *state = false;
        }
        self.low_signal.signal(());
    }

    /// 获取当前状态（用于测试验证）
    pub async fn get_state(&self) -> bool {
        *self.state.lock().await
    }
}

impl ButtonPin for MockButtonPin {
    async fn wait_for_high(&self) {
        loop {
            {
                let state = self.state.lock().await;
                if *state {
                    break;
                }
            }
            // 等待高电平信号
            self.high_signal.wait().await;
        }
    }

    async fn wait_for_low(&self) {
        loop {
            {
                let state = self.state.lock().await;
                if !*state {
                    break;
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

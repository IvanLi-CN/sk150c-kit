use alloc::sync::Arc;
use embassy_stm32::exti::ExtiInput;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Instant, Timer};

use super::traits::{ButtonPin, TimeProvider};

/// 真实硬件时间提供者
/// 使用embassy_time提供真实的时间操作
#[derive(Clone)]
pub struct RealTimeProvider;

impl RealTimeProvider {
    pub fn new() -> Self {
        Self
    }
}

impl TimeProvider for RealTimeProvider {
    fn now(&self) -> Instant {
        Instant::now()
    }

    async fn sleep_until(&self, deadline: Instant) {
        Timer::at(deadline).await;
    }
}

/// 真实硬件按键引脚
/// 包装ExtiInput提供抽象的按键接口
#[derive(Clone)]
pub struct RealButtonPin {
    pin: Arc<Mutex<CriticalSectionRawMutex, ExtiInput<'static>>>,
}

impl RealButtonPin {
    pub fn new(pin: ExtiInput<'static>) -> Self {
        Self {
            pin: Arc::new(Mutex::new(pin)),
        }
    }
}

impl ButtonPin for RealButtonPin {
    async fn wait_for_high(&self) {
        self.pin.lock().await.wait_for_high().await;
    }

    async fn wait_for_low(&self) {
        self.pin.lock().await.wait_for_low().await;
    }

    fn is_high(&self) -> bool {
        // 注意：这里需要使用try_lock来避免阻塞
        // 在实际使用中，这个方法通常在已知pin状态的情况下调用
        match self.pin.try_lock() {
            Ok(pin) => pin.is_high(),
            Err(_) => false, // 如果无法获取锁，假设为低电平
        }
    }
}

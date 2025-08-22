use alloc::sync::Arc;
use embassy_futures::select;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant};

use super::traits::{ButtonPin, TimeProvider};

#[derive(PartialEq, Clone, Copy, Debug, defmt::Format)]
pub enum ButtonState {
    Idle,
    WaitingRelease,
    LongPressed,
}

#[derive(Debug, PartialEq, Clone, Copy, defmt::Format)]
pub enum ButtonEvent {
    None,
    ShortPress,
    LongPressStart, // 新增：1000ms时立即触发
    LongPressEnd,   // 长按释放时触发
}

/// 重构后的按键内部逻辑，支持依赖注入
pub struct ButtonInternal<T: TimeProvider, P: ButtonPin> {
    time_provider: Arc<T>,
    pin: Arc<P>,
    debounce: Duration,
    long_press: Duration,
    state: Arc<Mutex<CriticalSectionRawMutex, ButtonState>>,
    press_start: Arc<Mutex<CriticalSectionRawMutex, Option<Instant>>>,
    long_press_triggered: Arc<Mutex<CriticalSectionRawMutex, bool>>, // 防止重复触发
}

impl<T: TimeProvider, P: ButtonPin> ButtonInternal<T, P> {
    pub fn new(
        time_provider: Arc<T>,
        pin: Arc<P>,
        debounce: Duration,
        long_press: Duration,
    ) -> Self {
        Self {
            time_provider,
            pin,
            debounce,
            long_press,
            state: Arc::new(Mutex::new(ButtonState::Idle)),
            press_start: Arc::new(Mutex::new(None)),
            long_press_triggered: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn poll(&self) -> ButtonEvent {
        loop {
            let current_state = {
                let state_mutex = self.state.lock().await;
                *state_mutex
            };

            match current_state {
                ButtonState::Idle => {
                    // 清除按键开始时间和长按触发标志
                    *self.press_start.lock().await = None;
                    *self.long_press_triggered.lock().await = false;
                    defmt::info!("Button waiting for press...");

                    // 等待按键按下
                    self.pin.wait_for_high().await;
                    defmt::info!("Button pressed! Recording start time...");

                    // 记录按键开始时间并进入等待释放状态
                    *self.press_start.lock().await = Some(self.time_provider.now());
                    *self.state.lock().await = ButtonState::WaitingRelease;
                }

                ButtonState::WaitingRelease => {
                    let start_time = {
                        let start_mutex = self.press_start.lock().await;
                        match *start_mutex {
                            Some(time) => time,
                            None => {
                                // 异常情况，重置状态
                                defmt::warn!(
                                    "Button start time is None in WaitingRelease state, resetting"
                                );
                                self.reset().await;
                                continue;
                            }
                        }
                    };

                    // 创建1000ms定时器
                    let long_press_deadline = start_time + self.long_press;

                    // 同时等待按键释放和长按定时器
                    match select::select(
                        self.pin.wait_for_low(),
                        self.time_provider.sleep_until(long_press_deadline),
                    )
                    .await
                    {
                        select::Either::First(_) => {
                            // 按键释放了，检查持续时间
                            let duration = self.time_provider.now() - start_time;
                            let duration_ms = duration.as_millis();

                            defmt::info!("Button released after {}ms", duration_ms);

                            if duration >= self.debounce && duration < self.long_press {
                                // 有效短按 (50ms-1000ms)
                                defmt::info!("Valid short press detected ({}ms)", duration_ms);
                                self.reset().await;
                                return ButtonEvent::ShortPress;
                            } else if duration < self.debounce {
                                // 抖动，忽略
                                defmt::info!(
                                    "Button bounce detected ({}ms), ignoring",
                                    duration_ms
                                );
                                self.reset().await;
                                return ButtonEvent::None;
                            } else {
                                // duration >= long_press，理论上不应该到这里，因为定时器会先触发
                                defmt::warn!(
                                    "Unexpected: button released after long press threshold"
                                );
                                self.reset().await;
                                return ButtonEvent::None;
                            }
                        }
                        select::Either::Second(_) => {
                            // 达到1000ms长按阈值 - 立即触发长按事件！
                            defmt::info!(
                                "Long press threshold reached (1000ms) - triggering immediately!"
                            );
                            *self.state.lock().await = ButtonState::LongPressed;
                            *self.long_press_triggered.lock().await = true;
                            return ButtonEvent::LongPressStart; // 立即返回长按开始事件
                        }
                    }
                }

                ButtonState::LongPressed => {
                    defmt::info!("Button in long press state, waiting for release...");

                    // 等待按键释放
                    self.pin.wait_for_low().await;

                    let start_time = {
                        let start_mutex = self.press_start.lock().await;
                        start_mutex.unwrap_or(self.time_provider.now())
                    };

                    let duration = self.time_provider.now() - start_time;
                    defmt::info!("Long press released after {}ms", duration.as_millis());

                    self.reset().await;
                    return ButtonEvent::LongPressEnd;
                }
            }
        }
    }

    async fn reset(&self) {
        *self.state.lock().await = ButtonState::Idle;
        *self.press_start.lock().await = None;
        *self.long_press_triggered.lock().await = false;
    }

    // 检查按键当前状态（用于调试）
    pub fn is_button_active(&self) -> bool {
        self.pin.is_high()
    }

    // 用于测试的辅助方法
    #[cfg(test)]
    pub async fn get_state(&self) -> ButtonState {
        *self.state.lock().await
    }

    #[cfg(test)]
    pub async fn is_long_press_triggered(&self) -> bool {
        *self.long_press_triggered.lock().await
    }
}

impl<T: TimeProvider, P: ButtonPin> Clone for ButtonInternal<T, P> {
    fn clone(&self) -> Self {
        Self {
            time_provider: Arc::clone(&self.time_provider),
            pin: Arc::clone(&self.pin),
            debounce: self.debounce,
            long_press: self.long_press,
            state: Arc::clone(&self.state),
            press_start: Arc::clone(&self.press_start),
            long_press_triggered: Arc::clone(&self.long_press_triggered),
        }
    }
}

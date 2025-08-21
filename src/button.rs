use alloc::sync::Arc;
use embassy_futures::select::{self, select3};
use embassy_stm32::exti::ExtiInput;
use embassy_sync::mutex::Mutex;
use embassy_sync::pubsub::{PubSubBehavior, PubSubChannel};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, pubsub::Subscriber};
use embassy_time::{Duration, Instant, Timer};

use crate::{INPUT_CAP, INPUT_PUB, INPUT_SUB};

// 简化的输入事件类型 - 只支持单按钮
#[derive(Debug, PartialEq, Clone, defmt::Format)]
pub enum InputEvent {
    /// 按钮短按
    Click,
    /// 按钮正在长按
    Holding,
    /// 按钮长按结束
    LongReleased,
}

// 简化的单按钮内部结构
#[derive(Clone)]
struct ButtonInternal {
    pin: Arc<Mutex<CriticalSectionRawMutex, ExtiInput<'static>>>,
    debounce: Duration,
    long_press: Duration,
    state: Arc<Mutex<CriticalSectionRawMutex, ButtonState>>,
    press_start: Arc<Mutex<CriticalSectionRawMutex, Option<Instant>>>,
}

#[derive(PartialEq, Clone, Copy, Debug, defmt::Format)]
enum ButtonState {
    BeforeIdle,
    Idle,
    Debouncing,
    Pressed,
    LongHeld,
}

impl ButtonInternal {
    fn new(pin: ExtiInput<'static>, debounce: Duration, long_press: Duration) -> Self {
        Self {
            pin: Arc::new(Mutex::new(pin)),
            debounce,
            long_press,
            state: Arc::new(Mutex::new(ButtonState::Idle)),
            press_start: Arc::new(Mutex::new(None)),
        }
    }

    // PB8按钮是高电平有效（Pull::Down配置）
    async fn wait_active(&self) {
        self.pin.lock().await.wait_for_high().await;
    }

    async fn wait_inactive(&self) {
        self.pin.lock().await.wait_for_low().await;
    }

    async fn is_active(&self) -> bool {
        self.pin.lock().await.is_high()
    }

    async fn poll(&self) -> ButtonEvent {
        loop {
            let prev_state_mutex = self.state.lock().await;
            let prev_state = *prev_state_mutex;
            drop(prev_state_mutex);

            match prev_state {
                ButtonState::BeforeIdle => {
                    *self.state.lock().await = ButtonState::Idle;
                    return ButtonEvent::None;
                }

                ButtonState::Idle => {
                    *self.press_start.lock().await = None;
                    defmt::info!("Button waiting for press...");
                    self.wait_active().await;
                    defmt::info!("Button pressed! Starting debounce...");
                    *self.state.lock().await = ButtonState::Debouncing;
                }

                ButtonState::Debouncing => {
                    *self.press_start.lock().await = Some(Instant::now());
                    Timer::after(self.debounce).await;

                    let is_active = self.is_active().await;

                    if is_active {
                        defmt::info!("Button press confirmed after debounce");
                        *self.state.lock().await = ButtonState::Pressed;
                    } else {
                        defmt::info!("Button press was noise, ignoring");
                        self.reset().await;
                    }
                }

                ButtonState::Pressed => {
                    let start = self.press_start.lock().await;
                    if start.is_none() {
                        self.reset().await;
                        return ButtonEvent::ShortPress;
                    }
                    let start = start.unwrap();
                    let long_time = start + self.long_press; // 1秒
                    let timeout_time = start + Duration::from_millis(3000); // 3秒超时

                    let wait_long = Timer::at(long_time);
                    let wait_timeout = Timer::at(timeout_time);

                    // 等待按钮释放、1秒长按阈值或3秒超时
                    match select3(self.wait_inactive(), wait_long, wait_timeout).await {
                        select::Either3::First(_) => {
                            // 按钮在1秒前释放 - 短按
                            if Instant::now() < long_time {
                                defmt::info!("Button short press detected (released before 1s)");
                                self.reset().await;
                                return ButtonEvent::ShortPress;
                            } else {
                                // 按钮在1-3秒之间释放 - 长按
                                defmt::info!("Button long press detected (released between 1-3s)");
                                self.reset().await;
                                return ButtonEvent::LongPressEnd;
                            }
                        }
                        select::Either3::Second(_) => {
                            // 达到1秒阈值，进入长按状态
                            defmt::info!("Button long press detected (1s threshold reached)");
                            *self.state.lock().await = ButtonState::LongHeld;
                            return ButtonEvent::LongPressStart;
                        }
                        select::Either3::Third(_) => {
                            // 超过3秒，忽略此次按压
                            defmt::info!("Button press timeout (>3s), ignoring");
                            self.reset().await;
                            return ButtonEvent::None;
                        }
                    }
                }

                ButtonState::LongHeld => {
                    let start = self.press_start.lock().await;
                    if start.is_none() {
                        self.reset().await;
                        return ButtonEvent::None;
                    }
                    let start = start.unwrap();
                    let timeout_time = start + Duration::from_millis(3000); // 3秒超时
                    let wait_timeout = Timer::at(timeout_time);

                    defmt::info!("Button in long hold state, waiting for release or timeout...");

                    // 等待按钮释放或3秒超时
                    match select::select(self.wait_inactive(), wait_timeout).await {
                        select::Either::First(_) => {
                            // 按钮在3秒前释放 - 正常长按结束
                            defmt::info!("Button released after long press (within 3s)");
                            self.reset().await;
                            return ButtonEvent::LongPressEnd;
                        }
                        select::Either::Second(_) => {
                            // 超过3秒，忽略此次按压
                            defmt::info!("Button long press timeout (>3s), ignoring");
                            self.reset().await;
                            return ButtonEvent::None;
                        }
                    }
                }
            }
        }
    }

    async fn reset(&self) {
        *self.state.lock().await = ButtonState::BeforeIdle;
    }
}

#[derive(Debug, PartialEq, Clone, Copy, defmt::Format)]
enum ButtonEvent {
    None,
    ShortPress,
    LongPressStart,
    LongPressEnd,
}

// 简化的单按钮输入管理器
#[derive(Clone)]
pub struct InputManager {
    button: ButtonInternal,
    channel:
        Arc<PubSubChannel<CriticalSectionRawMutex, InputEvent, INPUT_CAP, INPUT_SUB, INPUT_PUB>>,
}

impl InputManager {
    // 简化构造函数，只接受单个按钮（PB8）
    pub fn new(button_pin: ExtiInput<'static>, debounce: Duration, long_press: Duration) -> Self {
        let button = ButtonInternal::new(button_pin, debounce, long_press);

        Self {
            button,
            channel: Arc::new(PubSubChannel::new()),
        }
    }

    // Get a receiver for input events
    pub fn subscriber(
        &self,
    ) -> Result<
        Subscriber<'_, CriticalSectionRawMutex, InputEvent, INPUT_CAP, INPUT_SUB, INPUT_PUB>,
        embassy_sync::pubsub::Error,
    > {
        self.channel.subscriber()
    }

    // Main loop tick function
    pub async fn tick(&mut self) {
        let event = self.button.poll().await;
        self.handle_button_event(event).await;
    }

    // 简化的单按钮事件处理
    async fn handle_button_event(&mut self, event: ButtonEvent) {
        match event {
            ButtonEvent::ShortPress => {
                defmt::info!("Button short press detected");
                self.channel.publish_immediate(InputEvent::Click);
            }
            ButtonEvent::LongPressStart => {
                defmt::info!("Button long press started");
                self.channel.publish_immediate(InputEvent::Holding);
            }
            ButtonEvent::LongPressEnd => {
                defmt::info!("Button long press ended");
                self.channel.publish_immediate(InputEvent::LongReleased);
            }
            _ => (),
        }
    }

    // 检查按钮是否处于激活状态（用于调试）
    #[allow(dead_code)]
    pub async fn is_button_active(&self) -> bool {
        self.button.is_active().await
    }
}

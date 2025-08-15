use alloc::sync::Arc;
use core::array;
use embassy_futures::select;
use embassy_stm32::exti::ExtiInput;
use embassy_sync::mutex::Mutex;
use embassy_sync::pubsub::{PubSubBehavior, PubSubChannel};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, pubsub::Subscriber};
use embassy_time::{Duration, Instant, Timer};

use crate::{INPUT_CAP, INPUT_PUB, INPUT_SUB};

#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum ButtonId {
    Btn0, // 对应button0（高电平有效）
    Btn1,
    Btn2,
    Btn3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum ButtonCode {
    BtnLT,
    BtnRT,
    BtnRB,
    BtnLB,
}

impl ButtonId {
    pub fn map_to_code(self) -> ButtonCode {
        match self {
            ButtonId::Btn0 => ButtonCode::BtnLB,
            ButtonId::Btn1 => ButtonCode::BtnRB,
            ButtonId::Btn2 => ButtonCode::BtnLT,
            ButtonId::Btn3 => ButtonCode::BtnRT,
        }
    }
}

// 输入事件类型
#[derive(Debug, PartialEq, Clone, defmt::Format)]
pub enum InputEvent {
    /// 单个按钮短按 (按钮代码)
    SingleClick(ButtonCode),
    /// 单个按钮正在长按 (按钮代码)
    SingleHolding(ButtonCode),
    /// 单个按钮长按结束 (按钮代码)
    SingleLongReleased(ButtonCode),
    /// 双按钮长按结束 (按钮代码1, 按钮代码2)
    DualLongReleased(ButtonCode, ButtonCode),
}

#[derive(Clone)]
struct ButtonInternal {
    pin: Arc<Mutex<CriticalSectionRawMutex, ExtiInput<'static>>>,
    id: ButtonId,
    debounce: Duration,
    long_press: Duration,
    active_high: bool, // 新增电平极性标志
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
    ReleasedAfterLong,
}

impl ButtonInternal {
    fn new(
        id: ButtonId,
        pin: ExtiInput<'static>,
        debounce: Duration,
        long_press: Duration,
        active_high: bool,
    ) -> Self {
        Self {
            id,
            pin: Arc::new(Mutex::new(pin)),
            debounce,
            long_press,
            active_high,
            state: Arc::new(Mutex::new(ButtonState::Idle)),
            press_start: Arc::new(Mutex::new(None)),
        }
    }

    async fn wait_active(&self) {
        if self.active_high {
            self.pin.lock().await.wait_for_high().await;
        } else {
            self.pin.lock().await.wait_for_low().await;
        }
    }

    async fn wait_inactive(&self) {
        if self.active_high {
            self.pin.lock().await.wait_for_low().await;
        } else {
            self.pin.lock().await.wait_for_high().await;
        }
    }

    async fn is_active(&self) -> bool {
        if self.active_high {
            self.pin.lock().await.is_high()
        } else {
            self.pin.lock().await.is_low()
        }
    }

    async fn poll(&self) -> ButtonEvent {
        loop {
            let prev_state_mutex = self.state.lock().await;
            let prev_state = prev_state_mutex.clone();
            drop(prev_state_mutex);

            match prev_state {
                ButtonState::BeforeIdle => {
                    *self.state.lock().await = ButtonState::Idle;
                    return ButtonEvent::None;
                }

                ButtonState::Idle => {
                    *self.press_start.lock().await = None;
                    self.wait_active().await;
                    *self.state.lock().await = ButtonState::Debouncing;
                }

                ButtonState::Debouncing => {
                    *self.press_start.lock().await = Some(Instant::now());
                    Timer::after(self.debounce).await;

                    let is_active = self.is_active().await;

                    if is_active {
                        *self.state.lock().await = ButtonState::Pressed;
                    } else {
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
                    let long_time = start + self.long_press;

                    let wait_long = Timer::at(long_time);

                    select::select(self.wait_inactive(), wait_long).await;

                    if Instant::now() >= long_time {
                        *self.state.lock().await = ButtonState::LongHeld;
                        return ButtonEvent::LongPressStart;
                    } else {
                        self.reset().await;
                        return ButtonEvent::ShortPress;
                    }
                }

                ButtonState::LongHeld => {
                    self.wait_inactive().await;
                    *self.state.lock().await = ButtonState::ReleasedAfterLong;
                    return ButtonEvent::LongPressEnd;
                }

                ButtonState::ReleasedAfterLong => {
                    self.reset().await;
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

#[derive(Clone)]
pub struct InputManager {
    buttons: Arc<[ButtonInternal; 4]>,
    active_button_count: Arc<Mutex<CriticalSectionRawMutex, u8>>,
    channel:
        Arc<PubSubChannel<CriticalSectionRawMutex, InputEvent, INPUT_CAP, INPUT_SUB, INPUT_PUB>>,
    button_events: Arc<Mutex<CriticalSectionRawMutex, [Option<ButtonEvent>; 4]>>,
}

impl InputManager {
    pub fn new(buttons: [ExtiInput<'static>; 4], debounce: Duration, long_press: Duration) -> Self {
        let btn_ids = [
            ButtonId::Btn0, // 对应button0（高电平有效）
            ButtonId::Btn1,
            ButtonId::Btn2,
            ButtonId::Btn3,
        ];

        let mut buttons_iter = buttons.into_iter();
        let buttons = array::from_fn(|i| {
            // 设置Btn1为高电平有效，其他为低电平有效
            let active_high = matches!(btn_ids[i], ButtonId::Btn0);

            ButtonInternal::new(
                btn_ids[i],
                buttons_iter.next().expect("Exact size iterator"),
                debounce,
                long_press,
                active_high,
            )
        });

        Self {
            buttons: Arc::new(buttons),
            active_button_count: Arc::new(Mutex::new(0)),
            channel: Arc::new(PubSubChannel::new()),
            button_events: Arc::new(Mutex::new([None; 4])),
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
        use embassy_futures::select::{select4, Either4};

        let res = select4(
            self.buttons[0].poll(),
            self.buttons[1].poll(),
            self.buttons[2].poll(),
            self.buttons[3].poll(),
        )
        .await;

        match res {
            Either4::First(event) => self.btn_state_change(ButtonId::Btn0, event).await,
            Either4::Second(event) => self.btn_state_change(ButtonId::Btn1, event).await,
            Either4::Third(event) => self.btn_state_change(ButtonId::Btn2, event).await,
            Either4::Fourth(event) => self.btn_state_change(ButtonId::Btn3, event).await,
        };
    }

    async fn btn_state_change(&mut self, btn_id: ButtonId, event: ButtonEvent) {
        let mut button_events = self.button_events.lock().await;

        let mut active_button_count = self.active_button_count.lock().await;
        match event {
            ButtonEvent::ShortPress => {
                if *active_button_count == 0 {
                    let btn_code = self.map_button_id_to_code(btn_id);
                    self.channel
                        .publish_immediate(InputEvent::SingleClick(btn_code));
                }
            }
            ButtonEvent::LongPressStart => {
                if *active_button_count == 0 {
                    let btn_code = self.map_button_id_to_code(btn_id);
                    self.channel
                        .publish_immediate(InputEvent::SingleHolding(btn_code));
                }
                *active_button_count += 1;
            }
            ButtonEvent::LongPressEnd => match *active_button_count {
                1 => {
                    let btn_code = self.map_button_id_to_code(btn_id);
                    self.channel
                        .publish_immediate(InputEvent::SingleLongReleased(btn_code));

                    *active_button_count = 0;
                }
                2 => {
                    let btn_second = button_events
                        .iter()
                        .enumerate()
                        .find(|(idx, ele)| {
                            **ele == Some(ButtonEvent::LongPressStart) && *idx != btn_id as usize
                        })
                        .map(|(i, _)| i)
                        .unwrap();

                    let btn_code1 = self.map_button_id_to_code(btn_id);
                    let btn_code2 = self.map_button_id_to_code(self.buttons[btn_second].id);

                    self.channel
                        .publish_immediate(InputEvent::DualLongReleased(btn_code1, btn_code2));

                    *active_button_count = 0;
                }
                _ => {
                    *active_button_count = active_button_count.saturating_sub(1);
                }
            },
            _ => (),
        }

        button_events[btn_id as usize] = Some(event);
    }

    pub async fn is_reset_status(&self) -> bool {
        self.buttons[1].is_active().await && self.buttons[3].is_active().await
    }

    /// 将ButtonId映射为ButtonCode
    fn map_button_id_to_code(&self, btn_id: ButtonId) -> ButtonCode {
        btn_id.map_to_code()
    }
}

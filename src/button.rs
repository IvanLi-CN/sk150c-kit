// 重构后的按键控制模块 - 支持依赖注入和完整测试
mod button_internal;
mod real_impl;
mod traits;

#[cfg(test)]
mod mock_impl;
#[cfg(test)]
mod tests;

pub use button_internal::ButtonInternal;
pub use real_impl::{RealButtonPin, RealTimeProvider};
pub use traits::{ButtonPin, TimeProvider};

use alloc::sync::Arc;
use embassy_stm32::exti::ExtiInput;
use embassy_sync::pubsub::{PubSubBehavior, PubSubChannel};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, pubsub::Subscriber};
use embassy_time::Duration;

use crate::{INPUT_CAP, INPUT_PUB, INPUT_SUB};

// 简化的输入事件类型 - 只支持单按钮
#[derive(Debug, PartialEq, Clone, defmt::Format)]
pub enum InputEvent {
    /// 按钮短按 (50ms-1000ms)
    Click,
    /// 按钮长按结束 (>=1000ms后释放)
    LongReleased,
}

// 重新导出内部类型供外部使用
pub use button_internal::{ButtonEvent, ButtonState};

// 类型别名，使用真实硬件实现
type RealButtonInternal = ButtonInternal<RealTimeProvider, RealButtonPin>;

// 旧的ButtonInternal实现已移动到button_internal.rs模块

// 旧的poll和reset方法已移动到button_internal.rs模块

// 旧的ButtonEvent枚举已移动到button_internal.rs模块

// 简化的单按钮输入管理器
#[derive(Clone)]
pub struct InputManager {
    button: RealButtonInternal,
    channel:
        Arc<PubSubChannel<CriticalSectionRawMutex, InputEvent, INPUT_CAP, INPUT_SUB, INPUT_PUB>>,
}

impl InputManager {
    // 简化构造函数，只接受单个按钮（PB8）
    pub fn new(button_pin: ExtiInput<'static>, debounce: Duration, long_press: Duration) -> Self {
        let time_provider = Arc::new(RealTimeProvider::new());
        let pin = Arc::new(RealButtonPin::new(button_pin));
        let button = ButtonInternal::new(time_provider, pin, debounce, long_press);

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
                defmt::info!("Publishing short press event (Click)");
                self.channel.publish_immediate(InputEvent::Click);
            }
            ButtonEvent::LongPressStart => {
                // 长按开始事件 - 在1000ms时立即触发，立即执行长按动作
                defmt::info!("Long press started (1000ms reached) - triggering immediate action");
                self.channel.publish_immediate(InputEvent::LongReleased);
            }
            ButtonEvent::LongPressEnd => {
                // 长按结束事件 - 但不发布，因为动作已经在LongPressStart时执行了
                defmt::info!("Long press ended - no action needed (already handled at start)");
            }
            ButtonEvent::None => {
                // 无事件，不需要处理
            }
        }
    }

    // 检查按钮是否处于激活状态（用于调试）
    #[allow(dead_code)]
    pub fn is_button_active(&self) -> bool {
        self.button.is_button_active()
    }
}

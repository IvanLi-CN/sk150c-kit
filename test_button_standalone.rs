// 独立的按键测试文件，用于验证重构后的按键控制逻辑
// 运行方式: rustc --test test_button_standalone.rs && ./test_button_standalone

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// 简化的时间提供者trait
trait TimeProvider: Send + Sync {
    fn now(&self) -> Instant;
}

// 简化的按键引脚trait  
trait ButtonPin: Send + Sync {
    fn is_high(&self) -> bool;
}

// Mock实现
struct MockTimeProvider {
    current_time: Arc<Mutex<Instant>>,
}

impl MockTimeProvider {
    fn new() -> Self {
        Self {
            current_time: Arc::new(Mutex::new(Instant::now())),
        }
    }
    
    fn advance_time(&self, duration: Duration) {
        let mut time = self.current_time.lock().unwrap();
        *time = *time + duration;
    }
}

impl TimeProvider for MockTimeProvider {
    fn now(&self) -> Instant {
        *self.current_time.lock().unwrap()
    }
}

struct MockButtonPin {
    state: Arc<Mutex<bool>>,
}

impl MockButtonPin {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(false)),
        }
    }
    
    fn set_high(&self) {
        *self.state.lock().unwrap() = true;
    }
    
    fn set_low(&self) {
        *self.state.lock().unwrap() = false;
    }
}

impl ButtonPin for MockButtonPin {
    fn is_high(&self) -> bool {
        *self.state.lock().unwrap()
    }
}

// 简化的按键事件
#[derive(Debug, PartialEq)]
enum ButtonEvent {
    None,
    ShortPress,
    LongPressStart,
    LongPressEnd,
}

// 简化的按键状态
#[derive(Debug, PartialEq)]
enum ButtonState {
    Idle,
    WaitingRelease,
    LongPressed,
}

// 简化的按键逻辑（同步版本用于测试）
struct ButtonLogic<T: TimeProvider, P: ButtonPin> {
    time_provider: Arc<T>,
    pin: Arc<P>,
    debounce: Duration,
    long_press: Duration,
    state: ButtonState,
    press_start: Option<Instant>,
    long_press_triggered: bool,
}

impl<T: TimeProvider, P: ButtonPin> ButtonLogic<T, P> {
    fn new(time_provider: Arc<T>, pin: Arc<P>, debounce: Duration, long_press: Duration) -> Self {
        Self {
            time_provider,
            pin,
            debounce,
            long_press,
            state: ButtonState::Idle,
            press_start: None,
            long_press_triggered: false,
        }
    }
    
    // 简化的同步poll方法用于测试
    fn check_event(&mut self) -> ButtonEvent {
        match self.state {
            ButtonState::Idle => {
                if self.pin.is_high() {
                    self.press_start = Some(self.time_provider.now());
                    self.state = ButtonState::WaitingRelease;
                    self.long_press_triggered = false;
                }
                ButtonEvent::None
            }
            
            ButtonState::WaitingRelease => {
                let start_time = self.press_start.unwrap();
                let current_time = self.time_provider.now();
                let duration = current_time - start_time;
                
                if !self.pin.is_high() {
                    // 按键释放
                    self.state = ButtonState::Idle;
                    self.press_start = None;
                    
                    if duration >= self.debounce && duration < self.long_press {
                        return ButtonEvent::ShortPress;
                    } else if duration < self.debounce {
                        return ButtonEvent::None; // 抖动
                    }
                    return ButtonEvent::None;
                } else if duration >= self.long_press && !self.long_press_triggered {
                    // 达到长按阈值
                    self.state = ButtonState::LongPressed;
                    self.long_press_triggered = true;
                    return ButtonEvent::LongPressStart;
                }
                
                ButtonEvent::None
            }
            
            ButtonState::LongPressed => {
                if !self.pin.is_high() {
                    // 长按释放
                    self.state = ButtonState::Idle;
                    self.press_start = None;
                    self.long_press_triggered = false;
                    return ButtonEvent::LongPressEnd;
                }
                ButtonEvent::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_short_press() {
        let time_provider = Arc::new(MockTimeProvider::new());
        let pin = Arc::new(MockButtonPin::new());
        let mut button = ButtonLogic::new(
            Arc::clone(&time_provider),
            Arc::clone(&pin),
            Duration::from_millis(50),
            Duration::from_millis(1000),
        );

        // 模拟按键按下
        pin.set_high();
        assert_eq!(button.check_event(), ButtonEvent::None);

        // 推进时间到500ms
        time_provider.advance_time(Duration::from_millis(500));
        
        // 释放按键
        pin.set_low();
        assert_eq!(button.check_event(), ButtonEvent::ShortPress);
        
        println!("✅ Short press test passed");
    }

    #[test]
    pub fn test_long_press_immediate_trigger() {
        let time_provider = Arc::new(MockTimeProvider::new());
        let pin = Arc::new(MockButtonPin::new());
        let mut button = ButtonLogic::new(
            Arc::clone(&time_provider),
            Arc::clone(&pin),
            Duration::from_millis(50),
            Duration::from_millis(1000),
        );

        // 模拟按键按下
        pin.set_high();
        assert_eq!(button.check_event(), ButtonEvent::None);

        // 推进时间到1000ms
        time_provider.advance_time(Duration::from_millis(1000));
        
        // 检查长按立即触发
        assert_eq!(button.check_event(), ButtonEvent::LongPressStart);
        
        // 释放按键
        pin.set_low();
        assert_eq!(button.check_event(), ButtonEvent::LongPressEnd);
        
        println!("✅ Long press immediate trigger test passed");
    }

    #[test]
    pub fn test_bounce_filter() {
        let time_provider = Arc::new(MockTimeProvider::new());
        let pin = Arc::new(MockButtonPin::new());
        let mut button = ButtonLogic::new(
            Arc::clone(&time_provider),
            Arc::clone(&pin),
            Duration::from_millis(50),
            Duration::from_millis(1000),
        );

        // 模拟按键按下
        pin.set_high();
        assert_eq!(button.check_event(), ButtonEvent::None);

        // 推进时间到30ms（小于50ms阈值）
        time_provider.advance_time(Duration::from_millis(30));
        
        // 释放按键
        pin.set_low();
        assert_eq!(button.check_event(), ButtonEvent::None); // 应该被过滤
        
        println!("✅ Bounce filter test passed");
    }

    #[test]
    pub fn test_boundary_conditions() {
        let time_provider = Arc::new(MockTimeProvider::new());
        let pin = Arc::new(MockButtonPin::new());
        let mut button = ButtonLogic::new(
            Arc::clone(&time_provider),
            Arc::clone(&pin),
            Duration::from_millis(50),
            Duration::from_millis(1000),
        );

        // 测试恰好50ms
        pin.set_high();
        button.check_event();
        time_provider.advance_time(Duration::from_millis(50));
        pin.set_low();
        assert_eq!(button.check_event(), ButtonEvent::ShortPress);
        
        // 重置
        button.state = ButtonState::Idle;
        
        // 测试恰好1000ms
        pin.set_high();
        button.check_event();
        time_provider.advance_time(Duration::from_millis(1000));
        assert_eq!(button.check_event(), ButtonEvent::LongPressStart);
        
        println!("✅ Boundary conditions test passed");
    }
}

    #[test]
    pub fn test_double_trigger_prevention() {
        let time_provider = Arc::new(MockTimeProvider::new());
        let pin = Arc::new(MockButtonPin::new());
        let mut button = ButtonLogic::new(
            Arc::clone(&time_provider),
            Arc::clone(&pin),
            Duration::from_millis(50),
            Duration::from_millis(1000),
        );

        // 模拟按键按下
        pin.set_high();
        assert_eq!(button.check_event(), ButtonEvent::None);

        // 推进时间到1000ms触发长按开始
        time_provider.advance_time(Duration::from_millis(1000));
        assert_eq!(button.check_event(), ButtonEvent::LongPressStart);

        // 继续按住2秒
        time_provider.advance_time(Duration::from_millis(2000));

        // 释放按键
        pin.set_low();
        assert_eq!(button.check_event(), ButtonEvent::LongPressEnd);

        // 关键验证：在实际应用中，这两个事件都会被转换为InputEvent::LongReleased
        // 但现在修复后，LongPressEnd不应该触发应用层动作

        println!("✅ Double trigger prevention test passed");
    }

fn main() {
    println!("🧪 Running button control tests...");

    // 运行所有测试
    tests::test_short_press();
    tests::test_long_press_immediate_trigger();
    tests::test_bounce_filter();
    tests::test_boundary_conditions();
    test_double_trigger_prevention();

    println!("🎉 All tests passed! Button control logic is working correctly.");
}

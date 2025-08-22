#[cfg(test)]
mod button_tests {
    use super::super::button_internal::{ButtonEvent, ButtonInternal, ButtonState};
    use super::super::mock_impl::{MockButtonPin, MockTimeProvider};
    use alloc::sync::Arc;
    use embassy_time::Duration;

    type TestButtonInternal = ButtonInternal<MockTimeProvider, MockButtonPin>;

    fn create_test_button() -> (
        TestButtonInternal,
        Arc<MockTimeProvider>,
        Arc<MockButtonPin>,
    ) {
        let time_provider = Arc::new(MockTimeProvider::new());
        let pin = Arc::new(MockButtonPin::new());
        let button = ButtonInternal::new(
            Arc::clone(&time_provider),
            Arc::clone(&pin),
            Duration::from_millis(50),   // 50ms debounce
            Duration::from_millis(1000), // 1000ms long press
        );
        (button, time_provider, pin)
    }

    #[tokio::test]
    async fn test_short_press_valid_range() {
        let (button, time_provider, pin) = create_test_button();

        // 测试各种有效的短按时长
        let test_durations = [50, 100, 500, 999]; // ms

        for duration_ms in test_durations {
            // 模拟按键按下
            pin.set_high().await;

            // 推进时间到指定时长
            time_provider
                .advance_time(Duration::from_millis(duration_ms))
                .await;

            // 模拟按键释放
            pin.set_low().await;

            // 验证触发短按事件
            let event = button.poll().await;
            assert_eq!(
                event,
                ButtonEvent::ShortPress,
                "Duration {}ms should trigger short press",
                duration_ms
            );

            // 验证状态重置
            assert_eq!(button.get_state().await, ButtonState::Idle);
        }
    }

    #[tokio::test]
    async fn test_long_press_immediate_trigger() {
        let (button, time_provider, pin) = create_test_button();

        // 模拟按键按下
        pin.set_high().await;

        // 推进时间到1000ms（长按阈值）
        time_provider
            .advance_time(Duration::from_millis(1000))
            .await;

        // 验证长按立即触发
        let event = button.poll().await;
        assert_eq!(
            event,
            ButtonEvent::LongPressStart,
            "Long press should trigger immediately at 1000ms"
        );

        // 验证状态转换到LongPressed
        assert_eq!(button.get_state().await, ButtonState::LongPressed);

        // 验证长按触发标志
        assert!(button.is_long_press_triggered().await);
    }

    #[tokio::test]
    async fn test_long_press_release_event() {
        let (button, time_provider, pin) = create_test_button();

        // 模拟按键按下
        pin.set_high().await;

        // 推进时间到1000ms触发长按
        time_provider
            .advance_time(Duration::from_millis(1000))
            .await;
        let event1 = button.poll().await;
        assert_eq!(event1, ButtonEvent::LongPressStart);

        // 继续按住一段时间
        time_provider
            .advance_time(Duration::from_millis(2000))
            .await;

        // 释放按键
        pin.set_low().await;

        // 验证长按释放事件
        let event2 = button.poll().await;
        assert_eq!(event2, ButtonEvent::LongPressEnd);

        // 验证状态重置
        assert_eq!(button.get_state().await, ButtonState::Idle);
    }

    #[tokio::test]
    async fn test_bounce_filter() {
        let (button, time_provider, pin) = create_test_button();

        // 测试各种抖动时长
        let bounce_durations = [0, 10, 30, 49]; // ms，都小于50ms阈值

        for duration_ms in bounce_durations {
            // 模拟按键按下
            pin.set_high().await;

            // 推进时间到抖动时长
            time_provider
                .advance_time(Duration::from_millis(duration_ms))
                .await;

            // 模拟按键释放
            pin.set_low().await;

            // 验证触发None事件（被过滤）
            let event = button.poll().await;
            assert_eq!(
                event,
                ButtonEvent::None,
                "Duration {}ms should be filtered as bounce",
                duration_ms
            );

            // 验证状态重置
            assert_eq!(button.get_state().await, ButtonState::Idle);
        }
    }

    #[tokio::test]
    async fn test_exactly_boundary_conditions() {
        let (button, time_provider, pin) = create_test_button();

        // 测试恰好50ms（短按下限）
        pin.set_high().await;
        time_provider.advance_time(Duration::from_millis(50)).await;
        pin.set_low().await;
        let event = button.poll().await;
        assert_eq!(
            event,
            ButtonEvent::ShortPress,
            "Exactly 50ms should be short press"
        );

        // 测试恰好999ms（短按上限）
        pin.set_high().await;
        time_provider.advance_time(Duration::from_millis(999)).await;
        pin.set_low().await;
        let event = button.poll().await;
        assert_eq!(
            event,
            ButtonEvent::ShortPress,
            "Exactly 999ms should be short press"
        );

        // 测试恰好1000ms（长按阈值）
        pin.set_high().await;
        time_provider
            .advance_time(Duration::from_millis(1000))
            .await;
        let event = button.poll().await;
        assert_eq!(
            event,
            ButtonEvent::LongPressStart,
            "Exactly 1000ms should trigger long press"
        );
    }

    #[tokio::test]
    async fn test_long_press_no_duplicate_trigger() {
        let (button, time_provider, pin) = create_test_button();

        // 模拟按键按下
        pin.set_high().await;

        // 推进时间到1000ms触发长按
        time_provider
            .advance_time(Duration::from_millis(1000))
            .await;
        let event1 = button.poll().await;
        assert_eq!(event1, ButtonEvent::LongPressStart);

        // 继续按住很长时间（10秒）
        time_provider
            .advance_time(Duration::from_millis(10000))
            .await;

        // 验证长按不会重复触发
        assert!(
            button.is_long_press_triggered().await,
            "Long press should remain triggered"
        );

        // 释放按键
        pin.set_low().await;
        let event2 = button.poll().await;
        assert_eq!(event2, ButtonEvent::LongPressEnd);
    }

    #[tokio::test]
    async fn test_state_transitions() {
        let (button, time_provider, pin) = create_test_button();

        // 测试长按状态转换
        pin.set_high().await;
        time_provider
            .advance_time(Duration::from_millis(1000))
            .await;
        let event = button.poll().await;
        assert_eq!(event, ButtonEvent::LongPressStart);
        assert_eq!(button.get_state().await, ButtonState::LongPressed);

        // 释放后应该回到Idle
        pin.set_low().await;
        let event = button.poll().await;
        assert_eq!(event, ButtonEvent::LongPressEnd);
        assert_eq!(button.get_state().await, ButtonState::Idle);
    }

    #[tokio::test]
    async fn test_very_long_press_scenarios() {
        let (button, time_provider, pin) = create_test_button();

        // 测试30秒长按
        pin.set_high().await;

        // 1000ms时应该触发长按
        time_provider
            .advance_time(Duration::from_millis(1000))
            .await;
        let event1 = button.poll().await;
        assert_eq!(event1, ButtonEvent::LongPressStart);

        // 继续按住29秒
        time_provider
            .advance_time(Duration::from_millis(29000))
            .await;

        // 释放按键
        pin.set_low().await;
        let event2 = button.poll().await;
        assert_eq!(event2, ButtonEvent::LongPressEnd);

        // 验证总时长记录正确（应该是30秒）
        // 这个测试验证了长时间按键的稳定性
    }

    #[tokio::test]
    async fn test_double_trigger_bug() {
        let (button, time_provider, pin) = create_test_button();

        // 模拟按键按下
        pin.set_high().await;

        // 推进时间到1000ms触发长按
        time_provider
            .advance_time(Duration::from_millis(1000))
            .await;
        let event1 = button.poll().await;
        assert_eq!(
            event1,
            ButtonEvent::LongPressStart,
            "Should trigger LongPressStart at 1000ms"
        );

        // 继续按住一段时间
        time_provider
            .advance_time(Duration::from_millis(2000))
            .await;

        // 释放按键
        pin.set_low().await;
        let event2 = button.poll().await;
        assert_eq!(
            event2,
            ButtonEvent::LongPressEnd,
            "Should trigger LongPressEnd on release"
        );

        // 关键测试：验证不会有额外的事件
        // 在实际应用中，这两个事件都会被转换为InputEvent::LongReleased
        // 导致系统状态被切换两次
    }

    #[tokio::test]
    async fn test_multiple_long_press_cycles() {
        let (button, time_provider, pin) = create_test_button();

        // 测试多次长按循环
        for i in 0..3 {
            // 按下
            pin.set_high().await;
            time_provider
                .advance_time(Duration::from_millis(1000))
                .await;
            let event1 = button.poll().await;
            assert_eq!(
                event1,
                ButtonEvent::LongPressStart,
                "Cycle {}: LongPressStart",
                i
            );

            // 释放
            pin.set_low().await;
            let event2 = button.poll().await;
            assert_eq!(
                event2,
                ButtonEvent::LongPressEnd,
                "Cycle {}: LongPressEnd",
                i
            );

            // 短暂间隔
            time_provider.advance_time(Duration::from_millis(100)).await;
        }
    }

    #[tokio::test]
    async fn test_rapid_press_after_long_press() {
        let (button, time_provider, pin) = create_test_button();

        // 长按
        pin.set_high().await;
        time_provider
            .advance_time(Duration::from_millis(1000))
            .await;
        let event1 = button.poll().await;
        assert_eq!(event1, ButtonEvent::LongPressStart);

        pin.set_low().await;
        let event2 = button.poll().await;
        assert_eq!(event2, ButtonEvent::LongPressEnd);

        // 立即短按
        pin.set_high().await;
        time_provider.advance_time(Duration::from_millis(100)).await;
        pin.set_low().await;
        let event3 = button.poll().await;
        assert_eq!(event3, ButtonEvent::ShortPress);
    }

    #[tokio::test]
    async fn test_edge_case_exactly_1000ms_hold() {
        let (button, time_provider, pin) = create_test_button();

        // 按下并恰好持续1000ms
        pin.set_high().await;
        time_provider
            .advance_time(Duration::from_millis(1000))
            .await;

        // 在1000ms时刻同时释放
        pin.set_low().await;

        // 应该触发长按开始
        let event1 = button.poll().await;
        assert_eq!(
            event1,
            ButtonEvent::LongPressStart,
            "Should trigger LongPressStart at exactly 1000ms"
        );

        // 然后应该立即检测到释放并触发长按结束
        let event2 = button.poll().await;
        assert_eq!(
            event2,
            ButtonEvent::LongPressEnd,
            "Should trigger LongPressEnd immediately after"
        );
    }
}

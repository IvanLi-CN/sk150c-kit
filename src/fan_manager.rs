use crate::shared::{
    CURRENT_FAN_RPM, FAN_MAX_DETECTION_TIME_MS, FAN_PULSES_PER_REVOLUTION, FAN_TIMER_FREQ_HZ,
    MAX_FAN_RPM,
};
use defmt_rtt as _;
use embassy_stm32::{
    gpio::Output, gpio::Pull, peripherals::TIM3, time::Hertz, timer::pwm_input::PwmInput, Peri,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Receiver};
use embassy_time::{Instant, Timer};

/// Fan manager state
#[derive(Debug, Clone, Copy, PartialEq)]
enum FanManagerState {
    StartupTest,     // Startup test phase (first 5 seconds)
    NormalOperation, // Normal operation phase
}

/// Fan manager
///
/// Responsible for automatically controlling fan on/off based on temperature, implementing 5°C hysteresis control:
/// - First 5 seconds after startup: fan test run
/// - Temperature ≥ 50°C: start fan
/// - Temperature ≤ 45°C: stop fan
/// - 5°C hysteresis prevents frequent switching
pub struct FanManager<'d> {
    fan_pin: Output<'d>,
    temperature_rx: Receiver<'d, CriticalSectionRawMutex, f64, 1>,
    current_temperature: f64,
    fan_enabled: bool,
    tick_counter: u32,
    state: FanManagerState,
    startup_time: Instant,
}

impl<'d> FanManager<'d> {
    /// Fan startup temperature threshold (°C)
    const HIGH_TEMP_THRESHOLD: f64 = 50.0;

    /// Fan stop temperature threshold (°C)
    const LOW_TEMP_THRESHOLD: f64 = 45.0;

    /// Temperature anomaly detection threshold (°C) - exceeding this temperature may indicate sensor failure
    const TEMP_ANOMALY_THRESHOLD: f64 = 100.0;

    /// Create new fan manager
    ///
    /// # Parameters
    /// - `fan_pin`: Fan control GPIO pin (PB10)
    /// - `temperature_rx`: Temperature data receiver
    pub fn new(
        mut fan_pin: Output<'d>,
        temperature_rx: Receiver<'d, CriticalSectionRawMutex, f64, 1>,
    ) -> Self {
        defmt::info!("🌀 Fan Manager initialized");
        defmt::info!("   High temp threshold: {}°C", Self::HIGH_TEMP_THRESHOLD);
        defmt::info!("   Low temp threshold: {}°C", Self::LOW_TEMP_THRESHOLD);
        defmt::info!("   Starting 5-second fan test...");

        // Startup test: immediately start fan
        fan_pin.set_high();

        Self {
            fan_pin,
            temperature_rx,
            current_temperature: 25.0, // Assume initial room temperature
            fan_enabled: true,         // Fan enabled during startup test
            tick_counter: 0,
            state: FanManagerState::StartupTest,
            startup_time: Instant::now(),
        }
    }

    /// Execute one fan management check
    ///
    /// Should be called every 5 seconds, synchronized with ADC sampling frequency
    pub async fn tick(&mut self) {
        self.tick_counter += 1;

        match self.state {
            FanManagerState::StartupTest => {
                // Startup test phase: check if 5 seconds have elapsed
                let elapsed = Instant::now().duration_since(self.startup_time);
                if elapsed.as_secs() >= 5 {
                    // 5-second test completed, switch to normal operation mode
                    defmt::info!(
                        "🌀 Fan test completed after {} seconds, switching to normal operation",
                        elapsed.as_secs()
                    );
                    self.state = FanManagerState::NormalOperation;
                    self.fan_pin.set_low(); // Turn off fan
                    self.fan_enabled = false;
                    defmt::info!("🛑 Fan DISABLED after startup test");
                } else {
                    // Test still in progress
                    defmt::info!("🌀 Fan test running... elapsed: {}s", elapsed.as_secs());
                }
            }
            FanManagerState::NormalOperation => {
                // Normal operation phase: control fan based on temperature
                if let Some(temperature) = self.temperature_rx.try_get() {
                    self.current_temperature = temperature;

                    // Check for temperature anomaly
                    if temperature > Self::TEMP_ANOMALY_THRESHOLD {
                        defmt::warn!(
                            "⚠️ Temperature anomaly detected: {}°C (>{}°C)",
                            temperature,
                            Self::TEMP_ANOMALY_THRESHOLD
                        );
                        // Keep current fan state unchanged when temperature is abnormal
                        return;
                    }

                    // Update fan state
                    self.update_fan_state(temperature).await;
                }

                // Periodic status report (once per minute, i.e., 12 five-second cycles)
                if self.tick_counter % 12 == 0 {
                    defmt::info!(
                        "🌡️ Temperature: {}°C, Fan: {}",
                        self.current_temperature,
                        if self.fan_enabled { "ON" } else { "OFF" }
                    );
                }
            }
        }
    }

    /// Update fan state based on temperature
    ///
    /// Implement 5°C hysteresis control logic
    async fn update_fan_state(&mut self, temperature: f64) {
        let should_enable = if self.fan_enabled {
            // Fan currently on, only turn off when temperature drops below 45°C
            temperature > Self::LOW_TEMP_THRESHOLD
        } else {
            // Fan currently off, only turn on when temperature reaches 50°C or above
            temperature >= Self::HIGH_TEMP_THRESHOLD
        };

        // Only update hardware and logs when state changes
        if should_enable != self.fan_enabled {
            self.fan_enabled = should_enable;

            if should_enable {
                self.fan_pin.set_high();
                defmt::info!(
                    "🌀 Fan ENABLED at {}°C (threshold: {}°C)",
                    temperature,
                    Self::HIGH_TEMP_THRESHOLD
                );
            } else {
                self.fan_pin.set_low();
                defmt::info!(
                    "🛑 Fan DISABLED at {}°C (threshold: {}°C)",
                    temperature,
                    Self::LOW_TEMP_THRESHOLD
                );
            }
        }
    }
}

/// Calculate fan speed (RPM)
///
/// # Parameters
/// - `period_ticks`: PWM input measured period count
///
/// # Returns
/// Speed value (RPM), returns 0 if no signal
fn calculate_rpm(period_ticks: u32) -> u32 {
    if period_ticks == 0 {
        return 0;
    }

    // Calculate signal frequency (Hz)
    let signal_freq = FAN_TIMER_FREQ_HZ / period_ticks;

    // Convert to RPM: frequency * 60 / pulses per revolution
    let rpm = (signal_freq * 60) / FAN_PULSES_PER_REVOLUTION;

    // Sanity check: fan speed is usually in 0-10000 RPM range
    if rpm > 10000 {
        defmt::warn!("⚠️ Abnormal fan speed detected: {} RPM, ignoring", rpm);
        return 0;
    }

    rpm
}

/// Fan speed sampling task
///
/// This task is responsible for:
/// 1. Initialize PWM input functionality
/// 2. Perform maximum speed detection for the first 5 seconds
/// 3. Continuously sample and output speed data
pub async fn fan_speed_sampling_task(
    tim3: Peri<'static, TIM3>,
    fan_touch_pin: Peri<
        'static,
        impl embassy_stm32::timer::TimerPin<TIM3, embassy_stm32::timer::Ch1>,
    >,
) {
    defmt::info!("🌀 Starting fan speed sampling task");

    // Create PWM input instance
    let mut pwm_input =
        PwmInput::new_ch1(tim3, fan_touch_pin, Pull::Up, Hertz::hz(FAN_TIMER_FREQ_HZ));

    // Enable PWM input
    pwm_input.enable();
    defmt::info!("🌀 PWM input enabled for fan speed measurement");

    let start_time = Instant::now();
    let mut max_rpm_detected = 0u32;
    let mut sample_count = 0u32;
    let mut log_counter = 0u32;

    loop {
        // Get period count and calculate speed
        let period_ticks = pwm_input.get_period_ticks();
        let current_rpm = calculate_rpm(period_ticks);

        sample_count += 1;

        // Check if in maximum speed detection period (first 5 seconds)
        let elapsed_ms = Instant::now().duration_since(start_time).as_millis();
        let is_max_detection_phase = elapsed_ms < FAN_MAX_DETECTION_TIME_MS;

        if is_max_detection_phase {
            // Maximum speed detection phase
            if current_rpm > max_rpm_detected {
                max_rpm_detected = current_rpm;
                defmt::info!("🌀 New max RPM detected: {} RPM", max_rpm_detected);
            }
        } else if sample_count > 0 && elapsed_ms >= FAN_MAX_DETECTION_TIME_MS {
            // Detection phase just ended, save maximum speed (execute only once)
            static mut MAX_RPM_SAVED: bool = false;
            if unsafe { !MAX_RPM_SAVED } {
                unsafe {
                    MAX_RPM_SAVED = true;
                }
                // Save maximum speed to global variable
                *MAX_FAN_RPM.lock().await = max_rpm_detected;
                defmt::info!(
                    "🌀 Max RPM detection completed: {} RPM (detected in {}ms)",
                    max_rpm_detected,
                    elapsed_ms
                );
            }
        }

        // Update current speed to global variable
        CURRENT_FAN_RPM.sender().send(current_rpm);

        // Output speed log once per second (10 cycles of 100ms)
        log_counter += 1;
        if log_counter >= 10 {
            log_counter = 0;
            if is_max_detection_phase {
                defmt::info!(
                    "🌀 Fan RPM: {} (Max detection phase: {}ms remaining)",
                    current_rpm,
                    FAN_MAX_DETECTION_TIME_MS - elapsed_ms
                );
            } else {
                defmt::info!("🌀 Fan RPM: {}", current_rpm);
            }
        }

        // 100ms sampling interval
        Timer::after_millis(100).await;
    }
}

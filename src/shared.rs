use crate::{
    config_manager::{Config, ConfigRequest},
    power,
};
use alloc::sync::Arc;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, mutex::Mutex,
    pubsub::PubSubChannel, watch::Watch,
};

#[allow(dead_code)]
pub const VALUE_STEP_MILLIVOLTS: u32 = 100;
pub const VREF: f64 = 3.0;

pub const VSN_MUL: f64 = (130_000.0 + 10_000.0) / 10_000.0;
#[allow(dead_code)]
pub const ISN_MUL: f64 = 1.0 / 0.010 / 25.0;

// ADC and power constants

pub(crate) static ADC_PUBSUB: PubSubChannel<CriticalSectionRawMutex, (f64, f64), 2, 1, 1> =
    PubSubChannel::new();

#[allow(dead_code)]
pub(crate) static CONFIG_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, ConfigRequest, 1> =
    Channel::new();

pub(crate) static CONFIG_SNAPSHOT_CHANNEL: Watch<CriticalSectionRawMutex, Config, 1> = Watch::new();

pub(crate) static SINK_REQUEST_CHANNEL: Watch<CriticalSectionRawMutex, power::DeviceRequest, 1> =
    Watch::new();

pub(crate) static PD_ERROR_CHANNEL: Channel<
    CriticalSectionRawMutex,
    Arc<usbpd::sink::policy_engine::Error>,
    1,
> = Channel::new();

// VBUS voltage status channel
pub(crate) static VBUS_VOLTAGE_CHANNEL: Watch<CriticalSectionRawMutex, f64, 1> = Watch::new();

// VIN voltage status channel
pub(crate) static VIN_VOLTAGE_CHANNEL: Watch<CriticalSectionRawMutex, f64, 1> = Watch::new();

// VBUS switch status channel
pub(crate) static VBUS_STATE_CHANNEL: Watch<CriticalSectionRawMutex, bool, 1> = Watch::new();

// VBUS reset signal channel
pub(crate) static VBUS_RESET_CHANNEL: Watch<CriticalSectionRawMutex, bool, 1> = Watch::new();

// Temperature data channel
pub(crate) static TEMPERATURE_CHANNEL: Watch<CriticalSectionRawMutex, f64, 1> = Watch::new();

// Fan speed related constants
pub const FAN_TIMER_FREQ_HZ: u32 = 1_000_000; // 1MHz timer frequency
pub const FAN_PULSES_PER_REVOLUTION: u32 = 2; // Fan pulses per revolution
pub const FAN_MAX_DETECTION_TIME_MS: u64 = 5000; // Max speed detection time (milliseconds)

// Fan speed data storage
pub(crate) static MAX_FAN_RPM: Mutex<CriticalSectionRawMutex, u32> = Mutex::new(0);
pub(crate) static CURRENT_FAN_RPM: Watch<CriticalSectionRawMutex, u32, 1> = Watch::new();

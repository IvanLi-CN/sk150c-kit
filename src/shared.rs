use crate::power;
use alloc::sync::Arc;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, pubsub::PubSubChannel,
    watch::Watch,
};

pub const VREF: f64 = 3.0;

pub const VSN_MUL: f64 = (130_000.0 + 10_000.0) / 10_000.0;

// ADC and power constants

pub(crate) static ADC_PUBSUB: PubSubChannel<CriticalSectionRawMutex, (f64, f64), 2, 1, 1> =
    PubSubChannel::new();

// Config channels removed as configuration manager is no longer used

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

// Fan DC speed control constants
pub const DAC_MAX_VALUE: u16 = 4095; // 12位DAC最大值 (2^12 - 1)

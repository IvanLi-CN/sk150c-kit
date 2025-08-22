use crate::{
    config_manager::{Config, ConfigRequest},
    power,
};
use alloc::sync::Arc;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, pubsub::PubSubChannel,
    watch::Watch,
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

// VBUS 电压状态通道
pub(crate) static VBUS_VOLTAGE_CHANNEL: Watch<CriticalSectionRawMutex, f64, 1> = Watch::new();

// VIN 电压状态通道
pub(crate) static VIN_VOLTAGE_CHANNEL: Watch<CriticalSectionRawMutex, f64, 1> = Watch::new();

// VBUS 开关状态通道
pub(crate) static VBUS_STATE_CHANNEL: Watch<CriticalSectionRawMutex, bool, 1> = Watch::new();

// VBUS 重置信号通道
pub(crate) static VBUS_RESET_CHANNEL: Watch<CriticalSectionRawMutex, bool, 1> = Watch::new();

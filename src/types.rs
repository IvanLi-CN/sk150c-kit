use embassy_stm32::i2c::{I2c, Master};
use embassy_stm32::mode;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::pubsub;

use crate::button::InputEvent;

#[derive(Debug, Clone, Copy, defmt::Format)]
pub struct PowerInfo {
    pub amps: f64,
    pub volts: f64,
    pub watts: f64,
}

impl Default for PowerInfo {
    fn default() -> Self {
        Self {
            amps: 0.0,
            volts: 0.0,
            watts: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, defmt::Format)]
pub struct StatusInfo {
    pub target_volts: f64,
    pub limit_amps: f64,
    pub output: bool,
}

impl Default for StatusInfo {
    fn default() -> Self {
        Self {
            target_volts: 0.0,
            limit_amps: 0.0,
            output: false,
        }
    }
}

pub(crate) type I2cBus = I2c<'static, mode::Async, Master>;
pub(crate) type SharedI2cBus = Mutex<CriticalSectionRawMutex, I2cBus>;

pub(crate) const INPUT_CAP: usize = 2;
pub(crate) const INPUT_PUB: usize = 1;
pub(crate) const INPUT_SUB: usize = 2;

pub(crate) type InputSubscriber<'d> =
    pubsub::Subscriber<'d, CriticalSectionRawMutex, InputEvent, INPUT_CAP, INPUT_SUB, INPUT_PUB>;

#[derive(Clone, Copy, Debug, defmt::Format)]
#[allow(dead_code)]
pub(crate) struct AvailableVoltCurr {
    pub _5v: Option<u32>,
    pub _9v: Option<u32>,
    pub _12v: Option<u32>,
    pub _15v: Option<u32>,
    pub _18v: Option<u32>,
    pub _20v: Option<u32>,
}

impl AvailableVoltCurr {
    #[allow(dead_code)]
    pub const fn default() -> Self {
        Self {
            _5v: None,
            _9v: None,
            _12v: None,
            _15v: None,
            _18v: None,
            _20v: None,
        }
    }
}

#[cfg(not(test))]
use embassy_stm32::i2c::{I2c, Master};
#[cfg(not(test))]
use embassy_stm32::mode;
#[cfg(not(test))]
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
#[cfg(not(test))]
use embassy_sync::mutex::Mutex;
#[cfg(not(test))]
use embassy_sync::pubsub;

#[cfg(not(test))]
use crate::button::InputEvent;

#[cfg(not(test))]
pub(crate) type I2cBus = I2c<'static, mode::Async, Master>;
#[cfg(not(test))]
pub(crate) type SharedI2cBus = Mutex<CriticalSectionRawMutex, I2cBus>;

#[cfg(not(test))]
pub(crate) const INPUT_CAP: usize = 2;
#[cfg(not(test))]
pub(crate) const INPUT_PUB: usize = 1;
#[cfg(not(test))]
pub(crate) const INPUT_SUB: usize = 2;

#[cfg(not(test))]
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

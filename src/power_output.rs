use alloc::sync::Arc;
use core::sync::atomic::AtomicBool;
use embassy_stm32::gpio::{Level, Output};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};

const OFF_LEVEL: Level = Level::Low;
const ON_LEVEL: Level = Level::High;

#[derive(Clone)]
pub struct PowerOutput<'d> {
    pin: Arc<Mutex<CriticalSectionRawMutex, Output<'d>>>,
    state: Arc<AtomicBool>,
}

impl<'d> PowerOutput<'d> {
    pub fn new(pin: Output<'d>) -> Self {
        Self {
            pin: Arc::new(Mutex::new(pin)),
            state: Arc::new(AtomicBool::new(false)),
        }
    }

    #[inline(always)]
    pub async fn set_state(&self, state: bool) {
        self.state
            .store(state, core::sync::atomic::Ordering::SeqCst);
        self.pin
            .lock()
            .await
            .set_level(if state { ON_LEVEL } else { OFF_LEVEL });
    }

    #[inline(always)]
    pub async fn set_on(&self) {
        self.set_state(true).await
    }

    #[inline(always)]
    pub async fn set_off(&self) {
        self.set_state(false).await
    }
}

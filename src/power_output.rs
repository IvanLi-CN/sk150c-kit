#![allow(dead_code)]

use alloc::sync::Arc;
use core::sync::atomic::AtomicBool;
use embassy_stm32::gpio::{Level, Output};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::Timer;

const OFF_LEVEL: Level = Level::Low;
const ON_LEVEL: Level = Level::High;

#[derive(Clone)]
pub struct PowerOutput<'d> {
    pin: Arc<Mutex<CriticalSectionRawMutex, Output<'d>>>,
    state: Arc<AtomicBool>,
    prev_state: Arc<AtomicBool>,
}

impl<'d> PowerOutput<'d> {
    pub fn new(pin: Output<'d>) -> Self {
        Self {
            pin: Arc::new(Mutex::new(pin)),
            state: Arc::new(AtomicBool::new(false)),
            prev_state: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn wait_change(&self) -> bool {
        let old = self.prev_state.load(core::sync::atomic::Ordering::SeqCst);
        while self.state.load(core::sync::atomic::Ordering::SeqCst) == old {
            Timer::after_millis(10).await;
        }
        self.prev_state
            .store(!old, core::sync::atomic::Ordering::SeqCst);
        !old
    }

    pub async fn get_state(&self) -> bool {
        let state = self.pin.lock().await.get_output_level() == ON_LEVEL;

        if state != self.state.load(core::sync::atomic::Ordering::SeqCst) {
            self.state
                .store(state, core::sync::atomic::Ordering::SeqCst);
        }

        state
    }

    pub async fn toggle(&self) {
        if self.state.load(core::sync::atomic::Ordering::SeqCst) {
            defmt::info!("output off");
            self.set_off().await;
        } else {
            defmt::info!("output on");
            self.set_on().await;
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

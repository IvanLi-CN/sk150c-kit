use alloc::sync::Arc;
use core::marker::PhantomData;
use defmt::{info, warn, Format};
use embassy_futures::select::{select, Either};
use embassy_stm32::{
    interrupt,
    ucpd::{
        self, Cc1Pin, Cc2Pin, CcPhy, CcPull, CcSel, CcVState, Config, Instance, InterruptHandler,
        PdPhy, RxDma, TxDma, Ucpd,
    },
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel, mutex::Mutex, signal::Signal, watch,
};
use embassy_time::{with_timeout, Duration, Timer};

use usbpd::{
    protocol_layer::message::{
        pdo::SourceCapabilities,
        request::{CurrentRequest, PowerSource, VoltageRequest},
    },
    sink::{self, device_policy_manager::DevicePolicyManager},
    timers::Timer as SinkTimer,
};
use usbpd::{sink::policy_engine::Sink, Driver as SinkDriver};

#[derive(Debug, Format)]
enum CableOrientation {
    Normal,
    Flipped,
    DebugAccessoryMode,
}

struct UcpdSinkDriver<'d, T: Instance> {
    /// The UCPD PD phy instance.
    pd_phy: PdPhy<'d, T>,
}

impl<'d, T: Instance> UcpdSinkDriver<'d, T> {
    fn new(pd_phy: PdPhy<'d, T>) -> Self {
        Self { pd_phy }
    }
}

impl<T: Instance> SinkDriver for UcpdSinkDriver<'_, T> {
    async fn wait_for_vbus(&self) {
        // The sink policy engine is only running when attached. Therefore VBus is present.
    }

    async fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, usbpd::DriverRxError> {
        self.pd_phy.receive(buffer).await.map_err(|err| match err {
            ucpd::RxError::Crc | ucpd::RxError::Overrun => usbpd::DriverRxError::Discarded,
            ucpd::RxError::HardReset => usbpd::DriverRxError::HardReset,
        })
    }

    async fn transmit(&mut self, data: &[u8]) -> Result<(), usbpd::DriverTxError> {
        self.pd_phy.transmit(data).await.map_err(|err| match err {
            ucpd::TxError::Discarded => usbpd::DriverTxError::Discarded,
            ucpd::TxError::HardReset => usbpd::DriverTxError::HardReset,
        })
    }

    async fn transmit_hard_reset(&mut self) -> Result<(), usbpd::DriverTxError> {
        self.pd_phy
            .transmit_hardreset()
            .await
            .map_err(|err| match err {
                ucpd::TxError::Discarded => usbpd::DriverTxError::Discarded,
                ucpd::TxError::HardReset => usbpd::DriverTxError::HardReset,
            })
    }
}

async fn wait_detached<T: ucpd::Instance>(cc_phy: &mut CcPhy<'_, T>) {
    loop {
        let (cc1, cc2) = cc_phy.vstate();
        if cc1 == CcVState::LOWEST && cc2 == CcVState::LOWEST {
            return;
        }
        cc_phy.wait_for_vstate_change().await;
    }
}

// Returns true when the cable was attached.
async fn wait_attached<T: ucpd::Instance>(cc_phy: &CcPhy<'_, T>) -> CableOrientation {
    loop {
        let (cc1, cc2) = cc_phy.vstate();
        if cc1 == CcVState::LOWEST && cc2 == CcVState::LOWEST {
            // Detached, wait until attached by monitoring the CC lines.
            cc_phy.wait_for_vstate_change().await;
            continue;
        }

        // Attached, wait for CC lines to be stable for tCCDebounce (100..200ms).
        if with_timeout(Duration::from_millis(100), cc_phy.wait_for_vstate_change())
            .await
            .is_ok()
        {
            // State has changed, restart detection procedure.
            continue;
        };

        // State was stable for the complete debounce period, check orientation.
        return match (cc1, cc2) {
            (_, CcVState::LOWEST) => CableOrientation::Normal, // CC1 connected
            (CcVState::LOWEST, _) => CableOrientation::Flipped, // CC2 connected
            _ => CableOrientation::DebugAccessoryMode,         // Both connected (special cable)
        };
    }
}

struct EmbassySinkTimer {}

impl SinkTimer for EmbassySinkTimer {
    async fn after_millis(milliseconds: u64) {
        Timer::after_millis(milliseconds).await
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub enum DeviceRequest {
    GetSourceCapabilities(Arc<Signal<CriticalSectionRawMutex, Option<SourceCapabilities>>>),
}

#[derive(Clone, Debug, defmt::Format)]
#[allow(dead_code)]
pub enum RequestError {
    Mismatch,
    Unsupported,
}

struct DeviceCtx<'a> {
    active_power_source: Option<PowerSource>,
    req_rx: watch::Receiver<'a, CriticalSectionRawMutex, DeviceRequest, 1>,
    source_capabilities: Option<SourceCapabilities>,
}

#[derive(Clone)]
pub struct Device<'a> {
    ctx: Arc<Mutex<CriticalSectionRawMutex, DeviceCtx<'a>>>,
}

impl<'a> Device<'a> {
    pub fn new(req_rx: watch::Receiver<'a, CriticalSectionRawMutex, DeviceRequest, 1>) -> Self {
        Self {
            ctx: Arc::new(Mutex::new(DeviceCtx {
                active_power_source: None,
                req_rx,
                source_capabilities: None,
            })),
        }
    }
}

impl DevicePolicyManager for Device<'_> {
    async fn request(
        &mut self,
        source_capabilities: &SourceCapabilities,
    ) -> usbpd::protocol_layer::message::request::PowerSource {
        let mut ctx = self.ctx.lock().await;
        ctx.source_capabilities = Some(source_capabilities.clone());

        // 简化策略：总是请求最高电压和最大电流
        let req = PowerSource::new_fixed(
            CurrentRequest::Highest,
            VoltageRequest::Highest,
            source_capabilities,
        )
        .unwrap();

        defmt::info!("request: highest voltage and current");
        ctx.active_power_source = Some(req);

        req
    }

    async fn get_event(
        &mut self,
        _: &SourceCapabilities,
    ) -> usbpd::sink::device_policy_manager::Event {
        use usbpd::sink::device_policy_manager::Event;

        let mut ctx = self.ctx.lock().await;
        let keep_alive_ticker = Timer::after_secs(10);

        let futures = select(ctx.req_rx.changed(), keep_alive_ticker);

        match futures.await {
            Either::First(DeviceRequest::GetSourceCapabilities(resp_signal)) => {
                resp_signal.signal(ctx.source_capabilities.clone());
                Event::None
            }
            Either::Second(_) => {
                // 定期保持连接活跃
                Event::RequestSourceCapabilities
            }
        }
    }
}

#[allow(dead_code)]
pub struct SinkAgent<'a> {
    req_tx: watch::Sender<'a, CriticalSectionRawMutex, DeviceRequest, 1>,
}

impl<'a> SinkAgent<'a> {
    pub fn new(req_tx: watch::Sender<'a, CriticalSectionRawMutex, DeviceRequest, 1>) -> Self {
        Self { req_tx }
    }

    #[allow(dead_code)]
    pub async fn get_source_capabilities(&self) -> Option<SourceCapabilities> {
        let resp = Arc::new(Signal::new());
        self.req_tx
            .send(DeviceRequest::GetSourceCapabilities(resp.clone()));

        resp.wait().await
    }
}

pub struct PowerInput<'d, T, Irq, C1P, C2P, Rx, Tx>
where
    T: Instance,
    Irq: interrupt::typelevel::Binding<T::Interrupt, InterruptHandler<T>> + Clone + 'd,
    C1P: Cc1Pin<T>,
    C2P: Cc2Pin<T>,
    Rx: RxDma<T> + 'd,
    Tx: TxDma<T> + 'd,
{
    peri: Peri<'d, T>,
    irq: Irq,
    cc1: Peri<'d, C1P>,
    cc2: Peri<'d, C2P>,
    config: Config,
    rx_dma: Peri<'d, Rx>,
    tx_dma: Peri<'d, Tx>,
    device: Device<'d>,
    pd_sink_error_tx:
        channel::Sender<'d, CriticalSectionRawMutex, Arc<sink::policy_engine::Error>, 1>,
    _phantom: PhantomData<(&'d T, C1P, C2P, Rx, Tx)>,
}

impl<'d, T, Irq, C1P, C2P, Rx, Tx> PowerInput<'d, T, Irq, C1P, C2P, Rx, Tx>
where
    T: Instance,
    Irq: interrupt::typelevel::Binding<T::Interrupt, InterruptHandler<T>> + Clone + 'd,
    C1P: Cc1Pin<T>,
    C2P: Cc2Pin<T>,
    Rx: RxDma<T> + 'd,
    Tx: TxDma<T> + 'd,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        peri: Peri<'d, T>,
        irq: Irq,
        cc1: Peri<'d, C1P>,
        cc2: Peri<'d, C2P>,
        config: Config,
        rx_dma: Peri<'d, Rx>,
        tx_dma: Peri<'d, Tx>,
        device: Device<'d>,
        pd_sink_error_tx: channel::Sender<
            'd,
            CriticalSectionRawMutex,
            Arc<sink::policy_engine::Error>,
            1,
        >,
    ) -> Self {
        Self {
            peri,
            irq,
            cc1,
            cc2,
            config,
            rx_dma,
            tx_dma,
            device,
            _phantom: PhantomData,
            pd_sink_error_tx,
        }
    }

    pub async fn run(&mut self) {
        loop {
            let mut ucpd = Ucpd::new(
                self.peri.reborrow(),
                self.irq.clone(),
                self.cc1.reborrow(),
                self.cc2.reborrow(),
                self.config,
            );
            ucpd.cc_phy().set_pull(CcPull::Sink);
            info!("Waiting for USB connection...");
            let cable_orientation = wait_attached(ucpd.cc_phy()).await;
            info!("USB cable attached, orientation: {}", cable_orientation);

            let cc_sel = match cable_orientation {
                CableOrientation::Normal => {
                    info!("Starting PD communication on CC1 pin");
                    CcSel::CC1
                }
                CableOrientation::Flipped => {
                    info!("Starting PD communication on CC2 pin");
                    CcSel::CC2
                }
                CableOrientation::DebugAccessoryMode => panic!("No PD communication in DAM"),
            };
            let (mut cc_phy, pd_phy) =
                ucpd.split_pd_phy(self.rx_dma.reborrow(), self.tx_dma.reborrow(), cc_sel);

            let driver = UcpdSinkDriver::new(pd_phy);
            let mut sink: Sink<UcpdSinkDriver<'_, T>, EmbassySinkTimer, _> =
                Sink::new(driver, self.device.clone());
            info!("Run sink");

            match select(sink.run(), wait_detached(&mut cc_phy)).await {
                Either::First(result) => {
                    warn!("Sink loop broken with result: {}", result);
                    if let Err(err) = result {
                        self.pd_sink_error_tx.send(Arc::new(err)).await;
                        // This is an unrecoverable error for this session.
                        // Terminate the task to release the UCPD peripheral.
                        warn!("Unrecoverable PD error. Terminating task.");
                        return;
                    }
                }
                Either::Second(_) => {
                    info!("Detached");
                    // Loop to wait for a new connection.
                    continue;
                }
            }
        }
    }
}

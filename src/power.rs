use alloc::sync::Arc;
use core::{marker::PhantomData, usize};
use defmt::{error, info, warn, Format};
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
use uom::si::{
    electric_current::milliampere,
    electric_potential::{millivolt, volt},
};
use usbpd::{
    protocol_layer::message::{
        pdo::{Augmented, PowerDataObject, SourceCapabilities, SprProgrammablePowerSupply},
        request::{CurrentRequest, PowerSource, VoltageRequest},
        units::{ElectricCurrent, ElectricPotential},
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

#[derive(Debug, PartialEq)]
pub struct TargetPower {
    pub voltage: ElectricPotential,
    pub current: ElectricCurrent,
}

impl Clone for TargetPower {
    fn clone(&self) -> Self {
        Self {
            voltage: self.voltage,
            current: self.current,
        }
    }
}

impl defmt::Format for TargetPower {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "{} mA @ {} mV",
            self.current.get::<milliampere>(),
            self.voltage.get::<millivolt>()
        );
    }
}

impl Default for TargetPower {
    fn default() -> Self {
        Self {
            voltage: ElectricPotential::new::<volt>(5),
            current: ElectricCurrent::new::<milliampere>(500),
        }
    }
}

#[derive(Clone)]
pub enum DeviceRequest {
    Request(
        TargetPower,
        Arc<Signal<CriticalSectionRawMutex, Result<(), RequestError>>>,
    ),
    SetTarget(TargetPower),
    GetSourceCapabilities(Arc<Signal<CriticalSectionRawMutex, Option<SourceCapabilities>>>),
    GetActiveTarget(Arc<Signal<CriticalSectionRawMutex, TargetPower>>),
    SelectPdo(
        usize, // PDO index
        Arc<Signal<CriticalSectionRawMutex, Result<(), RequestError>>>,
    ),
    GetActivePdo(Arc<Signal<CriticalSectionRawMutex, Option<(usize, PowerDataObject)>>>),
}

#[derive(Clone, Debug, defmt::Format)]
pub enum RequestError {
    Mismatch,
    Unsupported,
}

#[derive(Clone, Debug)]
pub enum PdoType {
    Fixed {
        voltage: ElectricPotential,
        max_current: ElectricCurrent,
    },
    Pps {
        min_voltage: ElectricPotential,
        max_voltage: ElectricPotential,
        max_current: ElectricCurrent,
    },
    Other,
}

impl PdoType {
    pub fn from_pdo(pdo: &PowerDataObject) -> Self {
        match pdo {
            PowerDataObject::FixedSupply(fixed) => PdoType::Fixed {
                voltage: fixed.voltage(),
                max_current: fixed.max_current(),
            },
            PowerDataObject::Augmented(Augmented::Spr(pps)) => PdoType::Pps {
                min_voltage: pps.min_voltage(),
                max_voltage: pps.max_voltage(),
                max_current: pps.max_current(),
            },
            _ => PdoType::Other,
        }
    }

    pub fn supports_voltage_adjustment(&self) -> bool {
        matches!(self, PdoType::Pps { .. })
    }

    pub fn supports_current_adjustment(&self) -> bool {
        matches!(self, PdoType::Pps { .. })
    }
}

struct DeviceCtx<'a> {
    active_target: TargetPower,
    active_power_source: Option<PowerSource>,
    req_rx: watch::Receiver<'a, CriticalSectionRawMutex, DeviceRequest, 1>,
    source_capabilities: Option<SourceCapabilities>,
    requesting: Option<(
        TargetPower,
        Arc<Signal<CriticalSectionRawMutex, Result<(), RequestError>>>,
    )>,
    selected_pdo_index: Option<usize>,
}

#[derive(Clone)]
pub struct Device<'a> {
    ctx: Arc<Mutex<CriticalSectionRawMutex, DeviceCtx<'a>>>,
}

impl<'a> Device<'a> {
    pub fn new(req_rx: watch::Receiver<'a, CriticalSectionRawMutex, DeviceRequest, 1>) -> Self {
        Self {
            ctx: Arc::new(Mutex::new(DeviceCtx {
                active_target: Default::default(),
                active_power_source: None,
                req_rx,
                source_capabilities: None,
                requesting: Default::default(),
                selected_pdo_index: None,
            })),
        }
    }

    fn find_pdo_index_for_request(
        request: &PowerSource,
        source_capabilities: &SourceCapabilities,
    ) -> Option<usize> {
        match request {
            PowerSource::Pps(pps_request) => {
                // For PPS requests, find the matching PPS PDO
                for (index, pdo) in source_capabilities.pdos().iter().enumerate() {
                    if let PowerDataObject::Augmented(Augmented::Spr(pps_pdo)) = pdo {
                        // Check if this PPS PDO can satisfy the request
                        let req_voltage = pps_request.output_voltage();
                        let req_current = pps_request.operating_current();

                        if req_voltage >= pps_pdo.min_voltage()
                            && req_voltage <= pps_pdo.max_voltage()
                            && req_current <= pps_pdo.max_current()
                        {
                            return Some(index);
                        }
                    }
                }
            }
            _ => {
                // For non-PPS requests, we can't easily determine the PDO index
                // without more complex matching logic. For now, return None.
                return None;
            }
        }
        None
    }
}

impl<'a> DevicePolicyManager for Device<'a> {
    async fn request(
        &mut self,
        source_capabilities: &SourceCapabilities,
    ) -> usbpd::protocol_layer::message::request::PowerSource {
        let mut ctx = self.ctx.lock().await;
        ctx.source_capabilities = Some(source_capabilities.clone());
        let (target, resp_signal) = match ctx.requesting.take() {
            Some((target, resp_signal)) => (target, Some(resp_signal)),
            None => {
                let target = ctx.active_target.clone();
                (target, None)
            }
        };

        match usbpd::protocol_layer::message::request::PowerSource::new_pps(
            usbpd::protocol_layer::message::request::CurrentRequest::Specific(target.current),
            target.voltage,
            source_capabilities,
        ) {
            Ok(req) => {
                if let PowerSource::Pps(req) = &req {
                    if req.capability_mismatch() {
                        error!("capability_mismatch. {}", target);

                        if let Some(resp_signal) = resp_signal {
                            resp_signal.signal(Err(RequestError::Mismatch));
                        }

                        return ctx.active_power_source.unwrap_or(
                            PowerSource::new_fixed(
                                CurrentRequest::Highest,
                                VoltageRequest::Safe5V,
                                source_capabilities,
                            )
                            .unwrap(),
                        );
                    }
                }

                defmt::info!("request: {}, {}", target, req);
                if let Some(resp_signal) = resp_signal {
                    resp_signal.signal(Ok(()));
                }
                ctx.active_target = target;
                ctx.active_power_source = Some(req.clone());

                // Update selected_pdo_index based on the request
                ctx.selected_pdo_index =
                    Self::find_pdo_index_for_request(&req, source_capabilities);

                return req;
            }
            Err(_) => {
                error!("can not get request. {}", target);

                if let Some(resp_signal) = resp_signal {
                    resp_signal.signal(Err(RequestError::Unsupported));
                }

                match ctx.active_power_source {
                    Some(req) => return req,
                    None => PowerSource::new_fixed(
                        CurrentRequest::Highest,
                        VoltageRequest::Safe5V,
                        source_capabilities,
                    )
                    .unwrap(),
                }
            }
        }
    }

    async fn get_event(
        &mut self,
        _: &SourceCapabilities,
    ) -> usbpd::sink::device_policy_manager::Event {
        use usbpd::sink::device_policy_manager::Event;

        let mut ctx = self.ctx.lock().await;
        let keep_pps_alive_ticker = Timer::after_secs(5);

        let futures = select(ctx.req_rx.changed(), keep_pps_alive_ticker);

        match futures.await {
            Either::First(DeviceRequest::Request(target, resp_signal)) => {
                ctx.requesting = Some((target, resp_signal));
                return Event::RequestSourceCapabilities;
            }
            Either::First(DeviceRequest::SetTarget(target)) => {
                ctx.active_target = target;
                return Event::None;
            }
            Either::First(DeviceRequest::GetSourceCapabilities(resp_signal)) => {
                resp_signal.signal(ctx.source_capabilities.clone());
                return Event::None;
            }
            Either::First(DeviceRequest::GetActiveTarget(resp_signal)) => {
                resp_signal.signal(ctx.active_target.clone());
                return Event::None;
            }
            Either::First(DeviceRequest::SelectPdo(pdo_index, resp_signal)) => {
                if let Some(ref caps) = ctx.source_capabilities {
                    if let Some(pdo) = caps.pdos().get(pdo_index) {
                        // Create target power based on PDO type
                        let target = match PdoType::from_pdo(pdo) {
                            PdoType::Fixed {
                                voltage,
                                max_current,
                            } => TargetPower {
                                voltage,
                                current: max_current,
                            },
                            PdoType::Pps {
                                min_voltage,
                                max_voltage: _,
                                max_current,
                            } => {
                                // Start with minimum voltage for PPS
                                TargetPower {
                                    voltage: min_voltage,
                                    current: max_current,
                                }
                            }
                            PdoType::Other => {
                                resp_signal.signal(Err(RequestError::Unsupported));
                                return Event::None;
                            }
                        };
                        ctx.requesting = Some((target, resp_signal));
                        ctx.selected_pdo_index = Some(pdo_index);
                        return Event::RequestSourceCapabilities;
                    }
                }
                resp_signal.signal(Err(RequestError::Unsupported));
                return Event::None;
            }
            Either::First(DeviceRequest::GetActivePdo(resp_signal)) => {
                let result = if let (Some(ref caps), Some(index)) =
                    (&ctx.source_capabilities, ctx.selected_pdo_index)
                {
                    caps.pdos().get(index).map(|pdo| (index, pdo.clone()))
                } else {
                    None
                };
                resp_signal.signal(result);
                return Event::None;
            }
            Either::Second(_) => {
                return Event::RequestSourceCapabilities;
            }
        }
    }
}

pub struct SinkAgent<'a> {
    req_tx: watch::Sender<'a, CriticalSectionRawMutex, DeviceRequest, 1>,
}

impl<'a> SinkAgent<'a> {
    pub fn new(req_tx: watch::Sender<'a, CriticalSectionRawMutex, DeviceRequest, 1>) -> Self {
        Self { req_tx }
    }

    pub async fn request(&self, target: TargetPower) -> Result<(), RequestError> {
        let resp = Arc::new(Signal::<CriticalSectionRawMutex, Result<(), RequestError>>::new());
        self.req_tx
            .send(DeviceRequest::Request(target, resp.clone()));

        resp.wait().await
    }

    pub async fn get_pps(&self) -> Option<SprProgrammablePowerSupply> {
        let resp = Arc::new(Signal::new());
        self.req_tx
            .send(DeviceRequest::GetSourceCapabilities(resp.clone()));

        let source_capabilities = resp.wait().await;

        source_capabilities
            .as_ref()
            .map(|caps| {
                caps.pdos().iter().find_map(|pdo| match pdo {
                    PowerDataObject::Augmented(Augmented::Spr(pps)) => Some(pps.clone()),
                    _ => None,
                })
            })
            .unwrap_or_default()
    }

    pub async fn get_target(&self) -> TargetPower {
        let resp = Arc::new(Signal::new());
        self.req_tx
            .send(DeviceRequest::GetActiveTarget(resp.clone()));

        resp.wait().await
    }

    pub async fn set_target(&self, target: TargetPower) {
        self.req_tx.send(DeviceRequest::SetTarget(target));
    }

    pub async fn get_source_capabilities(&self) -> Option<SourceCapabilities> {
        let resp = Arc::new(Signal::new());
        self.req_tx
            .send(DeviceRequest::GetSourceCapabilities(resp.clone()));

        resp.wait().await
    }

    pub async fn select_pdo(&self, pdo_index: usize) -> Result<(), RequestError> {
        let resp = Arc::new(Signal::<CriticalSectionRawMutex, Result<(), RequestError>>::new());
        self.req_tx
            .send(DeviceRequest::SelectPdo(pdo_index, resp.clone()));

        resp.wait().await
    }

    pub async fn get_active_pdo(&self) -> Option<(usize, PowerDataObject)> {
        let resp = Arc::new(Signal::new());
        self.req_tx.send(DeviceRequest::GetActivePdo(resp.clone()));

        resp.wait().await
    }

    pub async fn get_active_pdo_type(&self) -> Option<PdoType> {
        self.get_active_pdo()
            .await
            .map(|(_, pdo)| PdoType::from_pdo(&pdo))
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
                self.config.clone(),
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

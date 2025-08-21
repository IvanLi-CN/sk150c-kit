#![allow(dead_code)]

use alloc::sync::Arc;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, Sender},
    mutex::Mutex,
    signal::Signal,
    watch,
};

// use m24c64_driver::M24C64; // 暂时注释掉，因为不再使用 EEPROM
use uom::si::{electric_current::milliampere, electric_potential::millivolt};
use usbpd::protocol_layer::message::units::{ElectricCurrent, ElectricPotential};

#[derive(Debug, defmt::Format)]
pub enum ConfigManagerError {
    I2CError,
}

enum Register {
    TargetVoltage = 0x00,
    TargetCurrent = 0x04,
}

impl From<Register> for usize {
    fn from(value: Register) -> Self {
        value as usize
    }
}

pub struct ConfigManager {
    // 简化版本，不使用 EEPROM 存储
}

impl ConfigManager {
    pub fn new() -> Self {
        ConfigManager {}
    }

    async fn read(
        &mut self,
        _register: Register,
        _buffer: &mut [u8],
    ) -> Result<(), ConfigManagerError> {
        // 简化版本：不实际读取 EEPROM
        Ok(())
    }

    async fn write(&mut self, _register: Register, _data: &[u8]) -> Result<(), ConfigManagerError> {
        // 简化版本：不实际写入 EEPROM
        Ok(())
    }

    pub async fn read_target_voltage(&mut self) -> Result<ElectricPotential, ConfigManagerError> {
        let mut data = [0u8; 4];
        self.read(Register::TargetVoltage, &mut data).await?;

        let value = u32::from_be_bytes(data);

        Ok(ElectricPotential::new::<millivolt>(
            value.clamp(3000, 48_000),
        ))
    }

    pub async fn write_target_voltage(
        &mut self,
        voltage: ElectricPotential,
    ) -> Result<(), ConfigManagerError> {
        let value = voltage.get::<millivolt>();
        self.write(Register::TargetVoltage, &value.to_be_bytes())
            .await
    }

    pub async fn read_target_current(&mut self) -> Result<ElectricCurrent, ConfigManagerError> {
        let mut data = [0u8; 4];
        self.read(Register::TargetCurrent, &mut data).await?;

        let value = u32::from_be_bytes(data);

        Ok(ElectricCurrent::new::<milliampere>(value.clamp(100, 5_000)))
    }

    pub async fn write_target_current(
        &mut self,
        current: ElectricCurrent,
    ) -> Result<(), ConfigManagerError> {
        let value = current.get::<milliampere>();
        self.write(Register::TargetCurrent, &value.to_be_bytes())
            .await
    }

    pub async fn exec(&mut self, req: ConfigRequest) -> Result<(), ConfigManagerError> {
        match req {
            ConfigRequest::WriteTargetVoltage(voltage, resp) => {
                let res = self.write_target_voltage(voltage).await;
                resp.signal(res);
            }
            ConfigRequest::WriteTargetCurrent(current, resp) => {
                let res = self.write_target_current(current).await;
                resp.signal(res);
            }
        }

        Ok(())
    }

    pub async fn read_config(&mut self) -> Result<Config, ConfigManagerError> {
        let target_voltage = self.read_target_voltage().await?;
        let target_current = self.read_target_current().await?;

        Ok(Config {
            target_voltage,
            target_current,
        })
    }

    pub async fn reset_config(&mut self) -> Result<(), ConfigManagerError> {
        let config = Config::default();

        self.write_target_voltage(config.target_voltage).await?;
        self.write_target_current(config.target_current).await?;

        Ok(())
    }
}

pub enum ConfigRequest {
    WriteTargetVoltage(
        ElectricPotential,
        Arc<Signal<CriticalSectionRawMutex, Result<(), ConfigManagerError>>>,
    ),
    WriteTargetCurrent(
        ElectricCurrent,
        Arc<Signal<CriticalSectionRawMutex, Result<(), ConfigManagerError>>>,
    ),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub target_voltage: ElectricPotential,
    pub target_current: ElectricCurrent,
}

impl defmt::Format for Config {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "target: {}mV, {}mA",
            self.target_voltage.get::<millivolt>(),
            self.target_current.get::<milliampere>()
        );
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            target_voltage: ElectricPotential::new::<millivolt>(5000),
            target_current: ElectricCurrent::new::<milliampere>(500),
        }
    }
}

pub struct ConfigAgent<'a> {
    req_tx: Sender<'a, CriticalSectionRawMutex, ConfigRequest, 1>,
    snapshot_rx:
        Mutex<CriticalSectionRawMutex, watch::Receiver<'a, CriticalSectionRawMutex, Config, 1>>,
}

impl<'a> ConfigAgent<'a> {
    pub fn new(
        req_tx: Sender<'a, CriticalSectionRawMutex, ConfigRequest, 1>,
        snapshot_rx: watch::Receiver<'a, CriticalSectionRawMutex, Config, 1>,
    ) -> Self {
        ConfigAgent {
            req_tx,
            snapshot_rx: Mutex::new(snapshot_rx),
        }
    }

    pub fn create(
        req_ch: &'a Channel<CriticalSectionRawMutex, ConfigRequest, 1>,
        snapshot_ch: &'a watch::Watch<CriticalSectionRawMutex, Config, 1>,
    ) -> Result<Self, ()> {
        Ok(ConfigAgent::new(
            req_ch.sender(),
            snapshot_ch.receiver().unwrap(),
        ))
    }

    pub async fn write_target_voltage(&self, voltage: ElectricPotential) {
        let signal = Arc::new(Signal::new());
        self.req_tx
            .send(ConfigRequest::WriteTargetVoltage(voltage, signal.clone()))
            .await;
        signal.wait().await.ok();
    }

    pub async fn write_target_current(&self, current: ElectricCurrent) {
        let signal = Arc::new(Signal::new());
        self.req_tx
            .send(ConfigRequest::WriteTargetCurrent(current, signal.clone()))
            .await;
        signal.wait().await.ok();
    }

    pub async fn snapshot(&self) -> Config {
        let mut rx = self.snapshot_rx.lock().await;
        rx.get().await
    }

    pub fn get_cached_config(&self) -> Config {
        self.snapshot_rx
            .try_lock()
            .unwrap()
            .try_get()
            .unwrap_or_default()
    }
}

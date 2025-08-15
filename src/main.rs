#![no_std]
#![no_main]

use adc_reader::{AdcCalibration, AdcReader};
use alloc::sync::Arc;
use app_manager::{AppContext, AppManager};
use button::InputManager;
use config_manager::{ConfigAgent, ConfigManager};
use core::{
    mem::MaybeUninit,
    ptr::{read_volatile, write_volatile},
};
use defmt_rtt as _;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;

use embassy_executor::Spawner;
use embassy_stm32::{
    adc::{
        vals::{Rovsm, Trovs},
        Adc, AdcChannel, SampleTime,
    },
    bind_interrupts,
    exti::ExtiInput,
    gpio::{Level, Output, OutputType, Pull, Speed},
    i2c,
    peripherals::{self, DMA2_CH4, DMA2_CH5, PB4, PB6, UCPD1},
    spi::{self, Spi},
    time::{khz, Hertz},
    timer::simple_pwm::{PwmPin, SimplePwm},
    ucpd::{self},
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    mutex::Mutex,
    pubsub::{PubSubBehavior, Subscriber},
};
use embassy_time::Duration;
use embedded_alloc::LlffHeap as Heap;
use panic_probe as _;
use power::PowerInput;
use power_output::PowerOutput;
use shared::*;
use static_cell::StaticCell;
use types::*;
use usb::usb_task;

mod adc_reader;
mod app_manager;
mod button;
mod comp;
mod config_manager;
mod power;
mod power_output;
mod shared;
mod types;
mod usb;

const VREFBUF_BASE: u32 = 0x40010030;
const VREFBUF_CSR_ADDR: *mut u32 = (VREFBUF_BASE + 0x00) as *mut u32;
const TS_CAL1_ADDR: *mut u16 = 0x1FFF75A8 as *mut u16;
const TS_CAL2_ADDR: *mut u16 = 0x1FFF75CA as *mut u16;
const VREFINT_DATA_ADDR: *mut u16 = 0x1FFF75AA as *mut u16;

const ADC_READER_BUF_SIZE: usize = 8; // 最小缓冲区大小

static I2C_BUS_MUTEX: StaticCell<SharedI2cBus> = StaticCell::new();
static mut ADC_READER: MaybeUninit<AdcReader<'static, ADC_READER_BUF_SIZE>> = MaybeUninit::uninit();
static INPUT_MANAGER: StaticCell<MaybeUninit<InputManager>> = StaticCell::new();
static POWER_OUTPUT: StaticCell<MaybeUninit<PowerOutput>> = StaticCell::new();

extern crate alloc;

#[global_allocator]
static HEAP: Heap = Heap::empty();

// This marks the entrypoint of our application.
bind_interrupts!(
    struct Irqs {
        UCPD1 => ucpd::InterruptHandler<peripherals::UCPD1>;
        USB_LP => embassy_stm32::usb::InterruptHandler<peripherals::USB>;
        I2C3_EV => i2c::EventInterruptHandler<peripherals::I2C3>;
        I2C3_ER => i2c::ErrorInterruptHandler<peripherals::I2C3>;
    }
);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Initialize the allocator BEFORE you use it
    {
        use core::mem::MaybeUninit;
        const HEAP_SIZE: usize = 4096; // 增加堆大小到 4KB
        static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { HEAP.init(HEAP_MEM.as_mut_ptr() as usize, HEAP_SIZE) }
    }

    let mut config = embassy_stm32::Config::default();
    {
        use embassy_stm32::rcc::*;
        config.rcc.hsi48 = Some(Hsi48Config {
            sync_from_usb: true,
        });
        config.rcc.pll = Some(Pll {
            source: PllSource::HSI,
            prediv: PllPreDiv::DIV4,
            mul: PllMul::MUL85,
            divp: None,
            divq: None,
            // Main system clock at 170 MHz
            divr: Some(PllRDiv::DIV2),
        });
        config.rcc.mux.adc12sel = mux::Adcsel::SYS;
        config.rcc.sys = Sysclk::PLL1_R;
        config.rcc.mux.clk48sel = mux::Clk48sel::HSI48;
        // config.enable_ucpd1_dead_battery = true;
    }
    let p = embassy_stm32::init(config);
    defmt::info!("STM32 initialized successfully");

    unsafe {
        write_volatile(VREFBUF_CSR_ADDR, 0x0000_0021 as u32);
    }
    defmt::info!("VREFBUF configured");

    // input manager

    let buttons = [
        ExtiInput::new(p.PB8, p.EXTI8, Pull::Down), // BTN0 - active high
        ExtiInput::new(p.PC10, p.EXTI10, Pull::Up), // BTN1 - active low
        ExtiInput::new(p.PB1, p.EXTI1, Pull::Up),   // BTN2 - active low
        ExtiInput::new(p.PC13, p.EXTI13, Pull::Up), // BTN3 - 占位符 (PB2 现在用作输出控制)
    ];

    let input_mgr = InputManager::new(buttons, Duration::from_millis(50), Duration::from_secs(1));
    defmt::info!("Input manager created");

    let input_mgr = INPUT_MANAGER.init(MaybeUninit::new(input_mgr));
    defmt::info!("Input manager initialized");
    let input_manager = unsafe { input_mgr.assume_init_mut() };

    // 暂时跳过 I2C 初始化以简化调试
    defmt::info!("Skipping I2C initialization for debugging");

    defmt::info!("Skipping motion sensor and EEPROM for debugging");

    let config_snapshot_tx = CONFIG_SNAPSHOT_CHANNEL.sender();
    config_snapshot_tx.send(Default::default());
    defmt::info!("Using default config");

    // 软件欠压保护将在power_output创建后启动
    defmt::info!("软件欠压保护将在稍后启动");

    let power_device = power::Device::new(SINK_REQUEST_CHANNEL.receiver().unwrap());

    let sink_agent = power::SinkAgent::new(SINK_REQUEST_CHANNEL.sender());

    let pd_service = PowerInput::new(
        p.UCPD1,
        Irqs,
        p.PB6,
        p.PB4,
        ucpd::Config::default(),
        p.DMA2_CH4,
        p.DMA2_CH5,
        power_device,
        PD_ERROR_CHANNEL.sender(),
    );
    spawner.spawn(pd_task(pd_service)).unwrap();

    let mut adc1 = Adc::new(p.ADC1);
    adc1.set_sample_time(SampleTime::CYCLES640_5);
    adc1.set_oversampling_ratio(0x07); // ratio X256
    adc1.set_oversampling_shift(4); // shift 4
    adc1.enable_regular_oversampling_mode(Rovsm::RESUMED, Trovs::AUTOMATIC, true);
    let mut adc2 = Adc::new(p.ADC2);
    adc2.set_sample_time(SampleTime::CYCLES640_5);
    adc1.set_oversampling_ratio(0x07); // ratio X256
    adc2.set_oversampling_shift(4); // shift 4
    adc2.enable_regular_oversampling_mode(Rovsm::RESUMED, Trovs::AUTOMATIC, true);
    // 根据 .ioc 文件配置 ADC 通道
    // PA0: VOUT_SN (ADC1_IN1) - 输出电压检测
    // PA1: VIN_SN (ADC2_IN2) - 输入电压检测
    let vout_sn_ch = p.PA0.degrade_adc(); // ADC1_IN1
    let vin_sn_ch = p.PA1.degrade_adc(); // ADC2_IN2

    let v_temp_ch = adc1.enable_temperature().degrade_adc();
    let v_ref_int_ch = adc1.enable_vrefint().degrade_adc();

    let ts_cal1 = unsafe { read_volatile(TS_CAL1_ADDR as *const u16) } as f64;
    let ts_cal2 = unsafe { read_volatile(TS_CAL2_ADDR as *const u16) } as f64;
    let vrefint_cal = unsafe { read_volatile(VREFINT_DATA_ADDR as *const u16) } as f64;

    defmt::info!("ts_cal1 = {}", ts_cal1);
    defmt::info!("ts_cal2 = {}", ts_cal2);
    defmt::info!("vrefint_cal = {}", vrefint_cal);

    let dma_ch1 = p.DMA1_CH1;
    let _dma_ch2 = p.DMA1_CH2;

    // Init INA186 REF

    // let mut dac3 = Dac::new_internal(p.DAC3, p.DMA1_CH3, p.DMA1_CH4);
    // dac3.ch1().set_mode(dac::Mode::NormalInternalUnbuffered);
    // dac3.ch1().enable();
    // dac3.ch1().set(dac::Value::Bit12Left(2048));
    // let mut ref_buffer = OpAmp::new(p.OPAMP1, embassy_stm32::opamp::OpAmpSpeed::HighSpeed);
    // ref_buffer.buffer_dac(p.PA2);
    let mut ina_ref_pin = Output::new(p.PA4, Level::Low, Speed::Low);
    ina_ref_pin.set_low();

    // 根据 .ioc 文件配置硬件引脚
    // PA15: VIN_CE (输入控制使能)
    let vin_ce_pin = Output::new(p.PA15, Level::Low, Speed::Low);
    defmt::info!("VIN_CE pin PA15 configured");

    // PB5: VBUS_CE (VBUS控制使能)
    let vbus_ce_pin = Output::new(p.PB5, Level::Low, Speed::Low);
    defmt::info!("VBUS_CE pin PB5 configured");

    // 创建PowerOutput用于电源控制
    let power_output = PowerOutput::new(vbus_ce_pin);
    let power_output = POWER_OUTPUT.init(MaybeUninit::new(power_output));
    let power_output = unsafe { power_output.assume_init_mut() };

    // 启动软件欠压保护任务
    let uvp_config = comp::UvpConfig::default(); // 使用默认配置：5V阈值，自动恢复
    spawner
        .spawn(comp::undervoltage_protection_task(
            power_output.clone(),
            uvp_config,
        ))
        .unwrap();
    defmt::info!("软件欠压保护任务已启动");

    let adc_calibration = AdcCalibration {
        ts_cal1,
        ts_cal2,
        vrefint_cal,
    };

    cortex_m::interrupt::free(|_| {
        let adc_reader = AdcReader::new(
            adc1,
            dma_ch1,
            vout_sn_ch,
            vin_sn_ch,
            v_temp_ch,
            v_ref_int_ch,
            adc_calibration,
        );
        unsafe {
            ADC_READER.write(adc_reader);
        }
    });

    spawner.spawn(adc_task()).unwrap();
    // Spawn input management task
    spawner.spawn(input_task(input_manager)).unwrap();

    // 暂时禁用 USB 任务以减少代码大小
    // let driver = embassy_stm32::usb::Driver::new(p.USB, Irqs, p.PA12, p.PA11);
    // spawner.spawn(usb_task(driver)).unwrap();

    // Get input event subscriber

    let input_subscriber = input_manager.subscriber();

    if let Err(e) = input_subscriber {
        defmt::panic!("Failed to subscribe to input events: {}", e);
    }

    let config_agent =
        ConfigAgent::create(&CONFIG_REQUEST_CHANNEL, &CONFIG_SNAPSHOT_CHANNEL).unwrap();

    let app_ctx = AppContext {
        config: Arc::new(config_agent),
        input_rx: Arc::new(Mutex::new(input_subscriber.unwrap())),
        output: Arc::new(power_output.clone()),
        sink: Arc::new(sink_agent),
    };
    let mut app_manager = AppManager::new(app_ctx.clone());

    defmt::info!("Initializing app manager...");
    app_manager.init().await;
    defmt::info!("App manager initialized successfully");

    defmt::info!("Entering main loop");
    let mut counter = 0u32;
    loop {
        app_manager.tick().await;

        // 每1000次循环打印一次调试信息
        counter = counter.wrapping_add(1);
        if counter % 1000 == 0 {
            defmt::info!("Main loop running, counter: {}", counter);
        }

        // 添加小延迟避免过度占用CPU
        embassy_time::Timer::after_millis(1).await;
    }
}

#[embassy_executor::task]
async fn input_task(input_manager: &'static InputManager) {
    let mut input_manager = input_manager.clone();
    loop {
        input_manager.tick().await;
    }
}

#[embassy_executor::task]
async fn adc_task() {
    let adc_reader = unsafe { ADC_READER.assume_init_mut() };

    loop {
        if let Some(values) = adc_reader.poll().await {
            ADC_PUBSUB.publish_immediate((values.0, values.1));
            defmt::info!("ADC: V={}V, I={}A, T={}°C", values.0, values.1, values.2);
        }
    }
}

#[embassy_executor::task]
async fn config_task(mut config_manager: ConfigManager) {
    let config_req_rx = CONFIG_REQUEST_CHANNEL.receiver();
    loop {
        let req = config_req_rx.receive().await;
        match config_manager.exec(req).await {
            Ok(_) => {}
            Err(e) => {
                defmt::error!("config error: {}", e);
            }
        }
    }
}

#[embassy_executor::task]
async fn pd_task(mut pd_service: PowerInput<'static, UCPD1, Irqs, PB6, PB4, DMA2_CH4, DMA2_CH5>) {
    pd_service.run().await;
}

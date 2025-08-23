#![no_std]
#![no_main]

use adc_reader::{AdcCalibration, AdcReader};
use alloc::sync::Arc;
use app_manager::{PowerManager, PowerManagerContext};
use button::InputManager;
use config_manager::ConfigManager;
use vbus_manager::{VbusManager, VbusManagerContext};

use core::{
    mem::MaybeUninit,
    ptr::{read_volatile, write_volatile},
};
use defmt_rtt as _;

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
    time::khz,
    timer::simple_pwm::{PwmPin, SimplePwm},
    timer::Channel,
    ucpd::{self},
};
use embassy_sync::{mutex::Mutex, pubsub::PubSubBehavior};
use embassy_time::Duration;
use embedded_alloc::LlffHeap as Heap;
use embedded_hal_02::Pwm;

use panic_probe as _;
use power::PowerInput;
use power_output::PowerOutput;
use shared::*;
use static_cell::StaticCell;
use types::*;

mod adc_reader;
mod app_manager;
mod button;
mod config_manager;
mod fan_manager;
mod power;
mod power_output;
mod shared;
mod types;
mod usb;
mod vbus_manager;

mod tests;

const VREFBUF_BASE: u32 = 0x40010030;
const VREFBUF_CSR_ADDR: *mut u32 = VREFBUF_BASE as *mut u32;
const TS_CAL1_ADDR: *mut u16 = 0x1FFF75A8 as *mut u16;
const TS_CAL2_ADDR: *mut u16 = 0x1FFF75CA as *mut u16;
const VREFINT_DATA_ADDR: *mut u16 = 0x1FFF75AA as *mut u16;

const ADC_READER_BUF_SIZE: usize = 8; // 最小缓冲区大小

#[allow(dead_code)]
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
        #[allow(static_mut_refs)]
        unsafe {
            HEAP.init(HEAP_MEM.as_mut_ptr() as usize, HEAP_SIZE)
        }
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
        write_volatile(VREFBUF_CSR_ADDR, 0x0000_0021_u32);
    }
    defmt::info!("VREFBUF configured");

    // 简化的单按钮输入管理器 - 只使用PB8
    let power_button = ExtiInput::new(p.PB8, p.EXTI8, Pull::Down); // PB8 - 高电平有效
                                                                   // 消抖时间50ms，长按阈值1000ms（1s）
    let input_mgr = InputManager::new(
        power_button,
        Duration::from_millis(50),
        Duration::from_millis(1000),
    );
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

    let _sink_agent = power::SinkAgent::new(SINK_REQUEST_CHANNEL.sender());

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
    adc1.set_sample_time(SampleTime::CYCLES640_5); // 保持较长的采样时间
    adc1.set_oversampling_ratio(0x07); // ratio X256
    adc1.set_oversampling_shift(4); // shift 4
    adc1.enable_regular_oversampling_mode(Rovsm::RESUMED, Trovs::AUTOMATIC, true);
    let mut adc2 = Adc::new(p.ADC2);
    adc2.set_sample_time(SampleTime::CYCLES640_5); // 保持较长的采样时间
    adc2.set_oversampling_ratio(0x07); // ratio X256 (修正：应该是adc2)
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

    // PB7: VBUS_EN (VBUS控制使能) - USB-C 电源输出开关控制
    let vbus_en_pin = Output::new(p.PB7, Level::Low, Speed::Low);
    defmt::info!("VBUS_EN pin PB7 configured");

    // PB5: VBUS_LED (双色LED控制) - 改为 GPIO 输出模式
    let vbus_led_pin = Output::new(p.PB5, Level::Low, Speed::Low);
    defmt::info!("VBUS_LED pin PB5 configured");

    // PB10: FAN_PWM2 (风扇控制) - 配置为GPIO输出，高电平启动风扇
    let fan_control_pin = Output::new(p.PB10, Level::Low, Speed::Low);
    defmt::info!("FAN_PWM2 pin PB10 configured as GPIO output");

    // PA8: POWER_LED (TIM1_CH1) - PWM 呼吸灯控制
    // 配置为开漏输出，低电平点亮LED
    use embassy_stm32::timer::simple_pwm::PwmPinConfig;
    let pin_config = PwmPinConfig {
        output_type: OutputType::OpenDrain,
        speed: Speed::Low,
        pull: Pull::None,
    };
    let ch1 = PwmPin::new_with_config(p.PA8, pin_config);
    let mut pwm = SimplePwm::new(
        p.TIM1,
        Some(ch1),
        None,
        None,
        None,
        khz(1), // 1kHz PWM频率
        Default::default(),
    );
    // 设置最大占空比 - 使用embassy-stm32的API
    let max_duty = pwm.get_max_duty();
    pwm.set_duty(Channel::Ch1, 0); // 初始状态LED熄灭
    pwm.enable(Channel::Ch1);
    defmt::info!("PWM for PA8 (POWER_LED) configured, max_duty: {}", max_duty);

    // 创建PowerOutput用于电源控制 - 使用 PB7 (VBUS_EN)
    let power_output_instance = PowerOutput::new(vbus_en_pin);
    let power_output_static = POWER_OUTPUT.init(MaybeUninit::new(power_output_instance.clone()));
    let _power_output = unsafe { power_output_static.assume_init_mut() };

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
        #[allow(static_mut_refs)]
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

    // Get input event subscribers for both managers

    let power_input_subscriber = input_manager.subscriber();
    if let Err(e) = power_input_subscriber {
        defmt::panic!(
            "Failed to subscribe to input events for power manager: {}",
            e
        );
    }

    let vbus_input_subscriber = input_manager.subscriber();
    if let Err(e) = vbus_input_subscriber {
        defmt::panic!(
            "Failed to subscribe to input events for vbus manager: {}",
            e
        );
    }

    // 创建电源管理器上下文
    let power_ctx = PowerManagerContext {
        input_rx: Arc::new(Mutex::new(power_input_subscriber.unwrap())),
        power_switch: Arc::new(Mutex::new(vin_ce_pin)), // PA15 电源开关控制
        led_pwm: Arc::new(Mutex::new(pwm)),             // PA8 PWM LED控制
    };
    let mut power_manager = PowerManager::new(power_ctx);

    defmt::info!("Initializing power manager...");
    power_manager.init().await;
    defmt::info!("Power manager initialized successfully");

    // 创建 VBUS 管理器上下文
    let vbus_ctx = VbusManagerContext {
        input_rx: Arc::new(Mutex::new(vbus_input_subscriber.unwrap())),
        vbus_output: power_output_instance.clone(), // 使用现有的 PowerOutput
        vbus_led_pin: Arc::new(Mutex::new(vbus_led_pin)), // PB5 双色 LED 控制
    };
    let mut vbus_manager = VbusManager::new(vbus_ctx);

    defmt::info!("Initializing VBUS manager...");
    vbus_manager.init().await;
    defmt::info!("VBUS manager initialized successfully");

    // VBUS 管理器将在主循环中运行

    // 启动 VBUS ADC 监控任务
    spawner.spawn(vbus_adc_task()).unwrap();

    // 创建风扇管理器并启动任务
    let temperature_rx = shared::TEMPERATURE_CHANNEL.receiver().unwrap();
    let fan_manager = fan_manager::FanManager::new(fan_control_pin, temperature_rx);
    spawner.spawn(fan_task(fan_manager)).unwrap();
    defmt::info!("Fan management task started");

    // 运行系统状态机测试
    defmt::info!("Running system state machine tests...");
    let test_result = crate::tests::system_state_tests::run_all_tests();
    if !test_result {
        defmt::error!("Tests failed! System may have bugs.");
    }

    defmt::info!("Entering main loop");
    let mut counter = 0u32;

    // 获取电压和状态监听器
    let mut vbus_voltage_rx = shared::VBUS_VOLTAGE_CHANNEL.receiver().unwrap();
    let mut vin_voltage_rx = shared::VIN_VOLTAGE_CHANNEL.receiver().unwrap();
    let mut vbus_state_rx = shared::VBUS_STATE_CHANNEL.receiver().unwrap();

    // 保持最新的VBUS状态
    let mut current_vbus_enabled = false;

    loop {
        // 获取最新的电压和状态信息
        let vbus_voltage = vbus_voltage_rx.try_get().unwrap_or(0.0);
        let vin_voltage = vin_voltage_rx.try_get().unwrap_or(0.0);

        // 更新VBUS状态，只有在有新数据时才更新
        if let Some(new_vbus_enabled) = vbus_state_rx.try_get() {
            current_vbus_enabled = new_vbus_enabled;
        }

        // 更新VbusManager的电压信息
        vbus_manager.update_voltages(vbus_voltage, vin_voltage);

        // 执行VbusManager的tick
        vbus_manager.tick().await;

        // 更新PowerManager的电压信息（仅用于监控和LED显示）
        power_manager.update_voltages(vin_voltage, vbus_voltage, current_vbus_enabled);

        // 执行PowerManager的tick
        power_manager.tick().await;

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
async fn vbus_adc_task() {
    let mut adc_subscriber = ADC_PUBSUB.subscriber().unwrap();
    let vbus_voltage_sender = shared::VBUS_VOLTAGE_CHANNEL.sender();
    let vin_voltage_sender = shared::VIN_VOLTAGE_CHANNEL.sender();

    loop {
        let (vout_voltage, vin_voltage) = adc_subscriber.next_message_pure().await;

        // 发送 VBUS 电压到共享通道
        vbus_voltage_sender.send(vout_voltage);

        // 发送 VIN 电压到共享通道
        vin_voltage_sender.send(vin_voltage);

        // 记录电压状态变化
        if vout_voltage >= 5.5 {
            defmt::debug!(
                "VBUS voltage: {}V (HIGH), VIN voltage: {}V",
                vout_voltage,
                vin_voltage
            );
        } else {
            defmt::debug!(
                "VBUS voltage: {}V (LOW), VIN voltage: {}V",
                vout_voltage,
                vin_voltage
            );
        }
    }
}

#[embassy_executor::task]
async fn adc_task() {
    #[allow(static_mut_refs)]
    let adc_reader = unsafe { ADC_READER.assume_init_mut() };

    loop {
        if let Some(values) = adc_reader.poll().await {
            ADC_PUBSUB.publish_immediate((values.0, values.1));
            // 发布温度数据到温度通道
            shared::TEMPERATURE_CHANNEL.sender().send(values.2);
            // ADC日志已删除，避免刷屏
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

#[embassy_executor::task]
async fn fan_task(mut fan_manager: fan_manager::FanManager<'static>) {
    loop {
        fan_manager.tick().await;
        embassy_time::Timer::after_secs(5).await; // 5秒检查一次，与ADC采样同步
    }
}

use defmt_rtt as _;
use embassy_stm32::{
    adc::{Adc, AnyAdcChannel, SampleTime},
    peripherals::{self, ADC1},
    Peri,
};
use embassy_time::{Duration, Ticker};
use panic_probe as _;

use crate::shared::{ISN_MUL, VREF, VSN_MUL};

// ADC校准参数结构体
pub struct AdcCalibration {
    pub ts_cal1: f64,
    pub ts_cal2: f64,
    pub vrefint_cal: f64,
}

// ADC状态结构体
pub struct AdcReader<'a, const AVG_SIZE: usize> {
    adc: Adc<'a, peripherals::ADC1>,
    dma_ch: Peri<'a, peripherals::DMA1_CH1>,
    vsn_ch: AnyAdcChannel<ADC1>,
    isn_ch: AnyAdcChannel<ADC1>,
    v_temp_ch: AnyAdcChannel<ADC1>,
    v_ref_int_ch: AnyAdcChannel<ADC1>,
    buffer: [u16; 4],
    cal: AdcCalibration,
    ticker: Ticker,

    v_sn_prev: f64,
    i_sn_prev: f64,
}

impl<'a, const AVG_SIZE: usize> AdcReader<'a, AVG_SIZE> {
    pub async fn poll(&mut self) -> Option<(f64, f64, f64)> {
        self.ticker.next().await;

        // ADC读取
        self.adc
            .read(
                self.dma_ch.reborrow(),
                [
                    (&mut self.v_ref_int_ch, SampleTime::CYCLES640_5),
                    (&mut self.vsn_ch, SampleTime::CYCLES640_5),
                    (&mut self.v_temp_ch, SampleTime::CYCLES247_5),
                    (&mut self.isn_ch, SampleTime::CYCLES640_5),
                ]
                .into_iter(),
                &mut self.buffer,
            )
            .await;

        // 数据换算
        let adc_ref = self.buffer[0] as f64;
        let adc_vsn = self.buffer[1] as f64;
        let adc_temp = self.buffer[2] as f64;
        let adc_isn = self.buffer[3] as f64;

        let v_ref = VREF * self.cal.vrefint_cal / adc_ref;
        let v_sn = v_ref / 4095.0 * adc_vsn;
        let temperature = (130.0 - 30.0) / (self.cal.ts_cal2 - self.cal.ts_cal1)
            * ((adc_temp * (v_ref / VREF)) - self.cal.ts_cal1)
            + 30.0;
        let i_sn = v_ref / 4095.0 * adc_isn;

        let v_sn_avg = self.ema(self.v_sn_prev, v_sn, 0.1176);
        let i_sn_avg = self.ema(self.i_sn_prev, i_sn, 0.1176);

        self.v_sn_prev = v_sn_avg;
        self.i_sn_prev = i_sn_avg;

        let v_read = v_sn_avg * VSN_MUL;
        let i_read = i_sn_avg * ISN_MUL;
        Some((v_read, i_read, temperature))
    }

    #[inline(always)]
    fn ema(&self, old: f64, new: f64, alpha: f64) -> f64 {
        alpha * new + (1.0 - alpha) * old
    }

    pub fn new(
        adc: Adc<'a, peripherals::ADC1>,
        dma_ch: Peri<'a, peripherals::DMA1_CH1>,
        vsn_ch: AnyAdcChannel<ADC1>,
        isn_ch: AnyAdcChannel<ADC1>,
        v_temp_ch: AnyAdcChannel<ADC1>,
        v_ref_int_ch: AnyAdcChannel<ADC1>,
        cal: AdcCalibration,
    ) -> AdcReader<'a, AVG_SIZE> {
        Self {
            adc,
            dma_ch,
            vsn_ch,
            isn_ch,
            v_temp_ch,
            v_ref_int_ch,
            buffer: [0; 4],
            cal,
            ticker: Ticker::every(Duration::from_millis(6)),

            v_sn_prev: 0.0,
            i_sn_prev: 0.0,
        }
    }
}

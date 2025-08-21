use embassy_futures::join::join;
use embassy_stm32::{peripherals, usb};
use embassy_usb::driver::{Driver, Endpoint, EndpointIn, EndpointOut};
use embassy_usb::{
    class::web_usb::{self, Url, WebUsb},
    driver::EndpointError,
    Builder,
};

#[embassy_executor::task]
pub async fn usb_task(driver: usb::Driver<'static, peripherals::USB>) {
    let mut config = embassy_usb::Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Ivan");
    config.product = Some("PD Sink");
    config.serial_number = Some("20250502");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 64];
    let mut msos_descriptor = [0; 256];

    let webusb_config = web_usb::Config {
        max_packet_size: 64,
        vendor_code: 1,
        // If defined, shows a landing page which the device manufacturer would like the user to visit in order to control their device. Suggest the user to navigate to this URL when the device is connected.
        landing_url: Some(Url::new("http://localhost:8080")),
    };

    let mut web_usb_state = web_usb::State::new();

    let mut builder = Builder::new(
        driver,
        config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut msos_descriptor,
        &mut control_buf,
    );

    // Create classes on the builder (WebUSB just needs some setup, but doesn't return anything)
    WebUsb::configure(&mut builder, &mut web_usb_state, &webusb_config);
    // Create some USB bulk endpoints for testing.
    let mut endpoints = WebEndpoints::new(&mut builder, &webusb_config);

    let mut usb = builder.build();

    let usb_fut = usb.run();

    let echo_fut = async {
        loop {
            endpoints.wait_connected().await;
            defmt::info!("Connected");
            endpoints.echo().await;
            defmt::info!("Disconnected");
        }
    };

    join(usb_fut, echo_fut).await;
}
struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

#[allow(dead_code)]
struct WebEndpoints<'d, D: Driver<'d>> {
    write_ep: D::EndpointIn,
    read_ep: D::EndpointOut,
}

#[allow(dead_code)]
impl<'d, D: Driver<'d>> WebEndpoints<'d, D> {
    fn new(builder: &mut Builder<'d, D>, config: &'d web_usb::Config<'d>) -> Self {
        let mut func = builder.function(0xff, 0x00, 0x00);
        let mut iface = func.interface();
        let mut alt = iface.alt_setting(0xff, 0x00, 0x00, None);

        let write_ep = alt.endpoint_bulk_in(None, config.max_packet_size);
        let read_ep = alt.endpoint_bulk_out(None, config.max_packet_size);

        WebEndpoints { write_ep, read_ep }
    }

    // Wait until the device's endpoints are enabled.
    async fn wait_connected(&mut self) {
        self.read_ep.wait_enabled().await
    }

    // Echo data back to the host.
    async fn echo(&mut self) {
        let mut buf = [0; 64];
        loop {
            let n = self.read_ep.read(&mut buf).await.unwrap();
            let data = &buf[..n];
            defmt::info!("Data read: {:x}", data);
            self.write_ep.write(data).await.unwrap();
        }
    }
}

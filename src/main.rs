#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::Timer;
use embassy_rp::{
    bind_interrupts, gpio::{Level, Output, Pull}, peripherals::USB, pwm::{self, Config as PwmConfig}, usb::{Driver, InterruptHandler}
};
use embassy_usb::{Builder, Config as UsbConfig, UsbDevice, class::cdc_acm::{State}};
use embassy_usb::class::midi::MidiClass;

use static_cell::StaticCell;

use panic_probe as _;
use defmt_rtt as _;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let driver = Driver::new(p.USB, Irqs);
    let mut config = UsbConfig::new(0x1209, 0x0001);
    config.product = Some("Pico USB MIDI Clock");

    let mut builder = {
        static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

        let builder = embassy_usb::Builder::new(
            driver,
            config,
            CONFIG_DESCRIPTOR.init([0; 256]),
            BOS_DESCRIPTOR.init([0; 256]),
            &mut [], // no msos descriptors
            CONTROL_BUF.init([0; 64]),
        );
        builder
    };

    //static STATE: StaticCell<State> = StaticCell::new();
    //let mut class = CdcAcmClass::new(&mut builder, STATE.init(State::new()), 64);
    let mut midi = MidiClass::new(&mut builder, 1, 1, 64);

    let usb = builder.build();
    spawner.spawn(usb_task(usb)).unwrap();
    let mut pulse = Output::new(p.PIN_15, Level::Low);
    let mut led = Output::new(p.PIN_14, Level::Low);
    let mut tick_count: u32 = 0;

    loop { 
        // class.write_packet(b"hello\n").await.ok();
        let mut buf = [0u8; 64];
        midi.wait_connection().await;
        if let Ok(n) = midi.read_packet(&mut buf).await {
            for chunk in buf[..n].chunks(4) {
                if chunk.len() >= 2 && chunk[1] == 0xF8 {
                    tick_count += 1;
                    // 6 ticks = sixteenth note
                    if tick_count % 6 == 0 {
                        pulse.set_low();
                        led.set_high();
                        Timer::after_millis(5).await;
                        pulse.set_high();
                        led.set_low();
                    }
                }
            }
        }
    }
}

#[embassy_executor::task]
async fn usb_task(mut usb: UsbDevice<'static, Driver<'static, USB>>) {
    usb.run().await;
}

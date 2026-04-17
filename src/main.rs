#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::Timer;
use embassy_rp::{
    bind_interrupts, gpio::{Level, Output}, peripherals::USB, pwm::{Pwm, Config as PwmConfig}, usb::{Driver, InterruptHandler}
};
use embassy_usb::{Config as UsbConfig, UsbDevice};
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
    let mut usb_config = UsbConfig::new(0x1209, 0x0001);
    usb_config.product = Some("Pico USB MIDI Clock");
    let mut pwm_config: PwmConfig = Default::default();
    pwm_config.top = 2082;
    pwm_config.divider = 1u8.into();
    let mut pwm_out = Pwm::new_output_b(p.PWM_SLICE2, p.PIN_5, pwm_config.clone());

    let mut builder = {
        static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

        let builder = embassy_usb::Builder::new(
            driver,
            usb_config,
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
    let mut note_led = Output::new(p.PIN_13, Level::Low);
    let mut tick_count: u32 = 0;

    loop { 
        // class.write_packet(b"hello\n").await.ok();
        let mut buf = [0u8; 64];
        midi.wait_connection().await;
        if let Ok(n) = midi.read_packet(&mut buf).await {
            for chunk in buf[..n].chunks(4) {
                defmt::info!("MIDI: {:02x} {:02x} {:02x} {:02x}", chunk[0], chunk[1], chunk[2], chunk[3]);
                if chunk.len() >= 2 {
                    match chunk[1] & 0xF0 {
                        // clock
                        0xF0 => {
                            if chunk[1] == 0xF8 {
                                tick_count += 1;
                                // 6 ticks = sixteenth note
                                if tick_count % 24 == 0 {
                                    pulse.set_low();
                                    led.set_high();
                                    Timer::after_millis(5).await;
                                    pulse.set_high();
                                    led.set_low();
                                }
                            }
                        },
                        // note on
                        0x90 => {
                            match chunk {
                                [_, _, note, velocity] => {
                                    defmt::info!("Note ON: note={} vel={} duty={}", 
                note, velocity, (*note as u32 * 52));
                                    if *velocity == 0 {
                                        note_led.set_low();
                                    } else {
                                        note_led.set_high();
                                        let duty = ((*note as u32).saturating_sub(36) * 52) as u16;
                                        pwm_config.compare_b = duty;
                                        pwm_out.set_config(&pwm_config);
                                    }
                                },
                                _ => {}
                            }
                        },
                        // note off
                        0x80 => {
                            note_led.set_low();
                        },
                        _ => {}
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

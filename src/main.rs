#![no_main]
#![no_std]
#![allow(deprecated)] // thanks embedded_hal::digital::v2

use rtfm::app;
use core::fmt::Write;
use cortex_m::asm::delay;
use panic_semihosting as _;
use heapless::{String, consts::*};
use stm32f1xx_hal::{
    prelude::*,
    pac,
    spi,
    gpio::{Input, Output, PushPull, Floating, Alternate, gpioa::*, gpioc::*},
};

use usb_device::{prelude::*, bus};
use stm32_usbd::UsbBusType;
use usbd_serial::SerialPort;

mod keyboard;

const MS: u32 = 48_000_000 / 1000;

#[allow(unused)]
#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {
        cortex_m::interrupt::free(|_| {
            let itm = unsafe { &mut *cortex_m::peripheral::ITM::ptr() };
            cortex_m::iprintln!(&mut itm.stim[0], $($arg)*);
        });
    }
}

#[app(device = stm32f1xx_hal::stm32)]
const APP: () = {
    static mut USB_DEV: UsbDevice<'static, UsbBusType> = ();
    static mut SERIAL: SerialPort<'static, UsbBusType> = ();
    static mut KEYBOARD: keyboard::Keyboard<'static, UsbBusType> = ();
    static mut LED: PC13<Output<PushPull>> = ();
    static mut RFID: mfrc522::Mfrc522<
        mfrc522::interface::SpiInterface<
            spi::Spi<
                pac::SPI1,
                (PA5<Alternate<PushPull>>, PA6<Input<Floating>>, PA7<Alternate<PushPull>>)>,
            PA4<Output<PushPull>>>> = ();

    static mut COUNT: u32 = 0;

    #[init(spawn = [poll_rfid])]
    fn init(c: init::Context) -> init::LateResources {
        static mut USB_BUS: Option<bus::UsbBusAllocator<UsbBusType>> = None;

        let mut flash = c.device.FLASH.constrain();
        let mut rcc = c.device.RCC.constrain();

        let clocks = rcc.cfgr
            .use_hse(8.mhz())
            .sysclk(48.mhz())
            .pclk1(24.mhz())
            .freeze(&mut flash.acr);

        assert!(clocks.usbclk_valid());

        let mut afio = c.device.AFIO.constrain(&mut rcc.apb2);
        let mut gpioa = c.device.GPIOA.split(&mut rcc.apb2);
        let mut gpiob = c.device.GPIOB.split(&mut rcc.apb2);
        let mut gpioc = c.device.GPIOC.split(&mut rcc.apb2);

        let led = gpioc.pc13.into_push_pull_output(&mut gpioc.crh);
        let spi_sck_pin = gpioa.pa5.into_alternate_push_pull(&mut gpioa.crl);
        let spi_miso_pin = gpioa.pa6;
        let spi_mosi_pin = gpioa.pa7.into_alternate_push_pull(&mut gpioa.crl);
        let mut mfrc522_reset_pin = gpiob.pb11.into_push_pull_output(&mut gpiob.crh);
        let mut mfrc522_nss_pin = gpioa.pa4.into_push_pull_output(&mut gpioa.crl);

        mfrc522_nss_pin.set_high();

        mfrc522_reset_pin.set_low();
        delay(1 * MS);
        mfrc522_reset_pin.set_high();
        delay(1 * MS);

        let spi = spi::Spi::spi1(
            c.device.SPI1,
            (spi_sck_pin, spi_miso_pin, spi_mosi_pin),
            &mut afio.mapr,
            embedded_hal::spi::MODE_0,
            400.khz(),
            clocks,
            &mut rcc.apb2);

        let mut usb_dp = gpioa.pa12.into_push_pull_output(&mut gpioa.crh);
        usb_dp.set_low();
        delay(100 * MS);

        let usb_dm = gpioa.pa11;
        let usb_dp = usb_dp.into_floating_input(&mut gpioa.crh);

        let usb_bus = {
            *USB_BUS = Some(UsbBusType::new(c.device.USB, (usb_dm, usb_dp)));
            USB_BUS.as_ref().unwrap()
        };

        let rfid = mfrc522::Mfrc522::new_spi(spi, mfrc522_nss_pin).unwrap();

        let serial = SerialPort::new(usb_bus);

        let keyboard = keyboard::Keyboard::new(usb_bus);

        let usb_dev = UsbDeviceBuilder::new(usb_bus, UsbVidPid(0x5824, 0x27dd))
            .manufacturer("virkkunen.net")
            .product("RFID reader")
            .serial_number("virkkunen.net RFID reader")
            .build();

        c.spawn.poll_rfid().unwrap();

        init::LateResources {
            LED: led,
            USB_DEV: usb_dev,
            SERIAL: serial,
            KEYBOARD: keyboard,
            RFID: rfid,
        }
    }

    #[task(resources = [RFID, LED, SERIAL, KEYBOARD], schedule = [poll_rfid], priority = 1)]
    fn poll_rfid(c: poll_rfid::Context) {
        static mut PREV_UID: [u8; 10] = [0; 10];
        static mut PREV_COUNT: usize = 0;

        let mut serial = c.resources.SERIAL;
        let mut keyboard = c.resources.KEYBOARD;

        c.resources.LED.set_low();

        if let Ok(atqa) = c.resources.RFID.reqa() {
            if let Ok(picc) = c.resources.RFID.select(&atqa) {
                let uid = picc.uid().bytes();

                let mut padded = [0u8; 10];
                padded[..uid.len()].copy_from_slice(uid);

                if !(padded == *PREV_UID && *PREV_COUNT > 0) {
                    let mut hex_uid = String::<U11>::new();
                    for &b in uid {
                        write!(hex_uid, "{:02x}", b).ok();
                    }

                    let mut msg = String::<U64>::new();
                    write!(msg, "UID {}\r\n", hex_uid).ok();

                    serial.lock(|serial| serial.write(msg.as_ref()).ok());

                    hex_uid.push(' ').ok();

                    keyboard.lock(|kbd| kbd.type_text(hex_uid.as_ref()).ok());
                }

                *PREV_UID = padded;
                *PREV_COUNT = 5;
            }
        } else if *PREV_COUNT > 0 {
            *PREV_COUNT -= 1;

            if *PREV_COUNT == 0 {
                serial.lock(|serial| serial.write("GONE\r\n".as_bytes()).ok());
            }
        }

        c.resources.LED.set_high();

        c.schedule.poll_rfid(c.scheduled + (100 * MS).cycles()).unwrap();
    }

    #[interrupt(resources = [USB_DEV, SERIAL, KEYBOARD, COUNT], priority = 2)]
    fn USB_LP_CAN_RX0(c: USB_LP_CAN_RX0::Context) {
        *c.resources.COUNT += 1;

        let usb_dev = c.resources.USB_DEV;
        let mut serial = c.resources.SERIAL;
        let mut keyboard = c.resources.KEYBOARD;

        if usb_dev.poll(&mut [&mut *serial, &mut *keyboard]) {
            let mut buf = [0u8; 64];

            match serial.read(&mut buf) {
                Ok(_count) => {},
                Err(_) => {},
            }
        }
    }

    extern "C" {
        fn USART1();
    }
};

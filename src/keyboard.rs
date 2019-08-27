use usb_device::class_prelude::*;
use heapless::consts::*;
use heapless::spsc::Queue;

const USB_CLASS_HID: u8 = 0x03;
const USB_DESCRIPTOR_HID: u8 = 0x21;
const USB_DESCRIPTOR_REPORT: u8 = 0x22;
//const REQUEST_GET_REPORT: u8 = 0x01;
//const REQUEST_SET_REPORT: u8 = 0x09;

#[derive(Debug)]
pub enum Error {
    UnknownCharacter,
    Overflow,
    Usb(UsbError),
}

pub struct Keyboard<'a, B: UsbBus> {
    iface: InterfaceNumber,
    in_ep: EndpointIn<'a, B>,
    queue: Queue<(u8, u8), U64>,
    release: bool,
}

impl<B: UsbBus> Keyboard<'_, B> {
    pub fn new<'a>(alloc: &'a UsbBusAllocator<B>) -> Keyboard<'a, B> {
        Keyboard {
            iface: alloc.interface(),
            in_ep: alloc.interrupt(8, 10),
            queue: Queue::new(),
            release: false,
        }
    }

    pub fn type_text(&mut self, text: &str) -> Result<(), Error> {
        for ch in text.bytes() {
            let keycode = KEYCODES.iter().find(|c| c.0 == ch).ok_or(Error::UnknownCharacter)?;

            self.queue.enqueue((keycode.1, keycode.2)).map_err(|_| Error::Overflow)?;
        }

        self.write_report()?;

        Ok(())
    }

    fn write_report(&mut self) -> Result<(), Error> {
        let mut report = [0u8; 8];

        if self.release {
            self.release = false;
        } else if let Some((modifiers, keycode)) = self.queue.dequeue() {
            report[0] = modifiers;
            report[2] = keycode;
            self.release = true;
        } else {
            return Ok(());
        }

        match self.in_ep.write(&report[..]) {
            Ok(_) | Err(UsbError::WouldBlock) => Ok(()),
            Err(e) => Err(Error::Usb(e)),
        }
    }
}

impl<B: UsbBus> UsbClass<B> for Keyboard<'_, B> {
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()> {
        writer.interface(
            self.iface,
            USB_CLASS_HID,
            0x00,
            0x00)?;

        let rd_len = REPORT_DESCRIPTOR.len();

        writer.write(
            USB_DESCRIPTOR_HID,
            &[
                0x01, 0x01, // bcdHID
                0x00, // bCountryCode
                0x01, // bNumDescriptors
                USB_DESCRIPTOR_REPORT, // bDescriptorType
                rd_len as u8, (rd_len >> 8) as u8, // wDescriptorLength
            ])?;

        writer.endpoint(&self.in_ep)?;

        Ok(())
    }

    fn control_in(&mut self, xfer: ControlIn<B>) {
        let req = *xfer.request();

        if req.recipient == control::Recipient::Interface
            && req.index == u8::from(self.iface) as u16
            && req.request_type == control::RequestType::Standard
            && req.request == control::Request::GET_DESCRIPTOR
            && req.descriptor_type_index() == (USB_DESCRIPTOR_REPORT, 0)
        {
            xfer.accept_with_static(REPORT_DESCRIPTOR).ok();
        }
    }


    fn endpoint_in_complete(&mut self, ep: EndpointAddress) {
        if ep == self.in_ep.address() {
            self.write_report().ok();
        }
    }
}

const REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01,        // Usage Page (Generic Desktop Ctrls)
    0x09, 0x06,        // Usage (Keyboard)
    0xA1, 0x01,        // Collection (Application)
    0x05, 0x07,        //   Usage Page (Kbrd/Keypad)
    0x19, 0xE0,        //   Usage Minimum (0xE0)
    0x29, 0xE7,        //   Usage Maximum (0xE7)
    0x15, 0x00,        //   Logical Minimum (0)
    0x25, 0x01,        //   Logical Maximum (1)
//    0x85, 0x01,        //   Report ID (1)
    0x75, 0x01,        //   Report Size (1)
    0x95, 0x08,        //   Report Count (8)
    0x81, 0x02,        //   Input (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position)
    0x95, 0x01,        //   Report Count (1)
    0x75, 0x08,        //   Report Size (8)
    0x81, 0x01,        //   Input (Const,Array,Abs,No Wrap,Linear,Preferred State,No Null Position)
    0x95, 0x05,        //   Report Count (5)
    0x75, 0x01,        //   Report Size (1)
    0x05, 0x08,        //   Usage Page (LEDs)
    0x19, 0x01,        //   Usage Minimum (Num Lock)
    0x29, 0x05,        //   Usage Maximum (Kana)
    0x91, 0x02,        //   Output (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x95, 0x01,        //   Report Count (1)
    0x75, 0x03,        //   Report Size (3)
    0x91, 0x01,        //   Output (Const,Array,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x95, 0x06,        //   Report Count (6)
    0x75, 0x08,        //   Report Size (8)
    0x15, 0x00,        //   Logical Minimum (0)
    0x26, 0xFF, 0x00,  //   Logical Maximum (255)
    0x05, 0x07,        //   Usage Page (Kbrd/Keypad)
    0x19, 0x00,        //   Usage Minimum (0x00)
    0x2A, 0xFF, 0x00,  //   Usage Maximum (0xFF)
    0x81, 0x00,        //   Input (Data,Array,Abs,No Wrap,Linear,Preferred State,No Null Position)
    0xC0,              // End Collection
];

// Only has the keys this thing actually needs, and only for a QWERTY layout.
const KEYCODES: &[(u8, u8, u8)] = &[
    (b'a', 0x00, 0x04),
    (b'b', 0x00, 0x05),
    (b'c', 0x00, 0x06),
    (b'd', 0x00, 0x07),
    (b'e', 0x00, 0x08),
    (b'f', 0x00, 0x09),
    (b'g', 0x00, 0x0a),
    (b'h', 0x00, 0x0b),
    (b'i', 0x00, 0x0c),
    (b'j', 0x00, 0x0d),
    (b'k', 0x00, 0x0e),
    (b'l', 0x00, 0x0f),
    (b'm', 0x00, 0x10),
    (b'n', 0x00, 0x11),
    (b'o', 0x00, 0x12),
    (b'p', 0x00, 0x13),
    (b'q', 0x00, 0x14),
    (b'r', 0x00, 0x15),
    (b's', 0x00, 0x16),
    (b't', 0x00, 0x17),
    (b'u', 0x00, 0x18),
    (b'v', 0x00, 0x19),
    (b'w', 0x00, 0x1a),
    (b'x', 0x00, 0x1b),
    (b'y', 0x00, 0x1c),
    (b'z', 0x00, 0x1d),
    (b'A', 0x02, 0x04),
    (b'B', 0x02, 0x05),
    (b'C', 0x02, 0x06),
    (b'D', 0x02, 0x07),
    (b'E', 0x02, 0x08),
    (b'F', 0x02, 0x09),
    (b'G', 0x02, 0x0a),
    (b'H', 0x02, 0x0b),
    (b'I', 0x02, 0x0c),
    (b'J', 0x02, 0x0d),
    (b'K', 0x02, 0x0e),
    (b'L', 0x02, 0x0f),
    (b'M', 0x02, 0x10),
    (b'N', 0x02, 0x11),
    (b'O', 0x02, 0x12),
    (b'P', 0x02, 0x13),
    (b'Q', 0x02, 0x14),
    (b'R', 0x02, 0x15),
    (b'S', 0x02, 0x16),
    (b'T', 0x02, 0x17),
    (b'U', 0x02, 0x18),
    (b'V', 0x02, 0x19),
    (b'W', 0x02, 0x1a),
    (b'X', 0x02, 0x1b),
    (b'Y', 0x02, 0x1c),
    (b'Z', 0x02, 0x1d),
    (b'1', 0x00, 0x1e),
    (b'2', 0x00, 0x1f),
    (b'3', 0x00, 0x20),
    (b'4', 0x00, 0x21),
    (b'5', 0x00, 0x22),
    (b'6', 0x00, 0x23),
    (b'7', 0x00, 0x24),
    (b'8', 0x00, 0x25),
    (b'9', 0x00, 0x26),
    (b'0', 0x00, 0x27),
    (b'\n', 0x00, 0x28),
    (b'\x1b', 0x00, 0x29),
    (b'\x08', 0x00, 0x2a),
    (b'\t', 0x00, 0x2b),
    (b' ', 0x00, 0x2c),
    (b'-', 0x00, 0x2d),
    (b',', 0x00, 0x36),
    (b'.', 0x00, 0x37),
];
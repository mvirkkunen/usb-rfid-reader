usb-rfid-reader
===============

Simple RFID reader with USB serial port and HID keyboard emulation. Uses STM32F103 and MFRC522.

Pin connections
---------------

The MFRC522 is hooked up to SPI1.

    STM32  MFRC522

    GND    GND
    VCC    VCC
    PA5    SCK
    PA6    MISO
    PA7    MOSI
    PB11   RST
    PA4    NSS

set -e

openocd_interface=stlink-v2
openocd_target=stm32f1x
package=usb-rfid-reader
binary="target/thumbv7m-none-eabi/release/$package"

cargo build --release

openocd -f interface/$openocd_interface.cfg -f target/$openocd_target.cfg -c "program $binary verify reset exit"

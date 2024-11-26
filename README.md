# hid-feature

hid-feature is a commandline tool to show and change Feature Reports on a HID device.

This is a Rust reimplementation of hid-feature from
[hid-tools](https://gitlab.freedesktop.org/libevdev/hid-tools/)

This tool needs read and write access to the `/dev/hidraw` node, typically this means
running it as root.

## Usage

Find the device's hidraw node with `list-devices`:

```
$ hid-feature list-devices
Available HID devices:
/dev/hidraw5  - Logitech USB Receiver
/dev/hidraw0  - Yubico YubiKey OTP+FIDO+CCID
/dev/hidraw1  - Yubico YubiKey OTP+FIDO+CCID
/dev/hidraw2  - Microsoft Microsoft Optical Mouse with Tilt Wheel
```

Then look at the device's HID Features and their current values:
```
$ hid-feature list /dev/hidraw2
Report ┃                      Usage                       ┃ Bits ┃ Bit Range ┃ Value Range ┃ Count ┃ Value ┃ Bytes
━━━━━━━╇━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━╇━━━━━━╇━━━━━━━━━━━╇━━━━━━━━━━━━━╇━━━━━━━╇━━━━━━━╇━━━━━━
  23   │ Vendor Defined Page 0xFF00 / Vendor Usage 0xff06 │  2   │   8..=9   │    0..=1    │   1   │     1 │ 01
  23   │ Vendor Defined Page 0xFF00 / Vendor Usage 0xff04 │  1   │  12..=12  │    0..=1    │   1   │     0 │ 01
  24   │ Vendor Defined Page 0xFF00 / Vendor Usage 0xff08 │  1   │   8..=8   │    0..=1    │   1   │    -1 │ 01
  18   │ Generic Desktop / Resolution Multiplier          │  2   │   8..=9   │    0..=1    │   1   │     1 │ 01
```
In this example we can see that Feature Report 18 has a 2-bit value at bits 8
and 9 that is the Resolution Multiplier (used for high-resolution wheel
scrolling). It is set to the Logical value 1.

Let's set it to 0 to get clunky scrolling on this device!

To set a given byte in a feature report, set the hexadecimal value or use `xx` to leave the setting as-is.
```
$ hid-feature set /dev/hidraw2 --report-id=18 xx 00
```
In this example, the second byte of the report with ID 18 is set to the value
`0x01`, all other values are left as-is.

Note that where a byte comprises of multiple different usages, it is the caller's responsibility to
compose the byte to the correct value. For example in the Feature Report 23 we can see two vendor-defined
usages at bits 8+9 and bit 12, respectively.

```
# Set bits 8/9 and bit 12 to 1
$ hid-feature set /dev/hidraw2 --report-id=23 xx 11
# Set only bits 8/9 but not bit 12
$ hid-feature set /dev/hidraw2 --report-id=23 xx 01
# Set only bit 12 but not bits 8/9
$ hid-feature set /dev/hidraw2 --report-id=23 xx 10
```
To make this easier, the value of the byte(s) the field occupies is printed as
hexadecimal value in the `list` output under the `Bytes` heading (`01`)

For example:
```
  24   │ Vendor Defined Page 0xFF00 / Vendor Usage 0xff08 │  16  │   8..=23   │    0..=65535    │   1   │  43828 │ ab 34
```

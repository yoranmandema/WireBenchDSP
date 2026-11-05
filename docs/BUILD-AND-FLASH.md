# Building, flashing and debugging

## Toolchain (already installed on this machine)

| Tool | Version | Purpose |
|---|---|---|
| rustc / cargo | 1.99.1 | Host + cross compilation |
| `thumbv7em-none-eabihf` target | — | Cortex-M7 firmware |
| probe-rs | 0.32.0 | Flash + debug (ST-Link / J-Link / DAPLink) |
| arm-none-eabi-gcc | 16.2.0 | objdump / size / gdb |
| openocd | — | GDB server, non-probe-rs targets |
| espflash / esptool.py | — | ESP32 |

Install from scratch:

```bash
rustup target add thumbv7em-none-eabihf thumbv7em-none-eabi thumbv6m-none-eabi
brew install probe-rs-tools arm-none-eabi-gcc open-ocd espflash esptool
```

## Build

```bash
cd firmware
cargo build                    # debug (fast to build, large)
cargo build --release          # LTO, ~12 KB text
```

Expected size (debug): `text 58 KB, data 0, bss 178 KB`.
Expected size (release): `text 12 KB, data 0, bss 178 KB`.

## Flash

```bash
# probe-rs (recommended)
probe-rs run --chip STM32H723VG
probe-rs run --chip STM32H723VG --release

# or via cargo
cargo run --release            # runner is configured in .cargo/config.toml
```

The WeAct board has **no onboard debugger** — wire an ST-Link V2 (or any
SWD probe) to the 4-pin SWD header:

| SWD header | Probe |
|---|---|
| SWDIO | SWDIO |
| SWCLK | SWCLK |
| GND | GND |
| 3V3 | 3V3 (optional, for target power sensing) |

## Debug

### defmt over RTT (recommended)

Add to `firmware/Cargo.toml`:

```toml
defmt = "0.3"
defmt-rtt = "0.4"
```

In `main.rs`:

```rust
use defmt_rtt as _;            // global logger
use cortex_m as _;             // critical-section impl (see below)
```

Add a timestamp provider (defmt links against `_defmt_timestamp`):

```rust
#[defmt::timestamp]
fn timestamp() -> u64 { 0 }    // or a DWT cycle count
```

`defmt-rtt` needs a `critical-section` implementation. Enable the
single-core one on cortex-m:

```toml
cortex-m = { version = "0.7", features = ["critical-section-single-core"] }
```

Then `probe-rs run --chip STM32H723VG` and the defmt output appears on the
RTT channel.

### GDB / OpenOCD

```bash
openocd -f interface/stlink.cfg -f target/stm32h7x.cfg
# in another shell:
arm-none-eabi-gdb target/thumbv7em-none-eabihf/debug/pedal-firmware
  (gdb) target remote :3333
  (gdb) monitor reset halt
  (gdb) load
  (gdb) break main
  (gdb) continue
```

### Logic analyser

For I2S debugging, a $10 USB logic analyser with a PWM/I2S decoder (Saleae
Logic, PulseView) answers in seconds:

* Is MCLK 12.288 MHz?
* Is BICK toggling at 3.072 MHz (64 × 48 kHz)?
* Is LRCK toggling at 48 kHz?
* Is there data on SD_A / SD_B?

## Common failures

| Symptom | Cause |
|---|---|
| `undefined symbol: _defmt_timestamp` | defmt needs a timestamp provider. |
| `no method named floor` on thumb only | `core` has no float methods — use `libm`. |
| `cannot move out of ccdr.peripheral` | `PeripheralREC` consumes its handle; own `ccdr`. |
| `cannot find value DMA1_STR1 in module interrupt` | import `hal::pac::interrupt`, not `cortex_m_rt::interrupt`. |
| `#[panic_handler] required` | add a handler or `panic-halt` / `panic-probe`. |
| Probe-rs: "No debug probes found" | Check the USB cable and the SWD wiring. |
| Probe-rs: "chip not found" | Use the exact name from `probe-rs chip list`. |

## ESP32

```bash
espflash flash target/.../firmware        # or
esptool.py --chip esp32s3 --port /dev/tty.usbserial-* write_flash 0x0 firmware.bin
```

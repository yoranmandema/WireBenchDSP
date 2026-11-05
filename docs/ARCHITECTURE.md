# Pedal DSP — architecture & reference design

A from-scratch, Rust, STM32H723-based guitar pedal: SAI/I2S audio in and out,
a DMA-driven stereo effect chain, and a analogue control surface.

```
firmware/          STM32H723 firmware (no_std)
dsp/               pedal-dsp — host-tested, no_std DSP library
docs/              this document + wiring + analogue design
```

---

## 1. Platform decision

| Option | Verdict |
|---|---|
| **STM32H723VG** (chosen) | 550 MHz Cortex-M7, DP-FPU, 1 MB flash, 564 KB RAM, SAI1/SAI4, ~$5–8 in LQFP100. Best performance-per-dollar for audio DSP. |
| STM32F405/F407 | 168 MHz, proven in commercial pedals, cheapest ecosystem. A fine fallback; the DSP library is portable. |
| STM32G4 | Has the FMAC/CORDIC hardware math accelerators — interesting for filters, but less RAM and a smaller audio ecosystem. |
| ESP32-S3 | Cheap and has PSRAM, but the Rust audio story is weaker and it has no true I2S DAC/ADC with the same fidelity. Better for Wi-Fi/BLE pedals. |

The Black Pill (STM32F411/F401) stays a supported *development* target; it is
not the performance target.

### Why an external I2S codec

The H723 has no analogue DAC and its ADC is not audio-grade. A stereo codec
gives proper line/instrument I/O, a headphone driver, and a clean I2S link.

**Primary: SGTL5000** (I2C control, line in/out, headphone out, 95 dB SNR DAC).
Alternatives: WM8731 (cheaper modules), AK4558, CS4272 (higher end).

---

## 2. Signal chain

```
 guitar ──► [JFET buffer] ──► [anti-alias LPF] ──► SGTL5000 ADC ──► I2S ──► STM32H723
                                                                              │
                                                                            DSP chain
                                                                              │
 STM32H723 ◄── I2S ◄── SGTL5000 DAC ◄── [reconstruction LPF] ◄── [output buffer] ◄──┘
                                                                              │
                                                                       amp / PA / headphones
```

### Digital chain (in `firmware/src/effects.rs`)

```
in ─► DC block ─► distortion (4× oversampled) ─► tone stack ─► delay ─► reverb ─► DC block ─► out
```

* **DC block** — 20 Hz high-pass, removes offset so the distortion stage and
  the codec never see a DC component.
* **Distortion** — tanh / soft-clip / hard-clip / tube / fuzz / rectify, run
  through a 4× oversampling stage (two cascaded half-band FIRs) to push
  aliasing out of the audio band. Measured: the aliased 3rd harmonic of a
  hard-clipped 9 kHz tone drops by ~78×.
* **Tone stack** — a 200 Hz low shelf and a 4 kHz high shelf (RBJ biquads),
  ±12 dB.
* **Delay** — stereo feedback delay, up to 100 ms, with fractional reads.
* **Reverb** — Freeverb (8 combs + 4 allpasses), 132 KB of buffers.

---

## 3. Hardware: WeAct Studio MiniSTM32H723

STM32H723VGT6, LQFP100, 25 MHz HSE, 550 MHz SYSCLK (VOS0), HCLK 275 MHz.

### SAI1 pin map

| SAI signal | MCU pin | WeAct header | Notes |
|---|---|---|---|
| SAI1_MCLK_A | **PE2** | P2 pin 3 | Also QSPI_BK1_IO2 (flash unused) |
| SAI1_SCK_A | **PE5** | — | Routed to the DVP/DCMI connector; solder a wire |
| SAI1_FS_A | **PE4** | P2 pin 5 | |
| SAI1_SD_A (TX) | **PE6** | P2 pin 7 | MCU → codec data |
| SAI1_SD_B (RX) | **PE3** | — | Codec → MCU; shared with the onboard LED |

> **PE5 caveat:** on the LQFP100 package PE5 is the *only* SAI1 SCK_A pin
> (PF8 is not bonded out), and WeAct routes it to the camera connector rather
> than P1/P2. For a clean prototype, either solder a wire to the DCMI pin, or
> use a **NUCLEO-H723ZG** (LQFP144) where the SAI pins reach the Arduino /
> Morpho headers.

### Codec wiring (SGTL5000 module)

| SGTL5000 | STM32H723 | Notes |
|---|---|---|
| MCLK | PE2 | 12.288 MHz (256 × 48 kHz) |
| BICK | PE5 | Bit clock from the SAI |
| LRCK | PE4 | Frame sync (48 kHz) |
| SDTI (in) | PE6 | MCU → codec (SAI1_SD_A) |
| SDTO (out) | PE3 | Codec → MCU (SAI1_SD_B) |
| SCL | PB8 | I2C1, 4.7 kΩ pull-up to 3V3 |
| SDA | PB9 | I2C1, 4.7 kΩ pull-up to 3V3 |
| VDDIO | 3V3 | Digital supply |
| VDDA | 1V8 | Analogue supply (module usually has an LDO) |
| LINEIN L/R | input jack | Via the input buffer |
| LINEOUT L/R | output jack | Via the reconstruction filter |
| GND | common ground | Star ground at the codec |

### Control surface

| Control | MCU pin | Notes |
|---|---|---|
| Pot 1 (drive) | PA0 (ADC1 IN1) | 10 kΩ pot between 3V3 and GND, wiper to PA0 |
| Pot 2 (tone) | PA1 (ADC1 IN2) | |
| Expression pedal | PA2 (ADC1 IN3) | TRS cable tip to PA2, sleeve to GND |
| Footswitch | PC13 | Active low, internal pull-up enabled |

---

## 4. Clock tree

```
HSE 25 MHz ──/2──► 12.5 MHz ──×44──► 550 MHz VCO ──/1──► SYSCLK 550 MHz
                                                    └─/2──► HCLK 275 MHz
PLL3_P = 48 000 × 257 = 12.336 MHz ──► SAI1 kernel clock
        (SAI MCLK = 12.288 MHz; the 257 trick avoids an integer boundary)
```

VOS0 (`pwr.vos0(&SYSCFG)`) is mandatory at 550 MHz.

---

## 5. Memory map

| Region | Start | Size | Use |
|---|---|---|---|
| ITCM | 0x0000_0000 | 64 KB | Instruction TCM (free) |
| DTCM | 0x2000_0000 | 128 KB | Data TCM (free — good for the stack) |
| AXI SRAM (D1) | 0x2400_0000 | 320 KB | `.data` / `.bss`, DMA buffers |
| AHB SRAM (D2) | 0x3000_0000 | 32 KB | SRAM1+SRAM2 |
| D3 SRAM | 0x3800_0000 | 16 KB | |
| Flash | 0x0800_0000 | 1 MB | Firmware |

Current debug build: **58 KB text, 177 KB data, 1 KB bss** — comfortable.

---

## 6. Audio pipeline

* SAI1 full-duplex, I2S Philips, 24-bit data in 32-bit words, 48 kHz.
* DMA1 Stream 0: memory → SAI1 TX (channel B), circular.
* DMA1 Stream 1: SAI1 RX (channel A) → memory, circular, HT + TC interrupts.
* 32 stereo frames per half-buffer → **0.67 ms** of audio per interrupt.
* The ISR converts `u32` ↔ `f32`, runs the effect chain, and writes the TX
  buffer. Everything else happens in the main loop.

---

## 7. Build & flash

```bash
cd firmware
cargo build                    # debug
cargo build --release          # release (LTO)
probe-rs run --chip STM32H723VG            # flash + run
probe-rs run --chip STM32H723VG --release
```

See `docs/BUILD-AND-FLASH.md` for the full toolchain and debugging setup.

---

## 8. Testing

```bash
cd dsp
cargo test          # 49 tests: biquads, FIR, oversampling, waveshaping,
                    # delay, envelopes, LFO, reverb, DC blocker, SVF
```

The tests run on the host, so DSP algorithms can be validated without
hardware. `pedal-dsp` is `no_std` and allocator-free (const-generic buffers).

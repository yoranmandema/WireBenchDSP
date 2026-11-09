//! STM32H723 guitar pedal firmware.
//!
//!   * SAI1 full-duplex I2S @ 48 kHz / 24-bit, DMA circular double buffering
//!   * SGTL5000 codec over I2C1
//!   * 2 pots + expression pedal on ADC1, footswitch on PC13
//!   * `pedal-dsp` effect chain (distortion -> tone -> delay -> reverb)

#![no_std]
#![no_main]

mod audio;
mod board;
mod codec;
mod controls;

use cortex_m::interrupt;
use cortex_m_rt::entry;
use stm32h7xx_hal as hal;
use hal::delay::Delay;
use hal::pac;
use hal::prelude::*;
use hal::rcc::rec::AdcClkSel;

// Minimal panic handler: mask interrupts and spin.  Swap in `panic-probe`
// (with the `print-defmt` feature) for RTT panic reporting when debugging.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    cortex_m::interrupt::disable();
    loop {
        cortex_m::asm::nop();
    }
}
// Logging: this build uses no logger so it links cleanly everywhere.
// To enable defmt + RTT (probe-rs), add to Cargo.toml:
//   defmt = "0.3"
//   defmt-rtt = "0.4"
// then `use defmt_rtt as _;` here and `defmt::info!(...)` where needed.

#[entry]
fn main() -> ! {
    let mut cp = cortex_m::Peripherals::take().expect("core peripherals already taken");
    let dp = pac::Peripherals::take().expect("peripherals already taken");

    // The instruction cache must be enabled or the audio stream breaks up.
    cp.SCB.enable_icache();

    // Clocks: 550 MHz SYSCLK, VOS0, PLL3_P = 12.336 MHz for SAI1.
    let mut ccdr = board::init_clocks(dp.PWR, dp.RCC, &dp.SYSCFG);

    // ADC kernel clock from per_ck (must happen before the GPIO splits move
    // fields out of `ccdr.peripheral`).
    ccdr.peripheral.kernel_adc_clk_mux(AdcClkSel::Per);

    // GPIO ports (each split consumes its reset/enable handle).
    let gpioa = dp.GPIOA.split(ccdr.peripheral.GPIOA);
    let gpiob = dp.GPIOB.split(ccdr.peripheral.GPIOB);
    let gpioc = dp.GPIOC.split(ccdr.peripheral.GPIOC);
    let gpioe = dp.GPIOE.split(ccdr.peripheral.GPIOE);

    let mut delay = Delay::new(cp.SYST, ccdr.clocks);

    // --- control surface ---
    let mut controls = controls::Controls::new(
        dp.ADC1,
        &mut delay,
        ccdr.peripheral.ADC12,
        &ccdr.clocks,
        gpioa,
        gpioc,
    );

    // --- codec ---
    let i2c_pins = (
        gpiob.pb8.into_alternate_open_drain::<4>(),
        gpiob.pb9.into_alternate_open_drain::<4>(),
    );
    let i2c = dp
        .I2C1
        .i2c(i2c_pins, 100.kHz(), ccdr.peripheral.I2C1, &ccdr.clocks);
    let mut codec = codec::Codec::new(i2c);
    let _ = codec.chip_id();
    let _ = codec.init();

    // --- effect chain ---
    cortex_m::interrupt::free(|_| {
        unsafe {
            audio::EFFECTS
                .as_mut_ptr()
                .write(pedal_dsp::chain::EffectChain::new(
                    board::SAMPLE_RATE as f32,
                ));
        }
    });

    // --- audio ---
    audio::start(
        dp.SAI1,
        dp.DMA1,
        ccdr.peripheral.DMA1,
        ccdr.peripheral.SAI1,
        gpioe,
        &ccdr.clocks,
    );

    loop {
        controls.read();

        interrupt::free(|_| {
            let chain = unsafe { &mut *audio::EFFECTS.as_mut_ptr() };
            // Pot 1 -> drive, Pot 2 -> tone, expression -> level.
            chain.drive = 1.0 + controls.pot1_norm * 9.0;
            chain.tone = controls.pot2_norm * 2.0 - 1.0;
            chain.level = 0.4 + controls.expression_norm * 0.6;
            chain.enabled = !controls.footswitch_pressed;
            chain.update_params();
        });

        cortex_m::asm::wfi();
    }
}

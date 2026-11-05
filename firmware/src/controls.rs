//! Control surface: two potentiometers + expression pedal into ADC1, and a
//! footswitch on a GPIO input.
//!
//! Values are normalised to 0..1 and exposed as plain fields; the main loop
//! maps them onto effect parameters.

use stm32h7xx_hal as hal;
use hal::adc;
use hal::delay::Delay;
use hal::gpio::gpioa::{Parts as GpioA, PA0, PA1, PA2};
use hal::gpio::gpioc::{Parts as GpioC, PC13};
use hal::gpio::{Analog, Input};
use hal::pac;
use hal::prelude::*;
use hal::rcc::rec::Adc12;
use hal::rcc::CoreClocks;

pub struct Controls {
    adc: adc::Adc<pac::ADC1, adc::Enabled>,
    pot1: PA0<Analog>,
    pot2: PA1<Analog>,
    expression: PA2<Analog>,
    footswitch: PC13<Input>,

    pub pot1_norm: f32,
    pub pot2_norm: f32,
    pub expression_norm: f32,
    pub footswitch_pressed: bool,
    prev_footswitch: bool,
    /// True when the footswitch changed since the last `read`.
    pub footswitch_toggled: bool,
}

impl Controls {
    pub fn new(
        adc1: pac::ADC1,
        delay: &mut Delay,
        adc12: Adc12,
        clocks: &CoreClocks,
        gpioa: GpioA,
        gpioc: GpioC,
    ) -> Self {
        let mut adc = adc::Adc::adc1(adc1, 4.MHz(), delay, adc12, clocks).enable();
        adc.set_resolution(adc::Resolution::SixteenBit);

        Self {
            adc,
            pot1: gpioa.pa0.into_analog(),
            pot2: gpioa.pa1.into_analog(),
            expression: gpioa.pa2.into_analog(),
            footswitch: gpioc.pc13.into_pull_up_input(),
            pot1_norm: 0.0,
            pot2_norm: 0.0,
            expression_norm: 0.0,
            footswitch_pressed: false,
            prev_footswitch: false,
            footswitch_toggled: false,
        }
    }

    /// Read all controls. Call this from the main loop (not the audio ISR).
    pub fn read(&mut self) {
        self.pot1_norm = read_adc(&mut self.adc, &mut self.pot1);
        self.pot2_norm = read_adc(&mut self.adc, &mut self.pot2);
        self.expression_norm = read_adc(&mut self.adc, &mut self.expression);

        let pressed = self.footswitch.is_low();
        self.footswitch_toggled = pressed != self.prev_footswitch;
        self.prev_footswitch = pressed;
        self.footswitch_pressed = pressed;
    }
}

/// Read a 16-bit ADC channel and normalise to 0..1.
#[inline]
fn read_adc<PIN>(adc: &mut adc::Adc<pac::ADC1, adc::Enabled>, pin: &mut PIN) -> f32
where
    PIN: embedded_hal::adc::Channel<pac::ADC1, ID = u8>,
{
    let raw: u32 = match adc.read(pin) {
        Ok(v) => v,
        Err(_) => return 0.0,
    };
    (raw as f32 / 65535.0).clamp(0.0, 1.0)
}

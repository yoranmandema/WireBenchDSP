//! Board bring-up: power scaling, clocks, and the pin map for the
//! WeAct Studio MiniSTM32H723 (STM32H723VGT6, LQFP100, 25 MHz HSE).
//!
//! Clock tree (matches the vendor example, which is known to work):
//!   HSE 25 MHz --/2--> 12.5 MHz --x44--> 550 MHz VCO --/1--> SYSCLK 550 MHz
//!   HCLK = SYSCLK / 2 = 275 MHz
//!   PLL3_P = 48_000 * 257 = 12.336 MHz -> SAI1 kernel clock
//!             (SAI MCLK = 256 * 48 000 = 12.288 MHz, ~0.4 % off — the same
//!              trick the Electro-Smith Daisy uses)
//!
//! VOS0 is required for 550 MHz.  The WeAct board supplies VCORE from the
//! internal LDO, which is the HAL default.

use stm32h7xx_hal as hal;
use hal::pac;
use hal::prelude::*;
use hal::rcc::{self, PllConfigStrategy};
use hal::time::Hertz;

/// 25 MHz crystal on X1.
pub const HSE: Hertz = Hertz::MHz(25);
/// CPU clock.
pub const SYSCLK: Hertz = Hertz::MHz(550);
/// AHB clock (SYSCLK / 2).
pub const HCLK: Hertz = Hertz::MHz(275);
/// SAI1 kernel clock: 48 kHz * 257.
pub const SAI_KERNEL: Hertz = Hertz::from_raw(48_000 * 257);
/// Audio sample rate.
pub const SAMPLE_RATE: u32 = 48_000;

/// Configure power scaling and the clock tree.
pub fn init_clocks(pwr: pac::PWR, rcc: pac::RCC, syscfg: &pac::SYSCFG) -> rcc::Ccdr {
    // VOS0 (1.26–1.40 V) is needed for 550 MHz.
    let pwrcfg = pwr.constrain().vos0(syscfg).freeze();

    let ccdr = rcc
        .constrain()
        .use_hse(HSE)
        .sys_ck(SYSCLK)
        .hclk(HCLK)
        .pclk1(HCLK / 4)
        .pclk2(HCLK / 4)
        .pll1_strategy(PllConfigStrategy::Iterative)
        .pll1_p_ck(SYSCLK)
        .pll3_strategy(PllConfigStrategy::Iterative)
        .pll3_p_ck(SAI_KERNEL)
        .freeze(pwrcfg, syscfg);

    ccdr
}

/// SAI1 pin assignment (the same wiring the Electro-Smith Daisy Seed uses):
///
/// | Signal | MCU pin | WeAct header | Notes                          |
/// |--------|---------|--------------|--------------------------------|
/// | MCLK_A | PE2     | P2 pin 3     | also QSPI_BK1_IO2 (unused)     |
/// | SCK_A  | PE5     | —            | DCMI_D6; solder to the DCMI cn)|
/// | FS_A   | PE4     | P2 pin 5     |                                |
/// | SD_A   | PE6     | P2 pin 7     | MCU -> codec (I2S out)         |
/// | SD_B   | PE3     | —            | codec -> MCU; shared with LED  |
///
/// PE5 is the only SCK_A option on the LQFP100 package (PF8 is not bonded out),
/// and it is routed to the board's DVP/DCMI connector rather than P1/P2.
pub mod sai_pins {
    use stm32h7xx_hal as hal;
    use hal::gpio::gpioe::*;
    use hal::gpio::Alternate;
    pub type Mclk = PE2<Alternate<6>>;
    pub type Sck = PE5<Alternate<6>>;
    pub type Fs = PE4<Alternate<6>>;
    pub type SdA = PE6<Alternate<6>>;
    pub type SdB = PE3<Alternate<6>>;
}

/// I2C1 for the codec (SGTL5000): SCL = PB8, SDA = PB9.
pub mod i2c_pins {
    use stm32h7xx_hal as hal;
    use hal::gpio::gpiob::*;
    use hal::gpio::Alternate;
    pub type Scl = PB8<Alternate<4>>;
    pub type Sda = PB9<Alternate<4>>;
}

/// Control surface pins.
pub mod control_pins {
    use stm32h7xx_hal as hal;
    use hal::gpio::gpioa::{PA0, PA1, PA2};
    use hal::gpio::gpioc::PC13;
    use hal::gpio::{Analog, Input};
    // Potentiometers / expression pedal into ADC1.
    pub type Pot1 = PA0<Analog>;
    pub type Pot2 = PA1<Analog>;
    pub type Expression = PA2<Analog>;
    // Footswitch (active low, internal pull-up applied in `Controls::new`).
    pub type Footswitch = PC13<Input>;
}

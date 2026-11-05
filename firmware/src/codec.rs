//! SGTL5000 audio codec driver (I2C control port).
//!
//! The SGTL5000 is a stereo codec with line in/out, a headphone amp, and an
//! I2S digital audio port.  It is controlled over I2C at 0x1A (7-bit).
//!
//! Register map follows the Linux ASoC driver (`sound/soc/codecs/sgtl5000.h`)
//! and the datasheet's power-up sequence.

use stm32h7xx_hal as hal;
use hal::prelude::*;
use embedded_hal::blocking::i2c::{Write, WriteRead};

/// 7-bit I2C address (ADDR pin low).
pub const ADDR: u8 = 0x1A;

/// Register addresses.
pub mod reg {
    pub const CHIP_ID: u16 = 0x0000;
    pub const CHIP_DIG_POWER: u16 = 0x0002;
    pub const CHIP_CLK_CTRL: u16 = 0x0004;
    pub const CHIP_I2S_CTRL: u16 = 0x0006;
    pub const CHIP_SSS_CTRL: u16 = 0x000A;
    pub const CHIP_ADCDAC_CTRL: u16 = 0x000E;
    pub const CHIP_DAC_VOL: u16 = 0x0010;
    pub const CHIP_PAD_STRENGTH: u16 = 0x0014;
    pub const CHIP_ANA_ADC_CTRL: u16 = 0x0020;
    pub const CHIP_ANA_HP_CTRL: u16 = 0x0022;
    pub const CHIP_ANA_CTRL: u16 = 0x0024;
    pub const CHIP_LINREG_CTRL: u16 = 0x0026;
    pub const CHIP_REF_CTRL: u16 = 0x0028;
    pub const CHIP_MIC_CTRL: u16 = 0x002A;
    pub const CHIP_LINE_OUT_CTRL: u16 = 0x002C;
    pub const CHIP_LINE_OUT_VOL: u16 = 0x002E;
    pub const CHIP_ANA_POWER: u16 = 0x0030;
    pub const CHIP_PLL_CTRL: u16 = 0x0032;
    pub const CHIP_CLK_TOP_CTRL: u16 = 0x0034;
    pub const CHIP_ANA_STATUS: u16 = 0x0036;
    pub const CHIP_SHORT_CTRL: u16 = 0x003C;
}

pub struct Codec<I2C> {
    i2c: I2C,
}

impl<I2C, E> Codec<I2C>
where
    I2C: embedded_hal::blocking::i2c::WriteRead<Error = E>
        + embedded_hal::blocking::i2c::Write<Error = E>,
{
    pub fn new(i2c: I2C) -> Self {
        Self { i2c }
    }

    fn write_reg(&mut self, reg: u16, value: u16) -> Result<(), E> {
        let bytes = [((reg >> 8) & 0xFF) as u8, (reg & 0xFF) as u8, value as u8];
        self.i2c.write(ADDR, &bytes)
    }

    fn read_reg(&mut self, reg: u16) -> Result<u16, E> {
        let mut buf = [0u8; 2];
        let mut addr = [0u8; 2];
        addr[0] = ((reg >> 8) & 0xFF) as u8;
        addr[1] = (reg & 0xFF) as u8;
        self.i2c.write_read(ADDR, &addr, &mut buf)?;
        Ok(u16::from_be_bytes(buf))
    }

    /// Chip ID should read 0xA0 in the upper byte.
    pub fn chip_id(&mut self) -> Result<u16, E> {
        self.read_reg(reg::CHIP_ID)
    }

    /// Power-up + routing sequence from the SGTL5000 datasheet.
    ///
    /// Assumes VDDIO = 3.3 V, VDDA = 1.8 V, VDDD driven internally,
    /// MCLK = 256 * Fs, Fs = 48 kHz, I2S slave mode.
    pub fn init(&mut self) -> Result<(), E> {
        // --- power supply configuration ---
        self.write_reg(reg::CHIP_LINREG_CTRL, 0x0008)?; // VDDD = 1.2 V internal
        self.write_reg(reg::CHIP_ANA_POWER, 0x7260)?; // power up internal LDO
        self.write_reg(reg::CHIP_ANA_POWER, 0x4260)?; // turn off startup supplies
        self.write_reg(reg::CHIP_CLK_TOP_CTRL, 0x0800)?; // internal oscillator for charge pump
        self.write_reg(reg::CHIP_ANA_POWER, 0x4A60)?; // enable charge pump
        self.write_reg(reg::CHIP_LINREG_CTRL, 0x006C)?; // charge pump from VDDIO rail

        // --- reference / bias ---
        self.write_reg(reg::CHIP_REF_CTRL, 0x004E)?; // VDDA/2 reference, 50% bias
        self.write_reg(reg::CHIP_LINE_OUT_CTRL, 0x0322)?; // LINEOUT ref VDDIO/2
        self.write_reg(reg::CHIP_REF_CTRL, 0x004F)?; // slow ramp (minimise pop)
        self.write_reg(reg::CHIP_SHORT_CTRL, 0x1106)?; // short detect
        self.write_reg(reg::CHIP_ANA_CTRL, 0x0133)?; // zero-cross detect

        // --- power up analogue blocks ---
        self.write_reg(reg::CHIP_ANA_POWER, 0x6AFF)?; // LINEOUT, HP, ADC, DAC

        // --- digital blocks ---
        self.write_reg(reg::CHIP_DIG_POWER, 0x0073)?; // I2S in/out, DAC, ADC

        // --- routing: DAC <- I2S_IN, I2S_OUT <- ADC ---
        self.write_reg(reg::CHIP_SSS_CTRL, 0x0010)?;

        // --- clocks: SYS_FS = 48 kHz, MCLK = 256 * Fs ---
        self.write_reg(reg::CHIP_CLK_CTRL, 0x0008)?;

        // --- digital audio format: I2S, 32-bit, slave, SCLK = 64*Fs ---
        self.write_reg(reg::CHIP_I2S_CTRL, 0x0000)?;

        // --- volumes ---
        self.write_reg(reg::CHIP_DAC_VOL, 0x3C3C)?; // 0 dB both channels
        self.write_reg(reg::CHIP_ADCDAC_CTRL, 0x0000)?; // no mute, no vol ramp
        self.write_reg(reg::CHIP_ANA_ADC_CTRL, 0x0000)?; // 0 dB ADC gain
        self.write_reg(reg::CHIP_ANA_HP_CTRL, 0x7F7F)?; // 0 dB headphone
        self.write_reg(reg::CHIP_LINE_OUT_VOL, 0x0505)?; // LINEOUT level

        Ok(())
    }

    /// Set DAC volume in dB (0 to -90), both channels.
    pub fn set_dac_volume_db(&mut self, db: f32) -> Result<(), E> {
        let steps = (-db / 0.5017).clamp(0.0, 255.0) as u16;
        let v = 0x3Cu16.saturating_sub(steps);
        self.write_reg(reg::CHIP_DAC_VOL, (v << 8) | v)
    }

    /// Mute/unmute the DAC.
    pub fn set_dac_mute(&mut self, mute: bool) -> Result<(), E> {
        let cur = self.read_reg(reg::CHIP_ADCDAC_CTRL)?;
        let v = if mute { cur | 0x000C } else { cur & !0x000C };
        self.write_reg(reg::CHIP_ADCDAC_CTRL, v)
    }
}

//! SAI1 + DMA audio engine.
//!
//! Full-duplex I2S at 48 kHz / 24-bit, stereo, with circular DMA double
//! buffering.  The RX DMA (DMA1 stream 1) raises half-transfer and
//! transfer-complete interrupts; each one processes half a buffer and writes
//! the result into the TX buffer.
//!
//! Buffer layout: `[L0 R0 L1 R1 ... ]` as 32-bit words (24-bit data left
//! justified).  The DMA runs in circular mode, so the two halves alternate.

use core::mem::MaybeUninit;

use cortex_m_rt::interrupt as interrupt_attr;
use stm32h7xx_hal as hal;
use hal::dma;
use hal::pac;
use hal::pac::interrupt;
use hal::rcc::rec::{Dma1, Sai1};
use hal::rcc::CoreClocks;
use hal::sai::{
    self, I2SChanConfig, I2SDataSize, I2SDir, I2SSync, I2sUsers, SaiChannel, SaiDmaExt,
    SaiI2sExt,
};
use hal::time::Hertz;
use hal::traits::i2s::FullDuplex;

use crate::board::SAMPLE_RATE;
use crate::effects::EffectChain;

/// Stereo frames processed per interrupt.
pub const FRAMES_PER_BLOCK: usize = 32;
/// 32-bit words per half-buffer (2 per stereo frame).
pub const WORDS_PER_BLOCK: usize = FRAMES_PER_BLOCK * 2;
/// Total circular buffer length (two halves).
pub const BUFFER_LEN: usize = WORDS_PER_BLOCK * 2;

pub const AUDIO_SAMPLE_HZ: Hertz = Hertz::from_raw(SAMPLE_RATE);

/// The DSP chain, shared between the control loop and the audio interrupt.
/// `MaybeUninit` keeps this 180 KB object in `.bss` rather than `.data`.
pub static mut EFFECTS: MaybeUninit<EffectChain> = MaybeUninit::uninit();

static mut TX_BUFFER: MaybeUninit<[u32; BUFFER_LEN]> = MaybeUninit::uninit();
static mut RX_BUFFER: MaybeUninit<[u32; BUFFER_LEN]> = MaybeUninit::uninit();

type RxTransfer = dma::Transfer<
    dma::dma::Stream1<pac::DMA1>,
    sai::dma::ChannelA<pac::SAI1>,
    dma::PeripheralToMemory,
    &'static mut [u32; BUFFER_LEN],
    dma::DBTransfer,
>;

type TxTransfer = dma::Transfer<
    dma::dma::Stream0<pac::DMA1>,
    sai::dma::ChannelB<pac::SAI1>,
    dma::MemoryToPeripheral,
    &'static mut [u32; BUFFER_LEN],
    dma::DBTransfer,
>;

static mut RX_TRANSFER: MaybeUninit<Option<RxTransfer>> = MaybeUninit::uninit();
static mut TX_TRANSFER: MaybeUninit<Option<TxTransfer>> = MaybeUninit::uninit();

/// Initialise SAI1, DMA1 and start the audio stream.
///
/// Takes the DMA1 and SAI1 reset/enable handles by value because the HAL's
/// `PeripheralREC` methods consume them.
pub fn start(
    sai1: pac::SAI1,
    dma1: pac::DMA1,
    dma1_rec: Dma1,
    sai1_rec: Sai1,
    gpioe: hal::gpio::gpioe::Parts,
    clocks: &CoreClocks,
) {
    // --- pins (MCLK, SCK, FS, SD_A/TX, SD_B/RX) ---
    let pins = (
        gpioe.pe2.into_alternate::<6>(),        // MCLK_A
        gpioe.pe5.into_alternate::<6>(),        // SCK_A
        gpioe.pe4.into_alternate::<6>(),        // FS_A
        gpioe.pe6.into_alternate::<6>(),        // SD_A (TX)
        Some(gpioe.pe3.into_alternate::<6>()),  // SD_B (RX)
    );

    // --- DMA streams ---
    let streams = dma::dma::StreamsTuple::new(dma1, dma1_rec);

    // Safety: these buffers are only touched by this module and the DMA.
    let tx_buffer: &'static mut [u32; BUFFER_LEN] = unsafe { TX_BUFFER.assume_init_mut() };
    let rx_buffer: &'static mut [u32; BUFFER_LEN] = unsafe { RX_BUFFER.assume_init_mut() };

    let tx_config = dma::dma::DmaConfig::default()
        .priority(dma::config::Priority::High)
        .memory_increment(true)
        .peripheral_increment(false)
        .circular_buffer(true)
        .fifo_enable(false);

    let tx_transfer: TxTransfer = dma::Transfer::init(
        streams.0,
        unsafe { pac::Peripherals::steal().SAI1.dma_ch_b() },
        tx_buffer,
        None,
        tx_config,
    );

    let rx_config = tx_config
        .transfer_complete_interrupt(true)
        .half_transfer_interrupt(true);

    let rx_transfer: RxTransfer = dma::Transfer::init(
        streams.1,
        unsafe { pac::Peripherals::steal().SAI1.dma_ch_a() },
        rx_buffer,
        None,
        rx_config,
    );

    // --- SAI1 configuration ---
    let sai1_rec = sai1_rec.kernel_clk_mux(hal::rcc::rec::Sai1ClkSel::Pll3P);

    let tx_chan = I2SChanConfig::new(I2SDir::Tx)
        .set_frame_sync_active_high(true)
        .set_clock_strobe(sai::I2SClockStrobe::Falling);

    let rx_chan = I2SChanConfig::new(I2SDir::Rx)
        .set_sync_type(I2SSync::Internal)
        .set_frame_sync_active_high(true)
        .set_clock_strobe(sai::I2SClockStrobe::Rising);

    let mut sai = sai1.i2s_ch_a(
        pins,
        AUDIO_SAMPLE_HZ,
        I2SDataSize::BITS_24,
        sai1_rec,
        clocks,
        I2sUsers::new(tx_chan).add_slave(rx_chan),
    );

    // Unmask the RX DMA interrupt.
    unsafe { pac::NVIC::unmask(pac::Interrupt::DMA1_STR1) }

    // Keep the transfers alive for the ISR.
    unsafe {
        TX_TRANSFER.write(Some(tx_transfer));
        RX_TRANSFER.write(Some(rx_transfer));
    }

    // Start the stream.  The RX stream is started first so it is ready to
    // capture data as soon as the SAI begins transmitting.
    unsafe {
        let tx = TX_TRANSFER.assume_init_mut().as_mut().unwrap();
        let rx = RX_TRANSFER.assume_init_mut().as_mut().unwrap();

        rx.start(|_sai_rb| {
            sai.enable_dma(SaiChannel::ChannelB);
        });

        tx.start(|sai_rb| {
            sai.enable_dma(SaiChannel::ChannelA);
            // Wait for the SAI TX FIFO to accept data, then enable the block
            // and push a sample to kick things off.
            while sai_rb.cha().sr.read().flvl().is_empty() {}
            sai.enable();
            sai.try_send(0, 0).expect("SAI TX stalled");
        });
    }
}

/// RX DMA half/complete interrupt: process one half of the ring buffer.
#[interrupt_attr]
fn DMA1_STR1() {
    let start = unsafe {
        let transfer = RX_TRANSFER
            .assume_init_mut()
            .as_mut()
            .expect("RX transfer not started");
        if transfer.get_half_transfer_flag() {
            transfer.clear_half_transfer_interrupt();
            0
        } else if transfer.get_transfer_complete_flag() {
            transfer.clear_transfer_complete_interrupt();
            WORDS_PER_BLOCK
        } else {
            return;
        }
    };

    let rx_ptr = unsafe { RX_BUFFER.as_ptr() as *const u32 };
    let tx_ptr = unsafe { TX_BUFFER.as_mut_ptr() as *mut u32 };

    for i in 0..FRAMES_PER_BLOCK {
        let l = word_to_f32(unsafe { *rx_ptr.add(start + 2 * i) });
        let r = word_to_f32(unsafe { *rx_ptr.add(start + 2 * i + 1) });

        let (lo, ro) = cortex_m::interrupt::free(|_| {
            let chain = unsafe { &mut *EFFECTS.as_mut_ptr() };
            chain.process_stereo(l, r)
        });

        unsafe {
            *tx_ptr.add(start + 2 * i) = f32_to_word(lo);
            *tx_ptr.add(start + 2 * i + 1) = f32_to_word(ro);
        }
    }
}

/// Convert a left-justified 24-bit I2S word to a float in roughly [-1, 1].
#[inline]
pub fn word_to_f32(word: u32) -> f32 {
    (word as i32) as f32 / 2_147_483_648.0
}

/// Convert a float in [-1, 1] to a left-justified 24-bit I2S word.
#[inline]
pub fn f32_to_word(x: f32) -> u32 {
    let clamped = x.clamp(-1.0, 1.0);
    let sample = (clamped * 8_388_607.0) as i32; // 24-bit
    (sample << 8) as u32
}

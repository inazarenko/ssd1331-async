//! Demonstrates how to use SSD1331 display on STM32F103 (Blue Pill, Maple,
//! etc).
//!
//! Connections:
//!
//! | Display | MCU   |
//! |---------|-------|
//! | GND     | GND   |
//! | VCC     | 3.3V  |
//! | SCL     | PA5   |
//! | SDA     | PA7   |
//! | RES     | PA0   |
//! | DC      | PC15  |
//! | CS      | PC14  |
//!
//! Assuming you have a debug probe connected to your board and probe-rs tools
//! installed, running the example with cargo should program the board. Note
//! that dev build may not fit into the flash memory of STM32F103C8.
//!
//! ```sh
//! cargo run --release --example main
//! ```

#![no_std]
#![no_main]

use cortex_m_rt::exception;
use defmt::error;
use embassy_executor::Spawner;
use embassy_stm32::{gpio, spi};
use embassy_time::{Delay, Duration, Timer};
use embedded_graphics::prelude::*;
use embedded_hal_bus::spi::ExclusiveDevice;
use ssd1331_async::{Config, Framebuffer, Rgb332, Ssd1331, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use static_cell::ConstStaticCell;

use {defmt_rtt as _, panic_probe as _};

const FRAME_BUFFER_SIZE: usize = DISPLAY_WIDTH as usize * DISPLAY_HEIGHT as usize;
static PIXEL_DATA: ConstStaticCell<[u8; FRAME_BUFFER_SIZE]> =
    ConstStaticCell::new([0; FRAME_BUFFER_SIZE]);

fn fast_config() -> embassy_stm32::Config {
    let mut cfg = embassy_stm32::Config::default();
    cfg.rcc.hse = Some(embassy_stm32::rcc::Hse {
        freq: embassy_stm32::time::Hertz(8_000_000),
        mode: embassy_stm32::rcc::HseMode::Oscillator,
    });
    cfg.rcc.apb1_pre = embassy_stm32::rcc::APBPrescaler::DIV2;
    cfg.rcc.sys = embassy_stm32::rcc::Sysclk::PLL1_P;
    cfg.rcc.pll = Some(embassy_stm32::rcc::Pll {
        src: embassy_stm32::rcc::PllSource::HSE,
        prediv: embassy_stm32::rcc::PllPreDiv::DIV1,
        mul: embassy_stm32::rcc::PllMul::MUL9,
    });
    cfg
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut p = embassy_stm32::init(fast_config());

    let mut display = {
        let mut spi_config = spi::Config::default();
        spi_config.frequency = embassy_stm32::time::Hertz(50_000_000);
        let spi_bus = spi::Spi::new_txonly(
            &mut p.SPI1,
            &mut p.PA5,
            &mut p.PA7,
            &mut p.DMA1_CH3,
            spi_config,
        );
        let cs = gpio::Output::new(&mut p.PC14, gpio::Level::Low, gpio::Speed::VeryHigh);
        let spi_dev = ExclusiveDevice::new_no_delay(spi_bus, cs).unwrap();

        let rst = gpio::Output::new(&mut p.PA0, gpio::Level::Low, gpio::Speed::VeryHigh);
        let dc = gpio::Output::new(&mut p.PC15, gpio::Level::Low, gpio::Speed::VeryHigh);

        Ssd1331::new(Config::default(), rst, dc, spi_dev, &mut Delay {})
            .await
            .unwrap()
    };

    let pixel_data = PIXEL_DATA.take();

    let mut fb = Framebuffer::<Rgb332>::new(pixel_data, display.size());
    fb.clear(Rgb332::new(0, 0, 1)).unwrap();

    display.write_all(&fb).await.unwrap();

    loop {
        Timer::after(Duration::from_millis(1000)).await;
    }
}

#[allow(non_snake_case)]
#[exception]
unsafe fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    error!("HardFault at {:#010x}", ef.pc());
    loop {
        cortex_m::asm::nop();
    }
}

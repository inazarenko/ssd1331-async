//! Async driver for SSD1331-based displays with SPI interface.

#![no_std]

use command::Command;
use embedded_graphics_core::pixelcolor::raw::ToBytes;
use embedded_graphics_core::prelude::{Dimensions, OriginDimensions, PixelColor, Point, Size};
use embedded_graphics_core::primitives::Rectangle;
use embedded_hal::digital::OutputPin;
use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::spi::SpiDevice;
use heapless::Vec;

mod command;
mod framebuffer;
mod rgb332;

pub use framebuffer::Framebuffer;
pub use rgb332::Rgb332;

pub const DISPLAY_WIDTH: u32 = 96;
pub const DISPLAY_HEIGHT: u32 = 64;

/// Number of bits per pixel in a data transfer.
///
/// The display internally supports BGR order and alternative 16-bit color
/// mode, but this driver does not, so effectively 8-bit is Rgb332 and 16-bit
/// is Rgb565. The built-in display RAM always uses 16 bits per pixel. When
/// sending 8-bit data, the display controller fills in the lower bits. 16-bit
/// pixels are always sent in big-endian order.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BitDepth {
    Eight = 0x00,
    Sixteen = 0x40, // Default after reset.
}

impl BitDepth {
    pub fn bytes(&self) -> usize {
        match self {
            Self::Eight => 1,
            Self::Sixteen => 2,
        }
    }
}

/// Row- or column-major order of pixels for a data transfer.
///
/// This can be changed before any transfer, but this driver just sets it on
/// init matching the display orientation (portrait or landscape).
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PixelOrder {
    RowMajor = 0x00, // Default after reset.
    ColumnMajor = 0x01,
}

/// Order in which a data transfer populates a given RAM row.
///
/// The display controller docs make it sound like this bit sets the mapping
/// between RAM and display pixels, but it really doesn't: if you flip the
/// flag after the transfer, the display will not change. Maybe there's a
/// clever use case for setting this per transfer, but this driver just sets
/// it once on init.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ColumnDirection {
    LeftToRight = 0x00, // Default after reset.
    RightToLeft = 0x02,
}

/// Mapping between RAM rows and physical display rows.
///
/// Changing this flips the displayed pixels vertically without modifying RAM
/// contents.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RowDirection {
    Normal = 0x00,
    Reversed = 0x10,
}

/// Whether the physical display rows are interleaved compared to the RAM
/// rows.
///
/// Most displays based on SSD1331 controller seem to interleave the pins, so
/// all pre-configured data mappings set this.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RowInterleave {
    Disabled = 0x00, // Default after reset.
    Enabled = 0x20,
}

/// Describes the mapping between the display memory and the physical pixels.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub pixel_order: PixelOrder,
    pub column_direction: ColumnDirection,
    pub row_direction: RowDirection,
    pub row_interleave: RowInterleave,
}

impl Default for Config {
    /// Creates a configuration for the default orientation.
    ///
    /// This is the same settings as the display assumes after reset, except
    /// the row interleave is enabled. On my model, this is portrait mode with
    /// the pins below the screen.
    fn default() -> Self {
        Self {
            pixel_order: PixelOrder::RowMajor,
            column_direction: ColumnDirection::LeftToRight,
            row_direction: RowDirection::Normal,
            row_interleave: RowInterleave::Enabled,
        }
    }
}

impl Config {
    /// For orientation rotated 90 degrees counter-clockwise from the default.
    pub fn ccw90() -> Self {
        Self {
            pixel_order: PixelOrder::ColumnMajor,
            column_direction: ColumnDirection::LeftToRight,
            row_direction: RowDirection::Reversed,
            row_interleave: RowInterleave::Enabled,
        }
    }

    /// For orientation rotated 180 degrees from the default.
    pub fn ccw180() -> Self {
        Self {
            pixel_order: PixelOrder::RowMajor,
            column_direction: ColumnDirection::RightToLeft,
            row_direction: RowDirection::Reversed,
            row_interleave: RowInterleave::Enabled,
        }
    }

    /// For orientation rotated 270 degrees counter-clockwise from the default.
    pub fn ccw270() -> Self {
        Self {
            pixel_order: PixelOrder::ColumnMajor,
            column_direction: ColumnDirection::RightToLeft,
            row_direction: RowDirection::Normal,
            row_interleave: RowInterleave::Enabled,
        }
    }
}

/// Error type for this driver.
///
/// Currently only used to propagate errors from the HAL.
#[derive(Debug)]
pub enum Error<PinE, SpiE> {
    Pin(PinE),
    Spi(SpiE),
}

/// The implementation of the driver.
///
/// Can be used with [`embedded-graphics`] crate in async frameworks (e.g.
/// Embassy). Since the `embedded-graphics` API is synchronous, the driver
/// assumes use of a framebuffer, and provides an async method to transfer its
/// contents to the display. Full-size framebuffer requires ~12Kb (6Kb for
/// 8-bit color mode). The driver allows using a smaller buffer and addressing
/// a sub-area of the display; for example, it's possible to draw monospaced
/// text one character at a time, or mix text and graphics areas.
///
/// The driver dutifully propagates all errors from the HAL, but the display
/// controller is stateful and the driver doesn't attempt to return it to a
/// known good state after an error. You can call `init()` to hard-reset the
/// display and reinitialize the driver after an error.
///
/// [`embedded-graphics`]: https://crates.io/crates/embedded-graphics
pub struct Ssd1331<RST, DC, SPI> {
    data_mapping: Config,

    rst: RST,
    dc: DC,
    spi: SPI,

    bit_depth: BitDepth,
    area: Rectangle,

    command_buf: Vec<u8, 16>,
}

impl<RST, DC, SPI> OriginDimensions for Ssd1331<RST, DC, SPI> {
    fn size(&self) -> Size {
        if self.data_mapping.pixel_order == PixelOrder::RowMajor {
            Size::new(DISPLAY_WIDTH, DISPLAY_HEIGHT)
        } else {
            Size::new(DISPLAY_HEIGHT, DISPLAY_WIDTH)
        }
    }
}

impl<RST, DC, SPI, PinE, SpiE> Ssd1331<RST, DC, SPI>
where
    RST: OutputPin<Error = PinE>,
    DC: OutputPin<Error = PinE>,
    SPI: SpiDevice<Error = SpiE>,
{
    /// Creates a new driver instance and initializes the display.
    ///
    /// Requires GPIO output pins connected to RST and DC pins on the display,
    /// and a SPI device with SDO and SCK outputs connected to the display.
    /// The CS (chip select) pin of the display can be controlled by the SPI
    /// device, or you can simply tie it low, and pass a DummyPin to the SPI
    /// device. SPI bus should be configured to MODE_0, MSB first (usually the
    /// default). Frequencies up to 50 MHz seem to work fine, even though the
    /// display datasheet specifies ~6 MHz max.
    pub async fn new(
        data_mapping: Config,
        rst: RST,
        dc: DC,
        spi: SPI,
        delay: &mut impl DelayNs,
    ) -> Result<Self, Error<PinE, SpiE>> {
        let mut d = Self {
            rst,
            dc,
            spi,
            data_mapping,
            bit_depth: BitDepth::Sixteen,
            area: Rectangle::zero(), // Just until init().
            command_buf: Vec::new(),
        };

        d.init(delay).await?;

        Ok(d)
    }

    /// Hard-resets and re-initializes the display.
    ///
    /// Also clears the display RAM. This will take a few milliseconds.
    /// Instances returned by [Self::new] are already initialized.
    pub async fn init(&mut self, delay: &mut impl DelayNs) -> Result<(), Error<PinE, SpiE>> {
        // Hold the display in reset for 1ms. Note that this does not seem to
        // clear the onboard RAM. The RST pin behaves as NRST (low level resets
        // the display).
        self.rst.set_low().map_err(Error::Pin)?;
        delay.delay_ms(1).await;
        self.rst.set_high().map_err(Error::Pin)?;
        delay.delay_ms(1).await;

        self.area = Rectangle::new(Point::zero(), Size::new(DISPLAY_WIDTH, DISPLAY_HEIGHT));
        self.bit_depth = BitDepth::Sixteen;

        self.command_buf.clear();

        self.send_commands(&[
            Command::RemapAndBitDepth(self.data_mapping, self.bit_depth),
            // Default is 15, results in grays saturating at about 50%.
            Command::MasterCurrent(5),
            // Default is 0x80 for all. Lowering the G channel seems to result
            // in a better color balance on my display. This should be a user
            // setting.
            Command::Contrast(0x80, 0x50, 0x80),
            Command::ClearWindow(self.area),
            Command::DisplayOn(true),
        ])
        .await?;

        // ClearWindow needs time to write to RAM.
        delay.delay_ms(1).await;

        Ok(())
    }

    /// Consumes the driver and returns the peripherals to you.
    pub fn release(self) -> (RST, DC, SPI) {
        (self.rst, self.dc, self.spi)
    }

    /// Sends the data to the given area of the display's frame buffer.
    ///
    /// The `area` is in your logical display coordinates; e.g if you use
    /// [Config::ccw90], the logical size is (64, 96) and the (0, 0) is the
    /// top-right corner of the un-rotated physical screen.
    ///
    /// You can fill the area using a smaller buffer by repeatedly calling
    /// this method and passing the same `area`. Sending more data than fits
    /// in the area will wrap around and overwrite the beginning of the area.
    ///
    /// # Panics
    ///
    /// If the area is empty or not completely contained within the display
    /// bounds.
    pub async fn write_pixels(
        &mut self,
        data: &[u8],
        bit_depth: BitDepth,
        area: Rectangle,
    ) -> Result<(), Error<PinE, SpiE>> {
        assert!(self.bounding_box().contains(area.top_left));
        assert!(self.bounding_box().contains(area.bottom_right().unwrap()));
        assert!(self.command_buf.is_empty());
        if self.bit_depth != bit_depth {
            self.bit_depth = bit_depth;
            assert!(Command::RemapAndBitDepth(self.data_mapping, self.bit_depth)
                .push(&mut self.command_buf));
        }
        let ram_area = self.ram_area(area);
        if self.area != ram_area {
            self.area = ram_area;
            assert!(Command::AddressRectangle(self.area).push(&mut self.command_buf));
        }
        self.flush_commands().await?;
        self.dc.set_high().map_err(Error::Pin)?;
        self.spi.write(data).await.map_err(Error::Spi)?;

        Ok(())
    }

    // Returns display RAM rectangle for the given rectangle on the logical
    // display. The display controller takes into account the X/Y mirroring
    // settings, but the axis remain X and Y regardless of the pixel order.
    fn ram_area(&self, area: Rectangle) -> Rectangle {
        if self.data_mapping.pixel_order == PixelOrder::RowMajor {
            area
        } else {
            Rectangle::new(
                Point::new(area.top_left.y, area.top_left.x),
                Size::new(area.size.height, area.size.width),
            )
        }
    }

    async fn send_commands(&mut self, commands: &[Command]) -> Result<(), Error<PinE, SpiE>> {
        for command in commands {
            if command.push(&mut self.command_buf) {
                continue;
            }
            self.flush_commands().await?;
            assert!(command.push(&mut self.command_buf));
        }
        self.flush_commands().await?;
        Ok(())
    }

    async fn flush_commands(&mut self) -> Result<(), Error<PinE, SpiE>> {
        if !self.command_buf.is_empty() {
            self.dc.set_low().map_err(Error::Pin)?;
            self.spi
                .write(&self.command_buf)
                .await
                .map_err(Error::Spi)?;
            self.command_buf.clear();
        }
        Ok(())
    }
}

/// Convenience trait to hide details of the driver type.
///
/// Once the display driver is created, only the error type depends on the HAL
/// types used for the implementation. For the use cases where panic on error
/// is acceptable, we can ignore the type parameters.
#[allow(async_fn_in_trait)]
pub trait WritePixels {
    /// See [Ssd1331::write_pixels].
    async fn write_pixels(&mut self, data: &[u8], bit_depth: BitDepth, area: Rectangle);

    /// Transfers the contents of the framebuffer to the display.
    async fn flush<C>(&mut self, fb: &Framebuffer<'_, C>, top_left: Point)
    where
        C: PixelColor + ToBytes,
    {
        self.write_pixels(
            fb.data(),
            fb.bit_depth(),
            Rectangle::new(top_left, fb.size()),
        )
        .await
    }
}

impl<RST, DC, SPI, PinE, SpiE> WritePixels for Ssd1331<RST, DC, SPI>
where
    RST: OutputPin<Error = PinE>,
    DC: OutputPin<Error = PinE>,
    SPI: SpiDevice<Error = SpiE>,
{
    async fn write_pixels(&mut self, data: &[u8], bit_depth: BitDepth, area: Rectangle) {
        self.write_pixels(data, bit_depth, area)
            .await
            .unwrap_or_else(|_| panic!("write failed"))
    }
}

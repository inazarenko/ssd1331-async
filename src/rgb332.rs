use embedded_graphics_core::{
    pixelcolor::{raw::RawU8, Gray8, Rgb888},
    prelude::{GrayColor, IntoStorage, PixelColor, RgbColor},
};

/// Color format used by SSD1331 display when in 8-bit color mode.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rgb332(RawU8);

impl Rgb332 {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self(RawU8::new(
            ((r & Self::MAX_R) << 5) | ((g & Self::MAX_G) << 2) | (b & Self::MAX_B),
        ))
    }
}

impl PixelColor for Rgb332 {
    type Raw = RawU8;
}

impl From<RawU8> for Rgb332 {
    fn from(data: RawU8) -> Self {
        Self(data)
    }
}

impl From<Rgb332> for RawU8 {
    fn from(data: Rgb332) -> RawU8 {
        data.0
    }
}

impl RgbColor for Rgb332 {
    fn r(&self) -> u8 {
        (self.into_storage() >> 5) & Self::MAX_R
    }

    fn g(&self) -> u8 {
        (self.into_storage() >> 2) & Self::MAX_G
    }

    fn b(&self) -> u8 {
        self.into_storage() & Self::MAX_B
    }

    const MAX_R: u8 = 7;
    const MAX_G: u8 = 7;
    const MAX_B: u8 = 3;

    const BLACK: Self = Self::new(0, 0, 0);
    const RED: Self = Self::new(Self::MAX_R, 0, 0);
    const GREEN: Self = Self::new(0, Self::MAX_G, 0);
    const BLUE: Self = Self::new(0, 0, Self::MAX_B);
    const YELLOW: Self = Self::new(Self::MAX_R, Self::MAX_G, 0);
    const MAGENTA: Self = Self::new(Self::MAX_R, 0, Self::MAX_B);
    const CYAN: Self = Self::new(0, Self::MAX_G, Self::MAX_B);
    const WHITE: Self = Self::new(Self::MAX_R, Self::MAX_G, Self::MAX_B);
}

impl From<Gray8> for Rgb332 {
    fn from(color: Gray8) -> Self {
        let luma = color.luma();
        Self::new(luma >> 5, luma >> 5, luma >> 6)
    }
}

impl From<Rgb888> for Rgb332 {
    fn from(c: Rgb888) -> Self {
        Self::new(c.r() >> 5, c.g() >> 5, c.b() >> 6)
    }
}

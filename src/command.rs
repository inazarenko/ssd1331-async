/// Commands for the SSD1331 controller.
///
/// The code in this module originated from [ssd1331] crate. Thanks to James
/// Waples and contributors.
///
/// [ssd1331]:  https://github.com/rust-embedded-community/ssd1331

use embedded_graphics_core::{pixelcolor::Rgb565, prelude::{Point, RgbColor}, primitives::Rectangle};
use heapless::Vec;

use crate::{ColorMode, Config};

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub(crate) enum Command {
    /// Set master current, 0..15 corresponding to 1/16 - 16/16 attenuation.
    MasterCurrent(u8),
    /// Set (r, g, b) contrast. Higher number is higher contrast.
    Contrast(u8, u8, u8),
    /// Turn display on or off.
    DisplayOn(bool),
    /// Set mapping between the incoming data and the display pixels.
    RemapAndColorDepth(Config, ColorMode),
    /// Fill the given window of RAM with zeros. The rectangle is in RAM
    /// coordinates; that is, the max X is 96 even when the display is in
    /// column-major mode. Internally, the display controller needs time
    /// to write to the RAM, so you may need a delay after this command.
    /// For a full-screen clear, 500 us seems to be enough.
    ClearWindow(Rectangle),
    /// Sends a sequence of Column/Row address commands to address a non-empty
    /// rectangle. Similar caveats to ClearWindow.
    AddressRectangle(Rectangle),
    /// Draw a line of the given color.
    DrawLine(Point, Point, Rgb565),
    /// Draw rectangle with given border and (if fill mode is enabled)
    /// interior colors. Requires that the rectangle is not empty.
    DrawRectangle(Rectangle, Rgb565, Rgb565),
    /// Set fill enabled or disabled for DrawRectangle command.
    SetFillEnabled(bool),
    /// No-op.
    NoOp,
}

fn clamp(c: i32) -> u8 {
    (c.max(0) & 0xFF) as u8
}

impl Command {
    pub fn push<const N: usize>(&self, buf: &mut Vec<u8, N>) -> bool {
        let result = match self {
            &Command::MasterCurrent(current) => &[0x87, current.min(15)],
            &Command::Contrast(r, g, b) => &[0x81, r, 0x82, g, 0x83, b] as &[u8],
            &Command::DisplayOn(on) => &[0xAE | (on as u8)],
            &Command::RemapAndColorDepth(dm, cm) => &[
                0xA0,
                (dm.row_direction as u8)
                    | (dm.row_interleave as u8)
                    | (dm.pixel_order as u8)
                    | (dm.column_direction as u8)
                    | (cm as u8),
            ],
            &Command::ClearWindow(r) => {
                let br = r.bottom_right().unwrap();
                &[
                    0x25,
                    clamp(r.top_left.x),
                    clamp(r.top_left.y),
                    clamp(br.x),
                    clamp(br.y),
                ]
            }
            &Command::AddressRectangle(r) => {
                let br = r.bottom_right().unwrap();
                &[
                    0x15,
                    clamp(r.top_left.x),
                    clamp(br.x),
                    0x75,
                    clamp(r.top_left.y),
                    clamp(br.y),
                ]
            }
            &Command::DrawLine(a, b, color) => &[
                0x21,
                clamp(a.x),
                clamp(a.y),
                clamp(b.x),
                clamp(b.y),
                color.r(),
                color.g(),
                color.b(),
            ],
            &Command::DrawRectangle(r, border, fill) => {
                let br = r.bottom_right().unwrap();
                &[
                    0x22,
                    clamp(r.top_left.x),
                    clamp(r.top_left.y),
                    clamp(br.x),
                    clamp(br.y),
                    border.r(),
                    border.g(),
                    border.b(),
                    fill.r(),
                    fill.g(),
                    fill.b(),
                ]
            }
            &Command::SetFillEnabled(enabled) => &[0x26, enabled as u8],
            &Command::NoOp => &[0xBC],
        };

        buf.extend_from_slice(result).is_ok()
    }
}

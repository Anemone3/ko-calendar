use iced::{Color, Element, widget::svg};
use std::sync::LazyLock;

#[derive(Clone, Copy)]
pub enum Icon {
    Left,
    Right,
    Down,
    Up,
    Close,
    Bell,
    Sound,
    Video,
    Link,
    External,
    Pin,
    Focus,
    Clock,
    Back,
    Settings,
}

impl Icon {
    pub fn path(self) -> &'static str {
        match self {
            Self::Left => "<path d='m14 6-6 6 6 6'/>",
            Self::Right => "<path d='m10 6 6 6-6 6'/>",
            Self::Down => "<path d='m6 9 6 6 6-6'/>",
            Self::Up => "<path d='m6 15 6-6 6 6'/>",
            Self::Close => "<path d='m6 6 12 12M6 18 18 6'/>",

            Self::Bell => "<path d='M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9M10 21h4'/>",
            Self::Sound => {
                "<path d='M11 4 6 8H3v8h3l5 4ZM15 8a6 6 0 0 1 0 8M18 5a10 10 0 0 1 0 14'/>"
            }
            Self::Video => {
                "<rect x='3' y='6' width='12' height='12' rx='3'/><path d='m15 10 6-3v10l-6-3'/>"
            }
            Self::Link => {
                "<path d='m10 13 4-4m-6 6-1 1a4 4 0 0 1-6-6l4-4a4 4 0 0 1 6 0m2 3 1-1a4 4 0 1 1 6 6l-4 4a4 4 0 0 1-6 0' transform='translate(1 1)'/>"
            }
            Self::External => {
                "<path d='M14 3h7v7M21 3 10 14M10 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5'/>"
            }
            Self::Pin => {
                "<path d='M20 10c0 6-8 11-8 11S4 16 4 10a8 8 0 0 1 16 0Z'/><circle cx='12' cy='10' r='2.5'/>"
            }
            Self::Focus => "<circle cx='12' cy='12' r='8'/><path d='M12 7v5l3 2'/>",

            Self::Clock => "<circle cx='12' cy='12' r='9'/><path d='M12 6v6l4 2'/>",

            Self::Back => "<path d='m10 6-6 6 6 6M4 12h16'/>",
            Self::Settings => {
                "<circle cx='12' cy='12' r='3'/><path d='m9 3-.5 2-2 1-2-.5-2 3.5 1.5 1.5v3L2.5 15l2 3.5 2-.5 2 1 .5 2h6l.5-2 2-1 2 .5 2-3.5-1.5-1.5v-3L21.5 9l-2-3.5-2 .5-2-1-.5-2Z'/>"
            }
        }
    }
}

pub fn icon<'a, Message: 'a>(kind: Icon, size: f32, color: Color) -> Element<'a, Message> {
    let color = format!(
        "#{:02x}{:02x}{:02x}",
        (color.r * 255.) as u8,
        (color.g * 255.) as u8,
        (color.b * 255.) as u8
    );
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24' fill='none' stroke='{color}' stroke-width='1.65' stroke-linecap='round' stroke-linejoin='round'>{}</svg>",
        kind.path()
    );
    svg(svg::Handle::from_memory(source.into_bytes()))
        .width(size)
        .height(size)
        .into()
}

pub fn tray_pixels() -> Vec<u8> {
    melon_rgba(&MELON, &MELON_PALETTE, TRAY_SIZE / MELON_SIZE)
}

/// Monochrome melon for the macOS menu bar, meant to be shown as a template
/// image: macOS keeps only its alpha and tints it to match the menu bar.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn tray_template_pixels() -> Vec<u8> {
    melon_rgba(
        &MELON_TEMPLATE,
        &MELON_TEMPLATE_PALETTE,
        TRAY_SIZE / MELON_SIZE,
    )
}

/// Side, in pixels, of the RGBA buffers returned by the tray helpers.
pub const TRAY_SIZE: usize = 32;
const MELON_SIZE: usize = 16;
const KEYS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

// Kokonut's melon logo as 16 × 16 pixel art: each character is an index into
// the palette (see KEYS) and `.` is a transparent pixel.
const MELON_PALETTE: [u32; 34] = [
    0x4A3E00, 0x6F6000, 0x917D00, 0x344509, 0xFFA68C, 0xF64B1D, 0xFFBFA3, 0xE02906, 0x3E4700,
    0xFFD387, 0xFFDCCB, 0xD61E01, 0xFFC300, 0xF9532B, 0xD01500, 0xEE3D1C, 0xE7330F, 0xFFF08A,
    0xC60F00, 0x675A00, 0xFFB900, 0xFFE200, 0xF03B15, 0xB29308, 0xF86E47, 0xF89800, 0x2E3B08,
    0xF97D55, 0xF65C38, 0xFFCAAF, 0xCBB110, 0xFFAF8F, 0x475200, 0x514F00,
];
const MELON: [&str; MELON_SIZE] = [
    "..abc...........",
    ".dbbef..........",
    ".abgfhh.........",
    "ibjklmnh........",
    "ibjkllhhh.......",
    "dbjkolohpp......",
    "dbjkoqrsphp.....",
    "dtjkooooohhh....",
    "atjkhoosuvohw...",
    "aixgyosozusohf..",
    ".ibgeoosoolmhhn.",
    ".AbxkBooosllhCDE",
    ".AatjjFChhhpekjG",
    "..AHHxjkkkkkkjGA",
    "...AiiibbxxxxGA.",
    "......AAAAdddA..",
];
const MELON_TEMPLATE_PALETTE: [u32; 5] = [0x1E1E1E, 0x61605F, 0x494949, 0x3D3D3D, 0xA2A2A2];
const MELON_TEMPLATE: [&str; MELON_SIZE] = [
    "..aaa...........",
    ".aaa.b..........",
    ".aa.bbb.........",
    "aa..b.bb........",
    "aa..ccbbb.......",
    "aa..cccbbb......",
    "aa..cb.cbbb.....",
    "aa..cccccbbb....",
    "aa..bccd..cbb...",
    "aac.bcdce.dcbb..",
    ".ac..ccdccc.cbb.",
    ".acc..cccdcccb.b",
    ".aacc..bccccb..a",
    "..aacc........aa",
    "...aaaaccbbbbaa.",
    "......aaaaaaaa..",
];

/// Colour of one pixel, or `None` when it is transparent.
fn melon_pixel(rows: &[&str; MELON_SIZE], palette: &[u32], x: usize, y: usize) -> Option<u32> {
    let key = rows[y].as_bytes()[x];
    KEYS.iter()
        .position(|&k| k == key)
        .map(|index| palette[index])
}

/// Nearest-neighbour upscale (keeps the pixel art sharp) to a square RGBA buffer.
fn melon_rgba(rows: &[&str; MELON_SIZE], palette: &[u32], scale: usize) -> Vec<u8> {
    let side = MELON_SIZE * scale;
    let mut rgba = vec![0; side * side * 4];
    for y in 0..side {
        for x in 0..side {
            if let Some(color) = melon_pixel(rows, palette, x / scale, y / scale) {
                let i = (y * side + x) * 4;
                rgba[i..i + 3].copy_from_slice(&color.to_be_bytes()[1..]);
                rgba[i + 3] = 255;
            }
        }
    }
    rgba
}

/// The melon logo as an SVG of crisp rectangles, one per horizontal run of a colour.
fn melon_svg() -> String {
    let mut rects = String::new();
    for y in 0..MELON_SIZE {
        let mut x = 0;
        while x < MELON_SIZE {
            let Some(color) = melon_pixel(&MELON, &MELON_PALETTE, x, y) else {
                x += 1;
                continue;
            };
            let start = x;
            while x < MELON_SIZE && melon_pixel(&MELON, &MELON_PALETTE, x, y) == Some(color) {
                x += 1;
            }
            rects.push_str(&format!(
                "<rect x='{start}' y='{y}' width='{}' height='1' fill='#{color:06x}'/>",
                x - start
            ));
        }
    }
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{MELON_SIZE}' height='{MELON_SIZE}' viewBox='0 0 {MELON_SIZE} {MELON_SIZE}' shape-rendering='crispEdges'>{rects}</svg>"
    )
}

/// Kokonut's melon logo, e.g. beside the month in the calendar header.
pub fn logo<'a, Message: 'a>(size: f32) -> Element<'a, Message> {
    static HANDLE: LazyLock<svg::Handle> =
        LazyLock::new(|| svg::Handle::from_memory(melon_svg().into_bytes()));
    svg(HANDLE.clone()).width(size).height(size).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn melon_art_is_a_square_grid_with_known_palette_keys() {
        for (rows, palette) in [
            (&MELON, &MELON_PALETTE[..]),
            (&MELON_TEMPLATE, &MELON_TEMPLATE_PALETTE[..]),
        ] {
            for row in rows {
                assert_eq!(row.len(), MELON_SIZE, "{row}");
                for key in row.bytes().filter(|&k| k != b'.') {
                    let index = KEYS.iter().position(|&k| k == key).unwrap();
                    assert!(index < palette.len(), "{row}");
                }
            }
        }
    }

    #[test]
    fn tray_icons_are_32_px_rgba_with_transparent_background() {
        for pixels in [tray_pixels(), tray_template_pixels()] {
            assert_eq!(pixels.len(), TRAY_SIZE * TRAY_SIZE * 4);
            assert_eq!(pixels[3], 0, "top-left corner is transparent");
            assert!(pixels.chunks(4).any(|p| p[3] == 255));
        }
        // Pixel (2, 0) of the art is palette entry `a`, scaled 2× to (4..6, 0..2).
        let i = (TRAY_SIZE + 5) * 4;
        assert_eq!(tray_pixels()[i..i + 4], [0x4A, 0x3E, 0x00, 255]);
    }

    #[test]
    fn logo_svg_covers_every_opaque_pixel() {
        let svg = melon_svg();
        let covered: usize = svg
            .split("width='")
            .skip(2)
            .map(|part| part.split('\'').next().unwrap().parse::<usize>().unwrap())
            .sum();
        let opaque = MELON
            .iter()
            .flat_map(|row| row.bytes())
            .filter(|&k| k != b'.')
            .count();
        assert_eq!(covered, opaque);
    }
}

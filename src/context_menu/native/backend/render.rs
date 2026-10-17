//! Software rendering of the native popup menu into an ARGB buffer.

use std::sync::{LazyLock, Mutex};

use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, SwashCache};

use crate::context_menu::app::{PADDING, ROW_HEIGHT, menu_colors};

/// The visual style of the popup menu.
#[derive(Clone, Copy, Debug)]
pub struct MenuStyle {
    pub background: [u8; 4],
    pub text: [u8; 4],
    pub disabled_text: [u8; 4],
    pub hover_background: [u8; 4],
    pub hover_text: [u8; 4],
    pub row_height: f32,
    pub padding: f32,
    pub text_size: f32,
    pub radius: f32,
}

impl MenuStyle {
    /// Builds the style from the theme, matching the in-application panel.
    pub fn from_theme(theme: &iced_widget::Theme) -> Self {
        let colors = menu_colors(theme);

        let to_rgba = |color: iced_core::Color| {
            [
                (color.r * 255.0).round() as u8,
                (color.g * 255.0).round() as u8,
                (color.b * 255.0).round() as u8,
                (color.a * 255.0).round() as u8,
            ]
        };

        Self {
            background: to_rgba(colors.background),
            text: to_rgba(colors.text),
            disabled_text: to_rgba(colors.text.scale_alpha(0.38)),
            hover_background: to_rgba(colors.hover_background),
            hover_text: to_rgba(colors.hover_text),
            row_height: ROW_HEIGHT,
            padding: PADDING,
            text_size: 14.0,
            radius: 8.0,
        }
    }
}

impl MenuStyle {
    /// A plain white menu, independent of the application theme.
    ///
    /// The text widgets use this style for their native menus, which keeps
    /// them legible on any background without having to reach for the theme
    /// from the widget's event handler.
    pub fn light() -> Self {
        Self {
            background: [255, 255, 255, 255],
            text: [28, 28, 28, 255],
            disabled_text: [160, 160, 160, 255],
            hover_background: [238, 238, 238, 255],
            hover_text: [16, 16, 16, 255],
            row_height: ROW_HEIGHT,
            padding: PADDING,
            text_size: 14.0,
            radius: 8.0,
        }
    }
}

static FONT_SYSTEM: LazyLock<Mutex<FontSystem>> =
    LazyLock::new(|| Mutex::new(FontSystem::new()));

/// Packs a color as the native 32-bit word expected by the compositor for
/// `wl_shm::Format::Argb8888`: `0xAARRGGBB`, which is stored as the bytes
/// `[blue, green, red, alpha]` on a little-endian machine.
fn pack([red, green, blue, alpha]: [u8; 4]) -> u32 {
    u32::from_le_bytes([blue, green, red, alpha])
}

/// The red channel of a packed pixel.
fn red(pixel: u32) -> u8 {
    (pixel >> 16) as u8
}

/// The green channel of a packed pixel.
fn green(pixel: u32) -> u8 {
    (pixel >> 8) as u8
}

/// The blue channel of a packed pixel.
fn blue(pixel: u32) -> u8 {
    pixel as u8
}

/// The alpha channel of a packed pixel.
fn alpha(pixel: u32) -> u8 {
    (pixel >> 24) as u8
}

/// Composites a source color with the given coverage over a packed backdrop.
fn blend(backdrop: u32, source: [u8; 4]) -> u32 {
    let coverage = u32::from(source[3]);
    let backdrop_alpha = u32::from(alpha(backdrop));

    // Source-over compositing.
    let alpha = coverage + backdrop_alpha * (255 - coverage) / 255;

    let channel = |source: u8, backdrop: u8| -> u8 {
        if alpha == 0 {
            return 0;
        }

        let source = u32::from(source) * coverage;
        let backdrop =
            u32::from(backdrop) * backdrop_alpha * (255 - coverage) / 255;

        ((source + backdrop) / alpha).min(255) as u8
    };

    pack([
        channel(source[0], red(backdrop)),
        channel(source[1], green(backdrop)),
        channel(source[2], blue(backdrop)),
        alpha as u8,
    ])
}

/// Renders the menu into a buffer of ARGB8888 pixels.
pub fn render(
    width: u32,
    height: u32,
    labels: &[String],
    enabled: &[bool],
    hovered: Option<usize>,
    style: &MenuStyle,
) -> Vec<u32> {
    let mut pixels = vec![0u32; (width * height) as usize];

    let background = pack(style.background);
    let radius = style.radius.max(0.0);

    for y in 0..height {
        for x in 0..width {
            if !inside_rounded(x, y, width, height, radius) {
                continue;
            }

            pixels[(y * width + x) as usize] = background;
        }
    }

    let mut font_system = FONT_SYSTEM.lock().unwrap();
    let mut swash_cache = SwashCache::new();

    for (index, label) in labels.iter().enumerate() {
        let top = style.padding + index as f32 * style.row_height;
        let row_top = top.max(0.0) as u32;
        let row_bottom = ((top + style.row_height) as u32).min(height);

        let is_enabled = enabled.get(index).copied().unwrap_or(true);

        if hovered == Some(index) && is_enabled {
            let hover = pack(style.hover_background);

            for y in row_top..row_bottom {
                for x in style.padding as u32
                    ..width.saturating_sub(style.padding as u32)
                {
                    pixels[(y * width + x) as usize] = hover;
                }
            }
        }

        let color = if !is_enabled {
            style.disabled_text
        } else if hovered == Some(index) {
            style.hover_text
        } else {
            style.text
        };

        let mut buffer = Buffer::new(
            &mut font_system,
            Metrics::new(style.text_size, style.text_size),
        );
        buffer.set_size(Some(width as f32), Some(style.row_height));
        buffer.set_text(label, &Attrs::new(), Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);

        let origin_x = style.padding + 10.0;
        let origin_y = top + (style.row_height - style.text_size) / 2.0;

        buffer.draw(
            &mut font_system,
            &mut swash_cache,
            cosmic_text::Color::rgba(color[0], color[1], color[2], color[3]),
            |x, y, w, h, color| {
                if color.a() == 0 {
                    return;
                }

                for dy in 0..h {
                    for dx in 0..w {
                        let px = origin_x as i32 + x + dx as i32;
                        let py = origin_y as i32 + y + dy as i32;

                        if px < 0
                            || py < 0
                            || px >= width as i32
                            || py >= height as i32
                        {
                            continue;
                        }

                        let pixel = &mut pixels
                            [(py as u32 * width + px as u32) as usize];
                        let source =
                            [color.r(), color.g(), color.b(), color.a()];

                        *pixel = blend(*pixel, source);
                    }
                }
            },
        );
    }

    pixels
}

fn inside_rounded(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    radius: f32,
) -> bool {
    if radius <= 0.0 {
        return true;
    }

    let fx = x as f32 + 0.5;
    let fy = y as f32 + 0.5;

    let cx = if fx < radius {
        radius
    } else if fx > width as f32 - radius {
        width as f32 - radius
    } else {
        return true;
    };

    let cy = if fy < radius {
        radius
    } else if fy > height as f32 - radius {
        height as f32 - radius
    } else {
        return true;
    };

    let dx = fx - cx;
    let dy = fy - cy;

    dx * dx + dy * dy <= radius * radius
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A style with a color per channel, to catch swapped channels.
    fn style() -> MenuStyle {
        MenuStyle {
            background: [10, 20, 30, 255],
            text: [200, 210, 220, 255],
            disabled_text: [100, 110, 120, 255],
            hover_background: [40, 50, 60, 255],
            hover_text: [230, 240, 250, 255],
            row_height: ROW_HEIGHT,
            padding: PADDING,
            text_size: 14.0,
            radius: 0.0,
        }
    }

    fn channels(pixel: u32) -> (u8, u8, u8, u8) {
        (red(pixel), green(pixel), blue(pixel), alpha(pixel))
    }

    /// Owned labels for a menu.
    fn labels(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn pack_uses_the_argb_layout_of_the_compositor() {
        let pixel = pack([1, 2, 3, 4]);

        assert_eq!(channels(pixel), (1, 2, 3, 4));
        // The compositor reads a little-endian word: blue comes first.
        assert_eq!(pixel.to_le_bytes(), [3, 2, 1, 4]);
    }

    #[test]
    fn the_menu_is_opaque_and_uses_the_style_colors() {
        let pixels = render(32, 32, &[], &[], None, &style());

        for pixel in pixels {
            assert_eq!(channels(pixel), (10, 20, 30, 255));
        }
    }

    #[test]
    fn every_pixel_of_a_row_is_opaque() {
        let width = 200;
        let height = 64;
        let pixels = render(
            width,
            height,
            &labels(&["Cut"]),
            &[true],
            Some(0),
            &style(),
        );

        // The hovered row is fully drawn, including the antialiased text.
        for y in PADDING as u32..(PADDING + ROW_HEIGHT) as u32 {
            for x in 0..width {
                let pixel = pixels[(y * width + x) as usize];

                assert_eq!(
                    alpha(pixel),
                    255,
                    "pixel ({x}, {y}) is transparent"
                );
            }
        }

        // Away from the label, the row uses the hover color.
        assert_eq!(
            channels(pixels[(20 * width + 190) as usize]),
            (40, 50, 60, 255)
        );
    }

    #[test]
    fn a_light_native_menu_is_white() {
        let style = MenuStyle::from_theme(&iced_widget::Theme::Light);

        assert_eq!(style.background, [255, 255, 255, 255]);
    }

    #[test]
    fn a_disabled_entry_is_not_highlighted_and_uses_the_disabled_color() {
        let width = 200;
        let height = 64;
        let pixels = render(
            width,
            height,
            &labels(&["Cut"]),
            &[false],
            Some(0),
            &style(),
        );

        // The row stays on the background color; no hover highlight.
        assert_eq!(
            channels(pixels[(20 * width + 190) as usize]),
            (10, 20, 30, 255)
        );
    }

    #[test]
    fn only_the_hovered_row_is_highlighted() {
        let width = 200;
        let pixels = render(
            width,
            128,
            &labels(&["Cut", "Copy"]),
            &[true, true],
            Some(1),
            &style(),
        );

        // The first row keeps the background color.
        assert_eq!(
            channels(pixels[(20 * width + 190) as usize]),
            (10, 20, 30, 255)
        );
        // The second row is highlighted.
        assert_eq!(
            channels(pixels[(52 * width + 190) as usize]),
            (40, 50, 60, 255)
        );
    }
}

//! Native `xdg_popup` backend shared by the context menus.
//!
//! This is the Wayland fast path of `iced_systemmonitor`'s context menu,
//! ported to `iced_component`. The popup is a real `xdg_popup` surface created
//! on the Wayland connection owned by `winit`, so the compositor can draw it
//! outside of the application window and handle edge flipping for us.
//!
//! The backend is only compiled on Linux with the `native` feature. The
//! in-application panel remains the fallback whenever no Wayland window handle
//! is available or the popup cannot be created.

pub mod render;
pub mod wayland;

use iced_core::window::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};

use crate::context_menu::app::{PADDING, ROW_HEIGHT, WIDTH};

/// The width of the menu, shared with the in-application panel.
pub const fn width() -> f32 {
    WIDTH
}

/// The height a native menu with `count` entries occupies.
pub fn height(count: usize) -> f32 {
    PADDING * 2.0 + ROW_HEIGHT * count as f32
}

/// Where the menu grows from the click, so that it stays visible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    /// Downwards from the click, the usual case.
    Down,
    /// Upwards from the click, when there is no room below.
    Up,
}

/// The result of pumping native popup events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PopupEvent {
    /// Nothing happened.
    None,
    /// The user selected the entry at the given index.
    Selected(usize),
    /// The compositor dismissed the popup.
    Dismissed,
}

/// Shows a native menu, replacing any existing popup.
///
/// # Safety
///
/// `display` and `parent_surface` must be the raw `wl_display` and
/// `wl_surface` pointers of a live window created by the patched `winit`.
pub use wayland::show;

/// Pumps native popup events.
pub fn pump() -> PopupEvent {
    wayland::pump()
}

/// Hides any native popup.
pub fn hide() {
    wayland::hide();
}

pub use render::MenuStyle;

/// Extracts the raw Wayland display and surface pointers of a window.
///
/// Feed the result to
/// [`State::set_window_handle`](crate::context_menu::app::State::set_window_handle)
/// or [`Menu::set_window_handle`](crate::context_menu::native::Menu::set_window_handle)
/// to enable the native popup. The function returns [`None`] on non-Wayland
/// platforms or when the window has not been created yet.
pub fn wayland_handle<W>(window: &W) -> Option<(usize, usize)>
where
    W: HasWindowHandle + HasDisplayHandle,
{
    let display = window.display_handle().ok()?;
    let handle = window.window_handle().ok()?;

    match (display.as_raw(), handle.as_raw()) {
        (
            RawDisplayHandle::Wayland(display),
            RawWindowHandle::Wayland(handle),
        ) => Some((
            display.display.as_ptr() as usize,
            handle.surface.as_ptr() as usize,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_native_height_uses_the_shared_row_metrics() {
        assert_eq!(height(0), PADDING * 2.0);
        assert_eq!(height(3), PADDING * 2.0 + ROW_HEIGHT * 3.0);
    }

    #[test]
    fn the_native_menu_has_the_width_of_the_panel() {
        assert_eq!(width(), WIDTH);
    }
}

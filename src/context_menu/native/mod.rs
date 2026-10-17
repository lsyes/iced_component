//! Native Wayland context menus.
//!
//! [`Menu`] is a small handle that a text widget can use to show its context
//! menu as a native `xdg_popup`. The application owns one [`Menu`] per widget,
//! keeps its window handle up to date, batches [`Menu::subscription`] and
//! forwards the resulting [`Event`]s to [`Menu::update`]. The widget picks the
//! chosen entry up on its next update and performs the action.
//!
//! Without the `native` feature—or when the application does not provide a
//! Wayland window handle—[`Menu::open`] returns `false` and the widget falls
//! back to its in-application overlay menu. The whole API is therefore usable
//! unconditionally.
//!
//! # Usage
//!
//! ```no_run
//! # use iced_component::context_menu::native::{Event, Menu};
//! # use iced_widget::core::Point;
//! struct App {
//!     menu: Menu,
//! }
//!
//! #[derive(Debug, Clone)]
//! enum Message {
//!     Menu(Event),
//!     WindowHandle(Option<(usize, usize)>),
//! }
//!
//! impl App {
//!     fn update(&mut self, message: Message) {
//!         match message {
//!             Message::Menu(event) => self.menu.update(event),
//!             Message::WindowHandle(handle) => self.menu.set_window_handle(handle),
//!         }
//!     }
//!
//!     fn subscription(&self) -> iced_futures::Subscription<Message> {
//!         self.menu.subscription().map(Message::Menu)
//!     }
//! }
//! ```

#[cfg(all(feature = "native", target_os = "linux"))]
pub(crate) mod backend;

#[cfg(all(feature = "native", target_os = "linux"))]
pub use backend::wayland_handle;

use std::cell::RefCell;
use std::rc::Rc;

use iced_core::{Point, Size};
use iced_futures::Subscription;

/// What happened to the native menu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    /// Advances the native popup by one frame.
    Pump,
    /// Closes the menu without choosing anything.
    Dismiss,
}

/// A handle to one native context menu.
///
/// Keep one of these in the application state for every widget that should be
/// able to open a native menu, and pass a clone to the widget with
/// `native_menu(&menu)`. Never share one handle between widgets: the handle
/// reports the chosen entry to whichever widget takes it first.
#[derive(Clone, Debug, Default)]
pub struct Menu {
    inner: Rc<RefCell<Inner>>,
    #[cfg(test)]
    probe: Rc<RefCell<Probe>>,
}

/// Records opens without touching Wayland in unit tests.
#[cfg(test)]
#[derive(Debug, Default)]
struct Probe {
    enabled: bool,
    open: Option<(Point, Vec<(String, bool)>)>,
}

#[derive(Debug, Default)]
struct Inner {
    /// The entry chosen by the native popup, if the widget has not picked it up
    /// yet.
    pending: Option<usize>,
    /// Whether a native popup is currently on screen.
    #[cfg(all(feature = "native", target_os = "linux"))]
    native: bool,
    /// The raw Wayland display and surface pointers of the main window.
    #[cfg(all(feature = "native", target_os = "linux"))]
    handle: Option<(usize, usize)>,
    /// The size of the window, used to flip the menu near the bottom edge.
    #[cfg(all(feature = "native", target_os = "linux"))]
    window_size: Option<Size>,
}

impl Menu {
    /// Creates a closed menu handle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the raw Wayland handles used to parent the native popup.
    ///
    /// Use `wayland_handle` to capture them from the window. Without a
    /// handle—or without the `native` feature—the widget falls back to its
    /// overlay menu.
    pub fn set_window_handle(&self, handle: Option<(usize, usize)>) {
        #[cfg(all(feature = "native", target_os = "linux"))]
        {
            self.inner.borrow_mut().handle = handle;
        }

        #[cfg(not(all(feature = "native", target_os = "linux")))]
        {
            let _ = handle;
        }
    }

    /// Records the size of the window, so that the menu can flip upwards when
    /// there is not enough room below the click.
    pub fn set_window_size(&self, size: Size) {
        #[cfg(all(feature = "native", target_os = "linux"))]
        {
            self.inner.borrow_mut().window_size = Some(size);
        }

        #[cfg(not(all(feature = "native", target_os = "linux")))]
        {
            let _ = size;
        }
    }

    /// Whether a native popup is currently on screen.
    pub fn is_open(&self) -> bool {
        #[cfg(all(feature = "native", target_os = "linux"))]
        {
            self.inner.borrow().native
        }

        #[cfg(not(all(feature = "native", target_os = "linux")))]
        {
            false
        }
    }

    /// Tries to show the menu as a native popup at `position`, in window
    /// coordinates.
    ///
    /// The entries are `(label, is_enabled)` pairs. Returns `false` when no
    /// native popup could be created, in which case the caller should use its
    /// in-application menu instead.
    pub fn open(
        &self,
        position: Point,
        items: impl IntoIterator<Item = (String, bool)>,
    ) -> bool {
        let items: Vec<(String, bool)> = items.into_iter().collect();

        #[cfg(test)]
        {
            let mut probe = self.probe.borrow_mut();

            if probe.enabled {
                probe.open = Some((position, items.clone()));

                return true;
            }
        }

        #[cfg(all(feature = "native", target_os = "linux"))]
        {
            let mut inner = self.inner.borrow_mut();

            inner.pending = None;

            let Some((display, surface)) = inner.handle else {
                return false;
            };

            let labels = items
                .iter()
                .map(|(label, _)| label.clone())
                .collect::<Vec<_>>();
            let enabled = items
                .iter()
                .map(|(_, enabled)| *enabled)
                .collect::<Vec<_>>();
            let direction = direction(
                inner.window_size,
                position,
                backend::height(items.len()),
            );

            let result = unsafe {
                backend::show(
                    display as *mut std::ffi::c_void,
                    surface as *mut std::ffi::c_void,
                    position.x.round() as i32,
                    position.y.round() as i32,
                    direction,
                    labels,
                    enabled,
                    backend::render::MenuStyle::light(),
                )
            };

            match result {
                Ok(()) => {
                    inner.native = true;

                    true
                }
                Err(error) => {
                    log::warn!(
                        "failed to show the native context menu: {error}"
                    );

                    false
                }
            }
        }

        #[cfg(not(all(feature = "native", target_os = "linux")))]
        {
            let _ = (position, items);

            false
        }
    }

    /// Closes the menu without reporting a choice.
    pub fn close(&self) {
        let mut inner = self.inner.borrow_mut();

        inner.pending = None;

        #[cfg(all(feature = "native", target_os = "linux"))]
        {
            if inner.native {
                inner.native = false;
                backend::hide();
            }
        }
    }

    /// Handles an [`Event`] produced by [`Menu::subscription`].
    pub fn update(&self, event: Event) {
        match event {
            Event::Pump => {
                #[cfg(all(feature = "native", target_os = "linux"))]
                {
                    let mut inner = self.inner.borrow_mut();

                    if inner.native {
                        match backend::pump() {
                            backend::PopupEvent::Selected(index) => {
                                inner.native = false;
                                inner.pending = Some(index);
                                backend::hide();
                            }
                            backend::PopupEvent::Dismissed => {
                                inner.native = false;
                            }
                            backend::PopupEvent::None => {}
                        }
                    }
                }
            }
            Event::Dismiss => {
                #[cfg(all(feature = "native", target_os = "linux"))]
                {
                    let mut inner = self.inner.borrow_mut();

                    if inner.native {
                        inner.native = false;
                        backend::hide();
                    }
                }
            }
        }
    }

    /// Returns the entry chosen since the last call, if any.
    ///
    /// The widget calls this on its next update to perform the action.
    pub fn take_selection(&self) -> Option<usize> {
        self.inner.borrow_mut().pending.take()
    }

    /// Injects a selection for the widget tests.
    #[cfg(test)]
    pub(crate) fn set_selection(&self, index: usize) {
        self.inner.borrow_mut().pending = Some(index);
    }

    /// Makes the next open succeed without touching Wayland, for unit tests.
    #[cfg(test)]
    pub(crate) fn enable_probe(&self) {
        self.probe.borrow_mut().enabled = true;
    }

    /// Returns the position and entries of the last probed open.
    #[cfg(test)]
    pub(crate) fn probed_open(&self) -> Option<(Point, Vec<(String, bool)>)> {
        self.probe.borrow().open.clone()
    }

    /// The subscriptions the menu needs while it is open.
    pub fn subscription(&self) -> Subscription<Event> {
        if !self.is_open() {
            return Subscription::none();
        }

        #[allow(unused_mut)]
        let mut subscriptions = vec![iced_futures::event::listen_with(
            |event, _status, _window| match event {
                iced_core::Event::Mouse(
                    iced_core::mouse::Event::ButtonPressed(
                        iced_core::mouse::Button::Left,
                    ),
                ) => Some(Event::Dismiss),
                iced_core::Event::Keyboard(
                    iced_core::keyboard::Event::KeyPressed {
                        key:
                            iced_core::keyboard::Key::Named(
                                iced_core::keyboard::key::Named::Escape,
                            ),
                        ..
                    },
                ) => Some(Event::Dismiss),
                iced_core::Event::Window(
                    iced_core::window::Event::Unfocused,
                ) => Some(Event::Dismiss),
                _ => None,
            },
        )];

        #[cfg(all(feature = "native", target_os = "linux"))]
        if self.is_open() {
            subscriptions.push(pump_subscription());
        }

        Subscription::batch(subscriptions)
    }
}

/// The direction the menu has to grow in to stay inside the window.
#[cfg(all(feature = "native", target_os = "linux"))]
fn direction(
    window_size: Option<Size>,
    position: Point,
    height: f32,
) -> backend::Direction {
    let fits_below =
        window_size.is_none_or(|size| position.y + height <= size.height);

    if fits_below {
        backend::Direction::Down
    } else {
        backend::Direction::Up
    }
}

/// The stream that advances a native popup.
#[cfg(all(feature = "native", target_os = "linux"))]
fn pump_subscription() -> Subscription<Event> {
    Subscription::run(|| {
        iced_futures::futures::stream::unfold((), |()| async {
            futures_timer::Delay::new(std::time::Duration::from_millis(80))
                .await;

            Some((Event::Pump, ()))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_without_a_handle_does_not_open_natively() {
        let menu = Menu::new();

        assert!(
            !menu.open(Point::new(10.0, 10.0), [("Cut".to_string(), true)])
        );
        assert!(!menu.is_open());
        assert_eq!(menu.take_selection(), None);
    }

    #[test]
    fn dropping_the_handle_does_not_report_a_selection() {
        let menu = Menu::new();
        menu.set_window_size(Size::new(800.0, 600.0));
        menu.close();

        assert_eq!(menu.take_selection(), None);
    }
}

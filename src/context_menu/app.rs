//! An application-managed context menu.
//!
//! This is a port of the `context_menu` widget of
//! [`iced_systemmonitor`](https://github.com/lsyes/iced_systemmonitor) that
//! works on top of the public `iced` APIs. Instead of owning its entries as
//! closures, the menu is described by a list of labels and reports the index of
//! the chosen entry, which makes it convenient to store in the application
//! state.
//!
//! On Linux with the `native` feature, the menu is shown as a real Wayland
//! `xdg_popup` when the application hands the window handles to
//! `State::set_window_handle`; otherwise—or when the popup cannot be
//! created—it uses the in-application panel. Parenting the popup needs the
//! window's `xdg_surface`, which `winit` only exposes through the patch
//! vendored by this crate, so applications that enable `native` must add the
//! same `[patch]` section to their own `Cargo.toml`.
//!
//! # Usage
//!
//! 1. Keep a [`State`] in the application state.
//! 2. Describe the entries with [`State::set_items`].
//! 3. Wrap any widget with [`State::wrap`] to make it open the menu on
//!    right-click.
//! 4. Extend the root [`Stack`](iced_widget::Stack) with [`State::layers`].
//! 5. Batch [`State::subscription`] and forward [`Event`]s to
//!    [`State::update`], which reports the index of the chosen entry.
//!
//! ```no_run
//! use iced_component::context_menu::app::{Event, State};
//! use iced_widget::{Element, Widget, stack, text};
//!
//! #[derive(Debug, Clone)]
//! enum Message {
//!     Menu(Event),
//! }
//!
//! struct App {
//!     menu: State,
//! }
//!
//! impl App {
//!     fn update(&mut self, message: Message) {
//!         let Message::Menu(event) = message;
//!
//!         if let Some(index) = self.menu.update(event, &iced_widget::Theme::Light) {
//!             println!("chose entry {index}");
//!         }
//!     }
//!
//!     fn view(&self) -> Element<'_, Message> {
//!         let content = self.menu.wrap(text("Right-click me"), Message::Menu);
//!
//!         let mut layers: Vec<Element<'_, Message>> = vec![content];
//!         layers.extend(self.menu.layers(Message::Menu));
//!
//!         stack(layers).boxed()
//!     }
//! }
//! ```
//!
//! # Coordinates
//!
//! The menu is positioned in window coordinates, because it is drawn by a
//! layer of the root [`Stack`](iced_widget::Stack), outside of the widget that
//! opened it. Ancestors that translate their contents—like a
//! [`Scrollable`](iced_widget::Scrollable) or a
//! [`Float`](iced_widget::Float)—hand the cursor down in their content space,
//! so the position a widget would record naively is not the position of the
//! pointer in the window. The wrapper remembers the raw pointer position that
//! the runtime reports for pointer movement, which is always expressed in
//! window coordinates, and opens the menu there.
//!
//! This is the same idea as the translation handling of the overlay-based
//! [`context_menu`](super) widget: no matter how deeply the widget is nested,
//! the menu ends up underneath the pointer.

use std::marker::PhantomData;

use iced_core::keyboard;
use iced_core::mouse;
use iced_core::widget::tree::{self, Tree};
use iced_core::window;
use iced_core::{
    Border, Color, Event as CoreEvent, Layout, Length, Padding, Point,
    Rectangle, Shadow, Shell, Size, Vector, Widget as CoreWidget, alignment,
    layout, overlay, renderer,
};
use iced_futures::Subscription;
use iced_widget::{
    Column, Space, Theme, Widget as _, button, container, mouse_area, opaque,
    text,
};

#[cfg(all(feature = "native", target_os = "linux"))]
pub use super::native::wayland_handle;

#[cfg(all(feature = "native", target_os = "linux"))]
use super::native::backend;

/// The width of the menu.
pub const WIDTH: f32 = 190.0;

/// The height of a single entry.
pub const ROW_HEIGHT: f32 = 32.0;

/// The padding above the first and below the last entry.
pub const PADDING: f32 = 6.0;

/// The spacing between two entries.
pub const SPACING: f32 = 2.0;

/// The height a menu with `count` entries occupies.
pub fn height(count: usize) -> f32 {
    PADDING * 2.0
        + ROW_HEIGHT * count as f32
        + SPACING * count.saturating_sub(1) as f32
}

/// What happened to the context menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Opens the menu at a position in window coordinates.
    Open(Point),
    /// The entry at the given index was chosen.
    Select(usize),
    /// Closes the menu without choosing anything.
    Dismiss,
    /// Advances the native popup by one frame.
    ///
    /// The menu emits this event through [`State::subscription`] while a
    /// native popup is open. It is a no-op without the `native` feature.
    Pump,
}

/// The state of a context menu.
#[derive(Debug, Default)]
pub struct State {
    /// Where the in-application panel is shown, in window coordinates, if it is
    /// open.
    ///
    /// It is [`None`] while a native popup is in use.
    position: Option<Point>,
    /// The labels of the entries, top to bottom.
    items: Vec<String>,
    /// The size of the window, used to keep the menu inside of it.
    window_size: Option<Size>,
    /// Whether a native popup is currently on screen.
    #[cfg(all(feature = "native", target_os = "linux"))]
    native: bool,
    /// The raw Wayland display and surface pointers of the main window.
    #[cfg(all(feature = "native", target_os = "linux"))]
    handle: Option<(usize, usize)>,
}

impl State {
    /// Creates an empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the entries shown by the menu, top to bottom.
    pub fn set_items(&mut self, items: impl IntoIterator<Item = String>) {
        self.items = items.into_iter().collect();
    }

    /// Returns the labels of the entries, top to bottom.
    pub fn items(&self) -> &[String] {
        &self.items
    }

    /// Records the size of the window, so that the menu can be flipped when
    /// there is not enough room below the click.
    ///
    /// Applications should keep this up to date with
    /// [`iced::window::resize_events`](https://docs.rs/iced/latest/iced/window/fn.resize_events.html).
    pub fn set_window_size(&mut self, size: Size) {
        self.window_size = Some(size);
    }

    /// Records the raw Wayland handles used to parent the native popup.
    ///
    /// Use [`wayland_handle`] to capture them from the window. Without a
    /// handle—or without the `native` feature—the menu uses the
    /// in-application panel instead.
    #[cfg(all(feature = "native", target_os = "linux"))]
    pub fn set_window_handle(&mut self, handle: Option<(usize, usize)>) {
        self.handle = handle;
    }

    /// Whether the menu is currently visible, natively or in-application.
    pub fn is_open(&self) -> bool {
        self.position.is_some() || self.is_native()
    }

    /// Returns where the in-application panel is shown, in window coordinates,
    /// if it is open.
    pub fn position(&self) -> Option<Point> {
        self.position
    }

    /// Opens the in-application panel at the given position, in window
    /// coordinates.
    pub fn open(&mut self, position: Point) {
        self.close();
        self.position = Some(position);
    }

    /// Closes the menu and destroys its native popup, if it has one.
    pub fn close(&mut self) {
        self.position = None;

        #[cfg(all(feature = "native", target_os = "linux"))]
        {
            if self.native {
                self.native = false;
                backend::hide();
            }
        }
    }

    /// Handles an [`Event`], returning the index of the chosen entry.
    ///
    /// `theme` is used to style the native popup; it is unused when the
    /// `native` feature is disabled.
    pub fn update(&mut self, event: Event, _theme: &Theme) -> Option<usize> {
        match event {
            Event::Open(position) => {
                self.close();

                #[cfg(all(feature = "native", target_os = "linux"))]
                if self.open_native(position, _theme) {
                    return None;
                }

                self.position = Some(position);

                None
            }
            Event::Select(index) => self.choose(index),
            Event::Dismiss => {
                self.close();

                None
            }
            Event::Pump => {
                #[cfg(all(feature = "native", target_os = "linux"))]
                {
                    if self.native {
                        match backend::pump() {
                            backend::PopupEvent::Selected(index) => {
                                return self.choose(index);
                            }
                            backend::PopupEvent::Dismissed => self.close(),
                            backend::PopupEvent::None => {}
                        }
                    }
                }

                None
            }
        }
    }

    /// Closes the menu and reports the chosen entry, if it is still valid.
    fn choose(&mut self, index: usize) -> Option<usize> {
        self.close();

        (index < self.items.len()).then_some(index)
    }

    /// Whether a native popup is currently on screen.
    #[cfg(all(feature = "native", target_os = "linux"))]
    fn is_native(&self) -> bool {
        self.native
    }

    /// Whether a native popup is currently on screen.
    #[cfg(not(all(feature = "native", target_os = "linux")))]
    fn is_native(&self) -> bool {
        false
    }

    /// Tries to show the menu as a native Wayland popup.
    #[cfg(all(feature = "native", target_os = "linux"))]
    fn open_native(&mut self, position: Point, theme: &Theme) -> bool {
        let Some((display, surface)) = self.handle else {
            return false;
        };

        let direction = match self
            .direction_with(position, backend::height(self.items.len()))
        {
            Direction::Down => backend::Direction::Down,
            Direction::Up => backend::Direction::Up,
        };

        let result = unsafe {
            backend::show(
                display as *mut std::ffi::c_void,
                surface as *mut std::ffi::c_void,
                position.x.round() as i32,
                position.y.round() as i32,
                direction,
                self.items.clone(),
                vec![true; self.items.len()],
                backend::MenuStyle::from_theme(theme),
            )
        };

        match result {
            Ok(()) => {
                self.native = true;

                true
            }
            Err(error) => {
                log::warn!("failed to show the native context menu: {error}");

                false
            }
        }
    }

    /// The height of the menu with the current entries.
    fn menu_height(&self) -> f32 {
        #[cfg(all(feature = "native", target_os = "linux"))]
        if self.native {
            return backend::height(self.items.len());
        }

        height(self.items.len())
    }

    /// The direction the menu has to grow in to stay inside the window.
    fn direction(&self, position: Point) -> Direction {
        self.direction_with(position, self.menu_height())
    }

    /// The direction a menu of the given height has to grow in.
    fn direction_with(&self, position: Point, menu_height: f32) -> Direction {
        let fits_below = self
            .window_size
            .is_none_or(|size| position.y + menu_height <= size.height);

        if fits_below {
            Direction::Down
        } else {
            Direction::Up
        }
    }

    /// The corner of the panel, relative to the click, in window coordinates.
    fn panel_offset(&self, position: Point) -> Point {
        let mut offset = position;

        if self.direction(position) == Direction::Up {
            offset.y -= self.menu_height();
        }

        if let Some(size) = self.window_size {
            offset.x = offset.x.min((size.width - WIDTH).max(0.0));
            offset.y = offset
                .y
                .max(0.0)
                .min((size.height - self.menu_height()).max(0.0));
        }

        offset
    }

    /// The subscriptions the menu needs while it is open.
    pub fn subscription(&self) -> Subscription<Event> {
        if !self.is_open() {
            return Subscription::none();
        }

        #[allow(unused_mut)]
        let mut subscriptions = vec![iced_futures::event::listen_with(
            |event, _status, _window| match event {
                // A left click anywhere dismisses the menu. `listen` would only
                // report the events no widget handled, which is precisely not the
                // case here: clicking a row or a navigation button has to close the
                // menu as well.
                CoreEvent::Mouse(mouse::Event::ButtonPressed(
                    mouse::Button::Left,
                )) => Some(Event::Dismiss),
                CoreEvent::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(keyboard::key::Named::Escape),
                    ..
                }) => Some(Event::Dismiss),
                CoreEvent::Window(window::Event::Unfocused) => {
                    Some(Event::Dismiss)
                }
                _ => None,
            },
        )];

        #[cfg(all(feature = "native", target_os = "linux"))]
        if self.native {
            subscriptions.push(pump_subscription());
        }

        Subscription::batch(subscriptions)
    }

    /// The layers that draw the menu.
    ///
    /// Empty unless the in-application panel is open; append them to the root
    /// [`Stack`](iced_widget::Stack). A click on an entry is reported back as
    /// [`Event::Select`], and a click on the backdrop as [`Event::Dismiss`].
    pub fn layers<'a, Message, Renderer>(
        &'a self,
        on_event: impl Fn(Event) -> Message + 'a,
    ) -> Vec<iced_core::Element<'a, Message, Theme, Renderer>>
    where
        Message: Clone + 'a,
        Renderer: iced_core::text::Renderer + 'a,
    {
        let Some(position) = self.position else {
            return Vec::new();
        };

        // Keep the panel inside of the window, flipping it upwards when there
        // is no room below the click.
        let position = self.panel_offset(position);

        let items: Vec<iced_core::Element<'a, Message, Theme, Renderer>> = self
            .items
            .iter()
            .enumerate()
            .map(|(index, label)| {
                button(text(label.clone()).size(14.0))
                    .width(Length::Fill)
                    .height(Length::Fixed(ROW_HEIGHT))
                    .padding(Padding {
                        top: 0.0,
                        right: 12.0,
                        bottom: 0.0,
                        left: 12.0,
                    })
                    .style(item_style)
                    .on_press(on_event(Event::Select(index)))
                    .boxed()
            })
            .collect();

        let panel = container(Column::with_children(items).spacing(SPACING))
            .width(Length::Fixed(WIDTH))
            .padding(PADDING)
            .style(panel_style);

        vec![
            // Clicking the backdrop closes the menu before it reaches anything
            // underneath.
            opaque(
                mouse_area(
                    Space::new().width(Length::Fill).height(Length::Fill),
                )
                .on_press(on_event(Event::Dismiss)),
            )
            .boxed(),
            container(panel)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(alignment::Horizontal::Left)
                .align_y(alignment::Vertical::Top)
                .padding(Padding {
                    top: position.y,
                    left: position.x,
                    right: 0.0,
                    bottom: 0.0,
                })
                .boxed(),
        ]
    }

    /// Makes `content` open the menu when it is right-clicked.
    pub fn wrap<'a, Message, Renderer, W>(
        &self,
        content: W,
        on_event: impl Fn(Event) -> Message + 'a,
    ) -> iced_core::Element<'a, Message, Theme, Renderer>
    where
        Message: Clone + 'a,
        Renderer: iced_core::text::Renderer + 'a,
        W: CoreWidget<Message, Theme, Renderer> + 'a,
    {
        ContextMenuArea {
            content,
            on_event,
            _lifetime: PhantomData,
        }
        .boxed()
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

/// The direction the menu grows from the click.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Direction {
    /// Downwards from the click, the usual case.
    Down,
    /// Upwards from the click, when there is no room below.
    Up,
}

/// The wrapper that turns a right click into an [`Event::Open`].
struct ContextMenuArea<'a, Message, W, F> {
    content: W,
    on_event: F,
    _lifetime: PhantomData<&'a Message>,
}

/// The state stored in the widget tree of [`ContextMenuArea`].
#[derive(Debug, Default)]
struct WrapperState {
    /// The last pointer position reported by a raw pointer event, in window
    /// coordinates.
    ///
    /// Ancestors like a [`Scrollable`](iced_widget::Scrollable) translate the
    /// cursor into their content space before handing it to their children,
    /// while the event itself keeps reporting window coordinates. Remembering
    /// the raw position lets the menu open underneath the pointer no matter how
    /// the content is translated.
    pointer_position: Option<Point>,
}

impl<Message, W, F> iced_core::widget::Meta
    for ContextMenuArea<'_, Message, W, F>
{
}

impl<Message, Renderer, W, F> CoreWidget<Message, Theme, Renderer>
    for ContextMenuArea<'_, Message, W, F>
where
    Message: Clone,
    Renderer: iced_core::text::Renderer,
    W: CoreWidget<Message, Theme, Renderer>,
    F: Fn(Event) -> Message,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<WrapperState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(WrapperState::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) {
        self.content.layout(&mut tree.children[0], renderer, limits);

        tree.size = tree.children[0].size;
        tree.children[0].translation = Vector::ZERO;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn iced_core::widget::Operation,
    ) {
        self.content.operate(
            &mut tree.children[0],
            layout,
            viewport,
            renderer,
            operation,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &CoreEvent,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // Remember the pointer in window coordinates before the content sees
        // the event. This is the position the menu has to open at; the cursor
        // may have been translated by an ancestor.
        if let Some(position) = super::pointer_position(event) {
            tree.state.downcast_mut::<WrapperState>().pointer_position =
                Some(position);
        }

        self.content.update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            shell,
            viewport,
        );

        if let CoreEvent::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Right,
        )) = event
            && let Some(local) = cursor.observe().position_over(layout.bounds())
        {
            let position = tree
                .state
                .downcast_ref::<WrapperState>()
                .pointer_position
                .unwrap_or(local);

            shell.publish((self.on_event)(Event::Open(position)));
            shell.capture_event();
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}

/// The colors of the menu, shared by the in-application panel and the native
/// popup.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MenuColors {
    pub background: Color,
    pub text: Color,
    pub hover_background: Color,
    pub hover_text: Color,
    pub border: Color,
}

/// The colors of the menu for the given theme.
///
/// A light theme gets a plain white card—like the overlay-based
/// [`ContextMenu`](super::ContextMenu)—instead of the grayish weak background
/// of the theme. A dark theme keeps its dark card and its light text.
pub(crate) fn menu_colors(theme: &Theme) -> MenuColors {
    let palette = theme.palette();

    if palette.is_dark {
        MenuColors {
            background: palette.background.weak.color,
            text: palette.background.weak.text,
            hover_background: palette.background.strong.color,
            hover_text: palette.background.strong.text,
            border: Color::from_rgba(1.0, 1.0, 1.0, 0.12),
        }
    } else {
        MenuColors {
            background: Color::WHITE,
            text: palette.background.base.text,
            hover_background: palette.background.weak.color,
            hover_text: palette.background.weak.text,
            border: Color::from_rgba(0.0, 0.0, 0.0, 0.12),
        }
    }
}

/// The style of the menu panel.
fn panel_style(theme: &Theme) -> container::Style {
    let colors = menu_colors(theme);

    container::Style {
        text_color: Some(colors.text),
        background: Some(colors.background.into()),
        border: Border {
            width: 1.0,
            radius: 8.0.into(),
            color: colors.border,
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.25),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 12.0,
        },
        ..container::Style::default()
    }
}

/// The style of an entry.
fn item_style(theme: &Theme, status: button::Status) -> button::Style {
    let colors = menu_colors(theme);
    let palette = theme.palette();

    let mut style = button::Style {
        background: None,
        text_color: colors.text,
        border: Border {
            radius: 6.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(colors.hover_background.into());
            style.text_color = colors.hover_text;
        }
        button::Status::Pressed => {
            style.background = Some(palette.primary.base.color.into());
            style.text_color = palette.primary.base.text;
        }
        _ => {}
    }

    style
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened() -> State {
        let mut state = State::new();
        state.set_items(["One".to_string(), "Two".to_string()]);
        state.update(Event::Open(Point::new(10.0, 20.0)), &Theme::Light);

        state
    }

    #[test]
    fn the_menu_opens_at_the_requested_position() {
        let state = opened();

        assert!(state.is_open());
        assert_eq!(state.position(), Some(Point::new(10.0, 20.0)));
    }

    #[test]
    fn the_menu_draws_a_backdrop_and_the_entries() {
        let state = opened();

        assert_eq!(state.layers::<(), ()>(|_| ()).len(), 2);
    }

    #[test]
    fn choosing_an_entry_reports_its_index_and_closes() {
        let mut state = opened();

        assert_eq!(state.update(Event::Select(1), &Theme::Light), Some(1));
        assert!(!state.is_open());
    }

    #[test]
    fn dismissing_closes_without_a_choice() {
        let mut state = opened();

        assert_eq!(state.update(Event::Dismiss, &Theme::Light), None);
        assert!(!state.is_open());
    }

    fn positioned() -> State {
        let mut state = State::new();
        state.set_items(["One".to_string(), "Two".to_string()]);
        state.set_window_size(Size::new(800.0, 600.0));

        state
    }

    #[test]
    fn the_menu_grows_downwards_when_there_is_room_below() {
        let state = positioned();

        assert_eq!(state.direction(Point::new(100.0, 100.0)), Direction::Down);
    }

    #[test]
    fn the_menu_grows_upwards_when_it_would_not_fit_below() {
        let state = positioned();

        assert_eq!(state.direction(Point::new(100.0, 550.0)), Direction::Up);
    }

    #[test]
    fn the_menu_grows_downwards_without_a_known_window_size() {
        let mut state = State::new();
        state.set_items(["One".to_string()]);

        assert_eq!(state.direction(Point::new(100.0, 5000.0)), Direction::Down);
    }

    #[test]
    fn an_upwards_menu_ends_at_the_click() {
        let state = positioned();
        let offset = state.panel_offset(Point::new(100.0, 550.0));

        assert_eq!(offset.y, 550.0 - state.menu_height());
    }

    #[test]
    fn the_panel_is_kept_inside_of_the_window() {
        let state = positioned();

        let offset = state.panel_offset(Point::new(790.0, 590.0));

        assert!(offset.x + WIDTH <= 800.0);
        assert!(offset.y + state.menu_height() <= 600.0);
        assert!(offset.x >= 0.0);
    }

    #[test]
    fn a_light_menu_is_white() {
        assert_eq!(menu_colors(&Theme::Light).background, Color::WHITE);
    }

    #[test]
    fn a_dark_menu_keeps_its_dark_card() {
        assert_ne!(menu_colors(&Theme::Dark).background, Color::WHITE);
    }

    #[test]
    fn a_tiny_window_does_not_push_the_panel_negative() {
        let mut state = State::new();
        state.set_items(["One".to_string(), "Two".to_string()]);
        state.set_window_size(Size::new(100.0, 50.0));

        let offset = state.panel_offset(Point::new(90.0, 40.0));

        assert_eq!(offset.x, 0.0);
        assert_eq!(offset.y, 0.0);
    }
}

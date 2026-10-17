//! A text input with a right-click context menu.
//!
//! [`ContextMenuTextInput`] is a drop-in replacement for
//! [`iced_widget::text_input::TextInput`] that opens a Cut / Copy / Paste /
//! Select All menu when the user right-clicks it.
//!
//! # Example
//!
//! ```no_run
//! use iced_component::text_input::context_menu_text_input;
//!
//! type Element<'a, Message> =
//!     iced_widget::core::Element<'a, Message, iced_widget::Theme, iced_widget::Renderer>;
//!
//! #[derive(Debug, Clone)]
//! enum Message {
//!     ContentChanged(String),
//! }
//!
//! fn view(value: &str) -> Element<'_, Message> {
//!     context_menu_text_input("Type something...", value)
//!         .on_input(Message::ContentChanged)
//!         .into()
//! }
//! ```
//!
//! # Design
//!
//! The editing engine is the one the built-in [`TextInput`] itself uses:
//! [`text::Input`] owns the [`Editor`], the focus and the placeholder of a
//! field, and knows how to lay it out, feed it events and paint it. This widget
//! is a thin wrapper around that engine, so selection, dragging, undo, IME,
//! secure input and clipboard integration behave *exactly* like they do in the
//! standard widget, instead of being reimplemented here.
//!
//! Only two things are added on top:
//!
//! 1. **A right-click opens the shared [`crate::context_menu`]**,
//!    which is a widget of its own. On an unfocused input, the click is
//!    replayed as a left click so that the caret lands exactly where iced would
//!    have put it; a focused input is left alone, so that right-clicking a
//!    selection keeps it.
//! 2. **A chosen entry feeds a synthetic editor [`Binding`] to the engine** (for
//!    instance `Cut` for *Cut*). Because the engine handles that binding itself,
//!    the action reuses iced's own editing and clipboard plumbing—including the
//!    `on_input` / `on_paste` messages—rather than duplicating it.
//!
//! # Native menus
//!
//! On Linux with the `native` feature, pass an application-owned
//! [`context_menu::native::Menu`] with
//! [`native_menu`](ContextMenuTextInput::native_menu) to draw the menu as a
//! native Wayland `xdg_popup`. Keep one menu per input, feed it the window
//! handle and window size, and forward
//! [`context_menu::native::Event`]s to it.
//! Without a handle—or without the feature—the input keeps using its overlay
//! menu.
//!
//! [`TextInput`]: iced_widget::text_input::TextInput
//! [`Editor`]: iced_core::text::Editor
//! [`Binding`]: iced_core::text::editor::Binding

use std::borrow::Cow;

use iced_core::keyboard;
use iced_core::mouse;
use iced_core::text;
use iced_core::text::editor::Binding;
use iced_core::widget::operation::Focusable as _;
use iced_core::widget::tree::{self, Tree};
use iced_core::window;
use iced_core::{
    Element, Event, Font, Layout, Length, Padding, Pixels, Point, Rectangle,
    Shell, Size, Vector, Widget, renderer,
};
use iced_widget::text_input::{self, Status, Style};

use crate::context_menu::native::Menu as NativeMenu;
use crate::context_menu::{self, Entry, Menu};

/// The default [`Padding`] of a [`ContextMenuTextInput`].
pub const DEFAULT_PADDING: Padding = text_input::DEFAULT_PADDING;

/// A text input with a right-click context menu.
///
/// It offers the same builder API as [`TextInput`], plus
/// [`context_menu`](Self::context_menu) and [`menu_style`](Self::menu_style).
///
/// [`TextInput`]: iced_widget::text_input::TextInput
pub struct ContextMenuTextInput<'a, Message, Theme = iced_widget::Theme>
where
    Theme: text_input::Catalog + context_menu::Catalog,
{
    id: Option<iced_core::widget::Id>,
    placeholder: Cow<'a, str>,
    value: Cow<'a, str>,
    entries: Vec<Entry<'a, Message>>,
    on_input: Option<Box<dyn Fn(String) -> Message + 'a>>,
    on_paste: Option<Box<dyn Fn(String) -> Message + 'a>>,
    on_submit: Option<Message>,
    is_secure: bool,
    has_context_menu: bool,
    font: Option<Font>,
    width: Length,
    height: Length,
    padding: Padding,
    size: Option<Pixels>,
    line_height: Option<text::LineHeight>,
    alignment: text::Alignment,
    class: <Theme as text_input::Catalog>::Class<'a>,
    menu_class: Option<<Theme as context_menu::Catalog>::Class<'a>>,
    native_menu: Option<NativeMenu>,
    last_status: Option<Status>,
}

impl<'a, Message, Theme> ContextMenuTextInput<'a, Message, Theme>
where
    Message: Clone,
    Theme: text_input::Catalog + context_menu::Catalog,
{
    /// Creates a new [`ContextMenuTextInput`] with the given placeholder and
    /// its current value.
    pub fn new(
        placeholder: impl Into<Cow<'a, str>>,
        value: impl Into<Cow<'a, str>>,
    ) -> Self {
        Self {
            id: None,
            placeholder: placeholder.into(),
            value: value.into(),
            entries: Vec::new(),
            on_input: None,
            on_paste: None,
            on_submit: None,
            is_secure: false,
            has_context_menu: true,
            font: None,
            width: Length::Fill,
            height: Length::Fit,
            padding: DEFAULT_PADDING,
            size: None,
            line_height: None,
            alignment: text::Alignment::Default,
            class: <Theme as text_input::Catalog>::default(),
            menu_class: None,
            native_menu: None,
            last_status: None,
        }
    }

    /// Sets the [`iced_core::widget::Id`] of the input.
    pub fn id(mut self, id: impl Into<iced_core::widget::Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Converts the input into a secure password input.
    ///
    /// *Cut* and *Copy* are disabled while it is secure, just like the
    /// corresponding shortcuts are in the built-in widget.
    pub fn secure(mut self, is_secure: bool) -> Self {
        self.is_secure = is_secure;
        self
    }

    /// Enables or disables the right-click context menu.
    ///
    /// It is enabled by default. When disabled, right-clicks are ignored
    /// exactly like they are by [`TextInput`].
    ///
    /// [`TextInput`]: iced_widget::text_input::TextInput
    pub fn context_menu(mut self, context_menu: bool) -> Self {
        self.has_context_menu = context_menu;
        self
    }

    /// Sets the message that should be produced when some text is typed into
    /// the input.
    ///
    /// If this method is not called, the input will be disabled—and so will its
    /// context menu.
    pub fn on_input(
        mut self,
        on_input: impl Fn(String) -> Message + 'a,
    ) -> Self {
        self.on_input = Some(Box::new(on_input));
        self
    }

    /// Sets the message that should be produced when the input is focused and
    /// the enter key is pressed.
    pub fn on_submit(mut self, message: Message) -> Self {
        self.on_submit = Some(message);
        self
    }

    /// Sets the message that should be produced when some text is pasted into
    /// the input.
    pub fn on_paste(
        mut self,
        on_paste: impl Fn(String) -> Message + 'a,
    ) -> Self {
        self.on_paste = Some(Box::new(on_paste));
        self
    }

    /// Sets the font of the input.
    pub fn font(mut self, font: impl Into<Font>) -> Self {
        self.font = Some(font.into());
        self
    }

    /// Sets the width of the input.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the input.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the [`Padding`] of the input.
    pub fn padding<P: Into<Padding>>(mut self, padding: P) -> Self {
        self.padding = padding.into();
        self
    }

    /// Sets the text size of the input.
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// Sets the [`text::LineHeight`] of the input.
    pub fn line_height(
        mut self,
        line_height: impl Into<text::LineHeight>,
    ) -> Self {
        self.line_height = Some(line_height.into());
        self
    }

    /// Sets the horizontal alignment of the input.
    pub fn align_x(mut self, alignment: impl Into<text::Alignment>) -> Self {
        self.alignment = alignment.into();
        self
    }

    /// Sets the style of the input.
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self
    where
        <Theme as text_input::Catalog>::Class<'a>:
            From<text_input::StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as text_input::StyleFn<'a, Theme>).into();
        self
    }

    /// Sets the style class of the input.
    #[must_use]
    pub fn class(
        mut self,
        class: impl Into<<Theme as text_input::Catalog>::Class<'a>>,
    ) -> Self {
        self.class = class.into();
        self
    }

    /// Sets the style of the context menu.
    ///
    /// By default the menu reuses the default style of the [`Theme`](iced_widget::Theme).
    #[must_use]
    pub fn menu_style(
        mut self,
        style: impl Fn(&Theme) -> context_menu::Style + 'a,
    ) -> Self
    where
        <Theme as context_menu::Catalog>::Class<'a>:
            From<context_menu::StyleFn<'a, Theme>>,
    {
        self.menu_class =
            Some((Box::new(style) as context_menu::StyleFn<'a, Theme>).into());
        self
    }

    /// Uses a native Wayland popup for the context menu when possible.
    ///
    /// Keep one [`Menu`](crate::context_menu::native::Menu) per input in the
    /// application state, feed it the window handle with
    /// [`set_window_handle`](crate::context_menu::native::Menu::set_window_handle),
    /// and forward [`Event`](crate::context_menu::native::Event)s to
    /// [`update`](crate::context_menu::native::Menu::update). If no handle is
    /// available—or the `native` feature is disabled—the input falls back to
    /// its in-application menu.
    pub fn native_menu(mut self, menu: &NativeMenu) -> Self {
        self.native_menu = Some(menu.clone());
        self
    }
}

/// Creates a new [`ContextMenuTextInput`].
pub fn context_menu_text_input<'a, Message, Theme>(
    placeholder: impl Into<Cow<'a, str>>,
    value: impl Into<Cow<'a, str>>,
) -> ContextMenuTextInput<'a, Message, Theme>
where
    Message: Clone,
    Theme: text_input::Catalog + context_menu::Catalog,
{
    ContextMenuTextInput::new(placeholder, value)
}

/// The persistent state of a [`ContextMenuTextInput`].
struct State<R: text::Renderer> {
    input: text::Input<R>,
    value: String,
    menu: context_menu::State,
    /// The last pointer position in window coordinates.
    pointer_position: Option<Point>,
}

impl<R: text::Renderer> Default for State<R> {
    fn default() -> Self {
        Self {
            input: text::Input::new(),
            value: String::new(),
            menu: context_menu::State::new(),
            pointer_position: None,
        }
    }
}

impl<Message, Theme> iced_core::widget::Meta
    for ContextMenuTextInput<'_, Message, Theme>
where
    Theme: text_input::Catalog + context_menu::Catalog,
{
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for ContextMenuTextInput<'_, Message, Theme>
where
    Message: Clone,
    Theme: text_input::Catalog + context_menu::Catalog,
    Renderer: text::Renderer + 'static,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Renderer>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<Renderer>::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &iced_core::layout::Limits,
    ) {
        let state = tree.state.downcast_mut::<State<Renderer>>();

        if state.value != self.value.as_ref() {
            state.input.overwrite(&self.value);
            state.value = self.value.to_string();
        }

        tree.size = state.input.layout(
            renderer,
            limits,
            text::input::Layout {
                width: self.width,
                height: self.height,
                padding: self.padding,
                placeholder: &self.placeholder,
                font: self.font,
                size: self.size,
                line_height: self.line_height,
                alignment: self.alignment,
                multiline: None,
                is_secure: self.is_secure,
            },
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        _viewport: &Rectangle,
        _renderer: &Renderer,
        operation: &mut dyn iced_core::widget::Operation,
    ) {
        let state = tree.state.downcast_mut::<State<Renderer>>();
        let bounds = layout.bounds();

        operation.text_input(self.id.as_ref(), bounds, &mut state.input);
        operation.focusable(self.id.as_ref(), bounds, &mut state.input);
    }

    #[allow(clippy::too_many_arguments)]
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State<Renderer>>();
        let bounds = layout.bounds();

        if let Some(position) = crate::context_menu::pointer_position(event) {
            state.pointer_position = Some(position);
        }

        if let Some(native) = &self.native_menu
            && let Some(index) = native.take_selection()
        {
            perform_native_selection(self, index, state, bounds, shell);
        }

        // A right-click opens the menu. On an unfocused input, replaying it as
        // a left click makes the engine focus itself and drop the caret right
        // where the user clicked, without this wrapper having to know anything
        // about text layout. A focused input is left alone, so that
        // right-clicking a selection keeps it.
        if self.has_context_menu
            && self.on_input.is_some()
            && let Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right,
            )) = event
            && let Some(position) = cursor.observe().position_over(bounds)
        {
            if !state.input.is_focused() {
                for click in [
                    mouse::Event::ButtonPressed(mouse::Button::Left),
                    mouse::Event::ButtonReleased(mouse::Button::Left),
                ] {
                    let _ = state.input.update(
                        &Event::Mouse(click),
                        bounds,
                        cursor,
                        shell,
                        |key_press| Binding::from_key_press(key_press),
                    );
                }
            }

            let mut items: Vec<(String, bool)> =
                Vec::with_capacity(Action::ALL.len());

            self.entries = Action::ALL
                .iter()
                .map(|action| {
                    let enabled =
                        action.is_enabled(&state.value, self.is_secure);

                    items.push((action.label().to_string(), enabled));

                    Entry::dispatch(action.label()).enabled(enabled)
                })
                .collect();

            if let Some(native) = &self.native_menu
                && native
                    .open(state.pointer_position.unwrap_or(position), items)
            {
                state.menu.close();

                // The native popup is pumped by an application subscription
                // that only exists while the menu is open. Invalidate the
                // widgets so the runtime re-evaluates the subscriptions right
                // away, without asking the application for a dummy message.
                shell.invalidate_widgets();

                shell.capture_event();
                shell.request_redraw();

                return;
            }

            state.menu.open(position);

            shell.capture_event();
            shell.request_redraw();
            shell.invalidate_overlay();

            return;
        }

        let is_disabled = self.on_input.is_none();

        if let Some(on_input) = &self.on_input {
            let edit =
                state
                    .input
                    .update(event, bounds, cursor, shell, |key_press| {
                        Binding::from_key_press(key_press)
                    });

            if let Some(edit) = edit {
                let on_input = if let Some(on_paste) = &self.on_paste
                    && edit.has_pasted
                {
                    on_paste
                } else {
                    on_input
                };

                state.value = state.input.value();

                shell.publish(on_input(state.value.clone()));
            }
        }

        let status = if is_disabled {
            Status::Disabled
        } else if state.input.is_focused() {
            Status::Focused {
                is_hovered: cursor.is_over(bounds),
            }
        } else if cursor.is_over(bounds) {
            Status::Hovered
        } else {
            Status::Active
        };

        if let Event::Window(window::Event::RedrawRequested(_now)) = event {
            self.last_status = Some(status);

            shell.request_input_method(
                &state
                    .input
                    .input_method(bounds.shrink(self.padding).position()),
            );
        } else if self
            .last_status
            .is_some_and(|last_status| status != last_status)
        {
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State<Renderer>>();
        let style = <Theme as text_input::Catalog>::style(
            theme,
            &self.class,
            self.last_status.unwrap_or(Status::Disabled),
        );

        let bounds = layout.bounds();

        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: style.border,
                ..renderer::Quad::default()
            },
            style.background,
        );

        state.input.draw(
            renderer,
            bounds,
            *viewport,
            text::input::Style {
                value: style.value,
                selection: style.selection,
                placeholder: style.placeholder,
            },
        );
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            if self.on_input.is_none() {
                mouse::Interaction::Idle
            } else {
                mouse::Interaction::Text
            }
        } else {
            mouse::Interaction::default()
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<iced_core::overlay::Element<'b, Message, Theme, Renderer>> {
        let state = tree.state.downcast_mut::<State<Renderer>>();

        if !state.menu.is_open() {
            return Vec::new();
        }

        let bounds = layout.bounds();

        let State {
            input, value, menu, ..
        } = state;

        let on_input = self.on_input.as_ref();
        let on_paste = self.on_paste.as_ref();

        let mut menu = Menu::new(menu, &self.entries)
            .translation(translation)
            .font(self.font.unwrap_or_else(|| renderer.font()))
            .text_size(self.size.unwrap_or_else(|| renderer.text_size()))
            .on_dispatch(move |index, shell| {
                let Some(action) = Action::ALL.get(index).copied() else {
                    return;
                };

                // The engine reports an edit exactly when the value changed;
                // that is when the application must be told about it.
                let Some(edit) = action.dispatch(input, bounds, shell) else {
                    return;
                };

                let Some(on_input) = on_input else {
                    return;
                };

                let on_input = if let Some(on_paste) = on_paste
                    && edit.has_pasted
                {
                    on_paste
                } else {
                    on_input
                };

                *value = input.value();

                shell.publish(on_input(value.clone()));
            });

        if let Some(class) = self.menu_class.as_ref() {
            menu = menu.class(class);
        }

        vec![menu.overlay(renderer, window)]
    }
}

/// Applies an entry chosen in the native popup.
fn perform_native_selection<Message, Theme, Renderer>(
    input: &ContextMenuTextInput<'_, Message, Theme>,
    index: usize,
    state: &mut State<Renderer>,
    bounds: Rectangle,
    shell: &mut Shell<'_, Message>,
) where
    Message: Clone,
    Theme: text_input::Catalog + context_menu::Catalog,
    Renderer: text::Renderer,
{
    let Some(action) = Action::ALL.get(index).copied() else {
        return;
    };

    let Some(edit) = action.dispatch(&mut state.input, bounds, shell) else {
        return;
    };

    let Some(on_input) = &input.on_input else {
        return;
    };

    let on_input = if let Some(on_paste) = &input.on_paste
        && edit.has_pasted
    {
        on_paste
    } else {
        on_input
    };

    state.value = state.input.value();

    shell.publish(on_input(state.value.clone()));
    shell.request_redraw();
}

impl<'a, Message, Theme, Renderer>
    From<ContextMenuTextInput<'a, Message, Theme>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: text_input::Catalog + context_menu::Catalog + 'a,
    Renderer: text::Renderer + 'a,
{
    fn from(input: ContextMenuTextInput<'a, Message, Theme>) -> Self {
        <ContextMenuTextInput<'a, Message, Theme> as Widget<
            Message,
            Theme,
            Renderer,
        >>::_boxed(input)
    }
}

/// A single entry of the context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl Action {
    const ALL: [Action; 4] =
        [Action::Cut, Action::Copy, Action::Paste, Action::SelectAll];

    fn label(self) -> &'static str {
        match self {
            Action::Cut => "Cut",
            Action::Copy => "Copy",
            Action::Paste => "Paste",
            Action::SelectAll => "Select All",
        }
    }

    /// The editor [`Binding`] that performs this action.
    fn binding<Message>(self) -> Binding<Message> {
        match self {
            Action::Cut => Binding::Cut,
            Action::Copy => Binding::Copy,
            Action::Paste => Binding::Paste,
            Action::SelectAll => Binding::SelectAll,
        }
    }

    /// Runs the action on the input engine by feeding it a synthetic key press
    /// bound to this action.
    fn dispatch<Message, R>(
        self,
        input: &mut text::Input<R>,
        bounds: Rectangle,
        shell: &mut Shell<'_, Message>,
    ) -> Option<text::input::Edit>
    where
        R: text::Renderer,
    {
        let event = Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Character(" ".into()),
            modified_key: keyboard::Key::Character(" ".into()),
            physical_key: keyboard::key::Physical::Code(
                keyboard::key::Code::Space,
            ),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::default(),
            text: None,
            repeat: false,
        });

        input.update(
            &event,
            bounds,
            mouse::Cursor::Unavailable,
            shell,
            move |_| Some(self.binding()),
        )
    }

    fn is_enabled(self, value: &str, is_secure: bool) -> bool {
        match self {
            // The built-in widget refuses to copy or cut a secure value.
            Action::Cut | Action::Copy => !value.is_empty() && !is_secure,
            Action::Paste => true,
            Action::SelectAll => !value.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::core::window;
    use iced_test::core::{Event, Point, Settings, Size, event, mouse};
    use iced_test::simulator::{self, Simulator};
    use iced_widget::core::{Length, Vector};
    use iced_widget::{Renderer, Theme, Widget, container, float};

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Message {
        Changed(String),
    }

    fn view(
        menu: &NativeMenu,
    ) -> iced_test::core::Element<'static, Message, Theme, Renderer> {
        context_menu_text_input("", "hello")
            .on_input(Message::Changed)
            .native_menu(menu)
            .into()
    }

    fn translated_view(
        menu: &NativeMenu,
    ) -> iced_test::core::Element<'static, Message, Theme, Renderer> {
        let input = context_menu_text_input("", "hello")
            .on_input(Message::Changed)
            .native_menu(menu);

        let card = container(input)
            .width(Length::Fixed(200.0))
            .height(Length::Fixed(100.0));

        float(card)
            .translate(|_bounds, _viewport| Vector::new(40.0, 40.0))
            .boxed()
    }

    #[test]
    fn a_native_menu_opens_at_the_window_position() {
        let menu = NativeMenu::new();
        menu.enable_probe();

        let mut ui = Simulator::with_size(
            Settings::default(),
            Size::new(400.0, 200.0),
            translated_view(&menu),
        );

        // The cursor is translated by the float before the input sees it; the
        // native popup, however, has to open in window coordinates.
        let window_position = Point::new(140.0, 50.0);

        ui.point_at(window_position);
        let statuses = ui.simulate([
            Event::Mouse(mouse::Event::CursorMoved {
                position: window_position,
            }),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        ]);

        assert_eq!(statuses[1], event::Status::Captured);

        let (position, items) =
            menu.probed_open().expect("the native menu should open");

        assert_eq!(position, window_position);
        assert_eq!(items.len(), 4);
        assert!(items.iter().all(|(_, enabled)| *enabled));
    }

    #[test]
    fn a_native_selection_is_applied_on_the_next_update() {
        let menu = NativeMenu::new();
        let mut ui = Simulator::with_size(
            Settings::default(),
            Size::new(400.0, 200.0),
            view(&menu),
        );

        // Focus the input so the editor accepts the synthetic bindings.
        ui.point_at(Point::new(10.0, 10.0));
        let _ = ui.simulate(simulator::click());

        // "Select All" (index 3), then "Cut" (index 0). The native popup
        // would normally leave these selections in the shared menu handle.
        for index in [3, 0] {
            menu.set_selection(index);

            let _ = ui.simulate([Event::Window(
                window::Event::RedrawRequested(std::time::Instant::now()),
            )]);
        }

        let messages: Vec<Message> = ui.into_messages().collect();

        assert_eq!(messages, vec![Message::Changed(String::new())]);
    }
}

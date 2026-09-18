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
//! The editing engine is the built-in [`TextInput`] itself: this widget is a
//! thin wrapper that owns one and forwards `layout`, `operate`, `draw` and
//! every event to it. Selection, dragging, undo, IME, secure input and
//! clipboard integration therefore behave *exactly* like they do in the
//! standard widget, instead of being reimplemented here.
//!
//! Only two things are added on top:
//!
//! 1. **A right-click opens the shared [`context_menu`](crate::context_menu)**,
//!    which is a widget of its own. On an unfocused input, the click is
//!    replayed as a left click so that the caret lands exactly where iced would
//!    have put it; a focused input is left alone, so that right-clicking a
//!    selection keeps it.
//! 2. **A chosen entry dispatches a synthetic keyboard shortcut** (for instance
//!    `Ctrl+X` for *Cut*) to the wrapped input. Because the input handles that
//!    shortcut itself, the action reuses iced's own editing and clipboard
//!    plumbing—including the `on_input` / `on_paste` messages—rather than
//!    duplicating it.

use iced_core::clipboard::Clipboard;
use iced_core::keyboard;
use iced_core::keyboard::key::{Code, Physical};
use iced_core::mouse;
use iced_core::text;
use iced_core::widget;
use iced_core::widget::tree::{self, Tree};
use iced_core::{
    Element, Event, Layout, Length, Padding, Pixels, Rectangle, Shell, Size,
    Vector, Widget, alignment, layout, overlay, renderer,
};
use iced_widget::text_input::{self, Status, Style, TextInput};

use crate::context_menu::{self, Entry, Menu};

/// The default [`Padding`] of a [`ContextMenuTextInput`].
pub const DEFAULT_PADDING: Padding = text_input::DEFAULT_PADDING;

/// A text input with a right-click context menu.
///
/// It offers the same builder API as [`TextInput`], plus
/// [`context_menu`](Self::context_menu) and [`menu_style`](Self::menu_style).
pub struct ContextMenuTextInput<
    'a,
    Message,
    Theme = iced_widget::Theme,
    Renderer = iced_widget::Renderer,
> where
    Theme: text_input::Catalog + context_menu::Catalog,
    Renderer: text::Renderer,
{
    inner: TextInput<'a, Message, Theme, Renderer>,
    entries: Vec<Entry<'a, Message>>,
    value: String,
    is_enabled: bool,
    is_secure: bool,
    has_context_menu: bool,
    menu_class: Option<<Theme as context_menu::Catalog>::Class<'a>>,
    font: Option<Renderer::Font>,
    text_size: Option<Pixels>,
}

impl<'a, Message, Theme, Renderer>
    ContextMenuTextInput<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Theme: text_input::Catalog + context_menu::Catalog,
    Renderer: text::Renderer,
{
    /// Creates a new [`ContextMenuTextInput`] with the given placeholder and
    /// its current value.
    pub fn new(placeholder: &str, value: &str) -> Self {
        Self {
            inner: TextInput::new(placeholder, value),
            entries: Vec::new(),
            value: String::from(value),
            is_enabled: false,
            is_secure: false,
            has_context_menu: true,
            menu_class: None,
            font: None,
            text_size: None,
        }
    }

    /// Sets the [`widget::Id`] of the input.
    pub fn id(mut self, id: impl Into<widget::Id>) -> Self {
        self.inner = self.inner.id(id);
        self
    }

    /// Converts the input into a secure password input.
    ///
    /// *Cut* and *Copy* are disabled while it is secure, just like the
    /// corresponding shortcuts are in the built-in widget.
    pub fn secure(mut self, is_secure: bool) -> Self {
        self.is_secure = is_secure;
        self.inner = self.inner.secure(is_secure);
        self
    }

    /// Enables or disables the right-click context menu.
    ///
    /// It is enabled by default. When disabled, right-clicks are ignored
    /// exactly like they are by [`TextInput`].
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
        self.is_enabled = true;
        self.inner = self.inner.on_input(on_input);
        self
    }

    /// Sets the message that should be produced when the input is focused and
    /// the enter key is pressed.
    pub fn on_submit(mut self, message: Message) -> Self {
        self.inner = self.inner.on_submit(message);
        self
    }

    /// Sets the message that should be produced when some text is pasted into
    /// the input.
    pub fn on_paste(
        mut self,
        on_paste: impl Fn(String) -> Message + 'a,
    ) -> Self {
        self.inner = self.inner.on_paste(on_paste);
        self
    }

    /// Sets the font of the input.
    pub fn font(mut self, font: Renderer::Font) -> Self {
        self.font = Some(font);
        self.inner = self.inner.font(font);
        self
    }

    /// Sets the width of the input.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }

    /// Sets the [`Padding`] of the input.
    pub fn padding<P: Into<Padding>>(mut self, padding: P) -> Self {
        self.inner = self.inner.padding(padding);
        self
    }

    /// Sets the text size of the input.
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        let size = size.into();

        self.text_size = Some(size);
        self.inner = self.inner.size(size);
        self
    }

    /// Sets the [`text::LineHeight`] of the input.
    pub fn line_height(
        mut self,
        line_height: impl Into<text::LineHeight>,
    ) -> Self {
        self.inner = self.inner.line_height(line_height);
        self
    }

    /// Sets the horizontal alignment of the input.
    pub fn align_x(
        mut self,
        alignment: impl Into<alignment::Horizontal>,
    ) -> Self {
        self.inner = self.inner.align_x(alignment);
        self
    }

    /// Sets the style of the input.
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self
    where
        <Theme as text_input::Catalog>::Class<'a>:
            From<text_input::StyleFn<'a, Theme>>,
    {
        self.inner = self.inner.style(style);
        self
    }

    /// Sets the style of the context menu.
    ///
    /// By default the menu reuses the default style of the [`Theme`].
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
}

/// Creates a new [`ContextMenuTextInput`].
pub fn context_menu_text_input<'a, Message, Theme, Renderer>(
    placeholder: &str,
    value: &str,
) -> ContextMenuTextInput<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Theme: text_input::Catalog + context_menu::Catalog,
    Renderer: text::Renderer,
{
    ContextMenuTextInput::new(placeholder, value)
}

/// The persistent state of a [`ContextMenuTextInput`].
#[derive(Debug, Default)]
struct State {
    menu: context_menu::State,
    keyboard_modifiers: keyboard::Modifiers,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for ContextMenuTextInput<'_, Message, Theme, Renderer>
where
    Message: Clone,
    Theme: text_input::Catalog + context_menu::Catalog,
    Renderer: text::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&self, tree: &mut Tree) {
        ensure_input_tree(tree, &self.inner);
        self.inner.diff(&mut tree.children[0]);
    }

    fn size(&self) -> Size<Length> {
        // The inherent `TextInput::size` builder shadows the trait method.
        <TextInput<'_, Message, Theme, Renderer> as Widget<
            Message,
            Theme,
            Renderer,
        >>::size(&self.inner)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        ensure_input_tree(tree, &self.inner);

        // The inherent `TextInput::layout` is a different, lower-level method.
        <TextInput<'_, Message, Theme, Renderer> as Widget<
            Message,
            Theme,
            Renderer,
        >>::layout(
            &mut self.inner, &mut tree.children[0], renderer, limits
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.inner
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    #[allow(clippy::too_many_arguments)]
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // The user's modifiers are remembered so that a menu entry can restore
        // them after faking its own shortcut.
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) =
            event
        {
            tree.state.downcast_mut::<State>().keyboard_modifiers = *modifiers;
        }

        // A right-click opens the menu. On an unfocused input, replaying it as
        // a left click makes the built-in widget focus itself and drop the
        // caret right where the user clicked, without this wrapper having to
        // know anything about text layout. A focused input is left alone, so
        // that right-clicking a selection keeps it.
        if self.has_context_menu
            && self.is_enabled
            && let Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right,
            )) = event
            && let Some(position) = cursor.position_over(layout.bounds())
        {
            let is_focused = tree.children[0]
                .state
                .downcast_ref::<text_input::State<Renderer::Paragraph>>()
                .is_focused();

            if !is_focused {
                for click in [
                    mouse::Event::ButtonPressed(mouse::Button::Left),
                    mouse::Event::ButtonReleased(mouse::Button::Left),
                ] {
                    self.inner.update(
                        &mut tree.children[0],
                        &Event::Mouse(click),
                        layout,
                        cursor,
                        renderer,
                        clipboard,
                        shell,
                        viewport,
                    );
                }
            }

            self.entries = Action::ALL
                .iter()
                .map(|action| {
                    Entry::dispatch(action.label())
                        .enabled(action.is_enabled(&self.value, self.is_secure))
                })
                .collect();

            tree.state.downcast_mut::<State>().menu.open(position);

            shell.capture_event();
            shell.request_redraw();

            return;
        }

        self.inner.update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        // The inherent `TextInput::draw` is a different, lower-level method.
        <TextInput<'_, Message, Theme, Renderer> as Widget<
            Message,
            Theme,
            Renderer,
        >>::draw(
            &self.inner,
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
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.inner.mouse_interaction(
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
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        _translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let state = tree.state.downcast_mut::<State>();

        state.menu.position()?;

        let input_tree = tree.children.first_mut()?;

        let mut shortcuts = Shortcuts {
            input: &mut self.inner,
            tree: input_tree,
            layout,
            viewport: *viewport,
            keyboard_modifiers: state.keyboard_modifiers,
        };

        let mut menu = Menu::new(&mut state.menu, &self.entries)
            .font(self.font.unwrap_or_else(|| renderer.default_font()))
            .text_size(
                self.text_size.unwrap_or_else(|| renderer.default_size()),
            )
            .on_dispatch(move |index, renderer, clipboard, shell| {
                shortcuts.perform(index, renderer, clipboard, shell);
            });

        if let Some(class) = self.menu_class.as_ref() {
            menu = menu.class(class);
        }

        Some(menu.overlay())
    }
}

impl<'a, Message, Theme, Renderer>
    From<ContextMenuTextInput<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: text_input::Catalog + context_menu::Catalog + 'a,
    Renderer: text::Renderer + 'a,
{
    fn from(input: ContextMenuTextInput<'a, Message, Theme, Renderer>) -> Self {
        Element::new(input)
    }
}

/// Ensures the wrapped [`TextInput`] has a child [`Tree`] of the correct shape.
fn ensure_input_tree<Message, Theme, Renderer>(
    tree: &mut Tree,
    inner: &TextInput<'_, Message, Theme, Renderer>,
) where
    Message: Clone,
    Theme: text_input::Catalog,
    Renderer: text::Renderer,
{
    let tag = inner.tag();

    if tree.children.is_empty() || tree.children[0].tag != tag {
        tree.children.clear();
        tree.children.push(Tree {
            tag,
            state: inner.state(),
            children: inner.children(),
        });
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

    /// The latin character of the shortcut that triggers this action in the
    /// built-in [`TextInput`].
    fn character(self) -> &'static str {
        match self {
            Action::Cut => "x",
            Action::Copy => "c",
            Action::Paste => "v",
            Action::SelectAll => "a",
        }
    }

    /// The physical key of the shortcut that triggers this action.
    fn code(self) -> Code {
        match self {
            Action::Cut => Code::KeyX,
            Action::Copy => Code::KeyC,
            Action::Paste => Code::KeyV,
            Action::SelectAll => Code::KeyA,
        }
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

/// Everything needed to feed a synthetic shortcut to the wrapped [`TextInput`].
struct Shortcuts<'a, 'b, Message, Theme, Renderer>
where
    Theme: text_input::Catalog,
    Renderer: text::Renderer,
{
    input: &'b mut TextInput<'a, Message, Theme, Renderer>,
    tree: &'b mut Tree,
    layout: Layout<'b>,
    viewport: Rectangle,
    keyboard_modifiers: keyboard::Modifiers,
}

impl<Message, Theme, Renderer> Shortcuts<'_, '_, Message, Theme, Renderer>
where
    Message: Clone,
    Theme: text_input::Catalog,
    Renderer: text::Renderer,
{
    /// Replays the keyboard shortcut of the chosen [`Action`].
    fn perform(
        &mut self,
        index: usize,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let Some(action) = Action::ALL.get(index).copied() else {
            return;
        };

        let modifiers = self.keyboard_modifiers;

        // Pretend the platform modifier is held...
        self.dispatch(
            keyboard::Event::ModifiersChanged(keyboard::Modifiers::COMMAND),
            renderer,
            clipboard,
            shell,
        );

        // ...press the shortcut...
        let key = keyboard::Key::Character(action.character().into());

        self.dispatch(
            keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: Physical::Code(action.code()),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::COMMAND,
                text: None,
                repeat: false,
            },
            renderer,
            clipboard,
            shell,
        );

        // ...and give the user their modifiers back.
        self.dispatch(
            keyboard::Event::ModifiersChanged(modifiers),
            renderer,
            clipboard,
            shell,
        );
    }

    fn dispatch(
        &mut self,
        event: keyboard::Event,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        self.input.update(
            self.tree,
            &Event::Keyboard(event),
            self.layout,
            mouse::Cursor::Unavailable,
            renderer,
            clipboard,
            shell,
            &self.viewport,
        );
    }
}

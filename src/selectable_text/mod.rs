//! Text that can be selected, with a right-click menu.
//!
//! [`SelectableText`] renders a string like [`iced_widget::text`] does, but the
//! user can select it with the mouse—click to place a cursor, drag to select a
//! range, double-click for a word, triple-click for a line—and copy it with
//! `Ctrl`/`Cmd` + `C`, `Ctrl`/`Cmd` + `A`, or through the right-click
//! [`context_menu`](crate::context_menu).
//!
//! # Example
//!
//! ```no_run
//! use iced_component::selectable_text::selectable_text;
//!
//! fn view<'a>() -> iced_widget::core::Element<'a, (), iced_widget::Theme, iced_widget::Renderer> {
//!     selectable_text("Some text the user can select and copy").into()
//! }
//! ```
//!
//! # Design
//!
//! The text is laid out and rendered by the [`Editor`] that every
//! [`text::Renderer`] already provides—the very same engine that powers
//! [`iced_widget::text_editor`]. That is what makes crisp selection geometry,
//! bidirectional text, wrapping and font shaping work without reimplementing
//! them here.
//!
//! The widget only ever performs *selection* actions on that editor, never
//! editing ones, which is what makes it read-only. Copying goes straight to the
//! [`Clipboard`] the runtime hands to [`Widget::update`], through
//! [`Editor::copy`].
//!
//! [`Editor`]: iced_core::text::editor::Editor
//! [`Editor::copy`]: iced_core::text::editor::Editor::copy

use std::borrow::Cow;

use iced_core::clipboard::{self, Clipboard};
use iced_core::keyboard;
use iced_core::mouse;
use iced_core::text;
use iced_core::text::editor::{Action, Editor as _, Selection};
use iced_core::text::highlighter::PlainText;
use iced_core::touch;
use iced_core::widget::tree::{self, Tree};
use iced_core::window;
use iced_core::{
    Color, Element, Event, Layout, Length, Padding, Pixels, Point, Rectangle,
    Shell, Size, Vector, Widget, layout, overlay, renderer,
};

use crate::context_menu::{self, Entry, Menu};

/// Text that can be selected with the mouse and copied to the clipboard.
pub struct SelectableText<
    'a,
    Message,
    Theme = iced_widget::Theme,
    Renderer = iced_widget::Renderer,
> where
    Theme: Catalog + context_menu::Catalog,
    Renderer: text::Renderer,
{
    content: Cow<'a, str>,
    id: Option<iced_core::widget::Id>,
    entries: Vec<Entry<'a, Message>>,
    width: Length,
    height: Length,
    padding: Padding,
    size: Option<Pixels>,
    line_height: Option<text::LineHeight>,
    font: Option<Renderer::Font>,
    wrapping: text::Wrapping,
    context_menu: bool,
    class: <Theme as Catalog>::Class<'a>,
    menu_class: Option<<Theme as context_menu::Catalog>::Class<'a>>,
}

impl<'a, Message, Theme, Renderer> SelectableText<'a, Message, Theme, Renderer>
where
    Theme: Catalog + context_menu::Catalog,
    Renderer: text::Renderer,
{
    /// Creates a new [`SelectableText`] with the given content.
    pub fn new(content: impl Into<Cow<'a, str>>) -> Self {
        Self {
            content: content.into(),
            id: None,
            entries: vec![
                Entry::dispatch("Copy"),
                Entry::dispatch("Select All"),
            ],
            width: Length::Shrink,
            height: Length::Shrink,
            padding: Padding::ZERO,
            size: None,
            line_height: None,
            font: None,
            wrapping: text::Wrapping::default(),
            context_menu: true,
            class: <Theme as Catalog>::default(),
            menu_class: None,
        }
    }

    /// Sets the [`iced_core::widget::Id`] of the text.
    pub fn id(mut self, id: impl Into<iced_core::widget::Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets the size of the text.
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// Sets the [`text::LineHeight`] of the text.
    pub fn line_height(
        mut self,
        line_height: impl Into<text::LineHeight>,
    ) -> Self {
        self.line_height = Some(line_height.into());
        self
    }

    /// Sets the font of the text.
    pub fn font(mut self, font: Renderer::Font) -> Self {
        self.font = Some(font);
        self
    }

    /// Sets the width of the text.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the text.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the [`Padding`] that surrounds the text.
    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    /// Sets the [`text::Wrapping`] strategy of the text.
    pub fn wrapping(mut self, wrapping: text::Wrapping) -> Self {
        self.wrapping = wrapping;
        self
    }

    /// Enables or disables the right-click context menu. It is enabled by
    /// default.
    pub fn context_menu(mut self, context_menu: bool) -> Self {
        self.context_menu = context_menu;
        self
    }

    /// Sets the style of the text.
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme) -> Style + 'a) -> Self
    where
        <Theme as Catalog>::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as StyleFn<'a, Theme>).into();
        self
    }

    /// Sets the style class of the text.
    #[must_use]
    pub fn class(
        mut self,
        class: impl Into<<Theme as Catalog>::Class<'a>>,
    ) -> Self {
        self.class = class.into();
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

    fn resolve_style(&self, theme: &Theme) -> Style {
        <Theme as Catalog>::style(theme, &self.class)
    }
}

/// Creates a new [`SelectableText`] with the given content.
pub fn selectable_text<'a, Message, Theme, Renderer>(
    content: impl Into<Cow<'a, str>>,
) -> SelectableText<'a, Message, Theme, Renderer>
where
    Theme: Catalog + context_menu::Catalog,
    Renderer: text::Renderer,
{
    SelectableText::new(content)
}

/// The persistent state of a [`SelectableText`].
struct State<E: iced_core::text::editor::Editor> {
    editor: E,
    content: String,
    is_focused: bool,
    is_dragging: bool,
    last_click: Option<mouse::Click>,
    keyboard_modifiers: keyboard::Modifiers,
    menu: context_menu::State,
}

impl<E: iced_core::text::editor::Editor> Default for State<E> {
    fn default() -> Self {
        Self {
            editor: E::default(),
            content: String::new(),
            is_focused: false,
            is_dragging: false,
            last_click: None,
            keyboard_modifiers: keyboard::Modifiers::default(),
            menu: context_menu::State::new(),
        }
    }
}

/// Converts a position in window coordinates into the coordinate space of the
/// text itself.
fn local(position: Point, text_bounds: Rectangle) -> Point {
    Point::new(position.x - text_bounds.x, position.y - text_bounds.y)
}

/// A pointer interaction, normalized across mouse and touch input.
enum Interaction {
    Press(Point),
    Move(Point),
    Release,
}

fn interaction(event: &Event, cursor: mouse::Cursor) -> Option<Interaction> {
    match event {
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            cursor.position().map(Interaction::Press)
        }
        Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Interaction::Move(*position))
        }
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Interaction::Release)
        }
        Event::Touch(touch::Event::FingerPressed { position, .. }) => {
            Some(Interaction::Press(*position))
        }
        Event::Touch(touch::Event::FingerMoved { position, .. }) => {
            Some(Interaction::Move(*position))
        }
        Event::Touch(
            touch::Event::FingerLifted { .. } | touch::Event::FingerLost { .. },
        ) => Some(Interaction::Release),
        _ => None,
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for SelectableText<'_, Message, Theme, Renderer>
where
    Theme: Catalog + context_menu::Catalog,
    Renderer: text::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Renderer::Editor>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<Renderer::Editor>::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State<Renderer::Editor>>();

        if state.content != self.content.as_ref() {
            state.editor = Renderer::Editor::with_text(&self.content);
            state.content = self.content.to_string();
        }

        state.editor.update(
            limits.shrink(self.padding).max(),
            self.font.unwrap_or_else(|| renderer.default_font()),
            self.size.unwrap_or_else(|| renderer.default_size()),
            self.line_height.unwrap_or_default(),
            self.wrapping,
            &mut PlainText,
        );

        let intrinsic =
            state.editor.min_bounds().expand(Size::from(self.padding));

        layout::Node::new(limits.resolve(self.width, self.height, intrinsic))
    }

    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn iced_core::widget::Operation,
    ) {
        // Reporting the text makes the widget discoverable through
        // `iced_test`'s selectors.
        operation.text(self.id.as_ref(), layout.bounds(), &self.content);
    }

    #[allow(clippy::too_many_arguments)]
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State<Renderer::Editor>>();

        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) =
            event
        {
            state.keyboard_modifiers = *modifiers;
        }

        let bounds = layout.bounds();
        let text_bounds = bounds.shrink(self.padding);

        // A right-click opens the context menu, whose entries are enabled
        // according to the current selection.
        if self.context_menu
            && let Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right,
            )) = event
            && let Some(position) = cursor.position_over(bounds)
        {
            self.entries = vec![
                Entry::dispatch("Copy").enabled(state.editor.copy().is_some()),
                Entry::dispatch("Select All").enabled(!state.editor.is_empty()),
            ];

            state.menu.open(position);

            shell.capture_event();
            shell.request_redraw();

            return;
        }

        match interaction(event, cursor) {
            Some(Interaction::Press(position)) => {
                if !bounds.contains(position) {
                    // Clicking anywhere else takes the focus away, but keeps the
                    // selection around—just like a browser does.
                    state.is_focused = false;
                    state.is_dragging = false;

                    return;
                }

                let position = local(position, text_bounds);

                let click = mouse::Click::new(
                    position,
                    mouse::Button::Left,
                    state.last_click,
                );

                state.last_click = Some(click);
                state.is_focused = true;
                state.is_dragging = true;
                state.menu.close();

                state.editor.perform(match click.kind() {
                    mouse::click::Kind::Single => Action::Click(position),
                    mouse::click::Kind::Double => Action::SelectWord,
                    mouse::click::Kind::Triple => Action::SelectLine,
                });

                shell.capture_event();
                shell.request_redraw();
            }
            Some(Interaction::Move(position)) => {
                if state.is_dragging {
                    state
                        .editor
                        .perform(Action::Drag(local(position, text_bounds)));

                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Some(Interaction::Release) => {
                state.is_dragging = false;
            }
            None => match event {
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key,
                    physical_key,
                    ..
                }) if state.is_focused => {
                    if state.keyboard_modifiers.command() {
                        match key.to_latin(*physical_key) {
                            Some('c') => {
                                if let Some(selection) = state.editor.copy() {
                                    clipboard.write(
                                        clipboard::Kind::Standard,
                                        selection,
                                    );
                                }

                                shell.capture_event();
                            }
                            Some('a') => {
                                state.editor.perform(Action::SelectAll);

                                shell.capture_event();
                                shell.request_redraw();
                            }
                            _ => {}
                        }
                    } else if matches!(
                        key.as_ref(),
                        keyboard::Key::Named(keyboard::key::Named::Escape)
                    ) {
                        state.is_focused = false;

                        shell.request_redraw();
                    }
                }
                Event::Window(window::Event::Unfocused) => {
                    state.is_focused = false;
                }
                _ => {}
            },
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State<Renderer::Editor>>();
        let style = self.resolve_style(theme);
        let text_bounds = layout.bounds().shrink(self.padding);

        // The selection is painted first, so that the text stays legible on top
        // of it.
        if let Selection::Range(ranges) = state.editor.selection() {
            let translation = text_bounds.position() - Point::ORIGIN;

            for range in ranges {
                if let Some(selection) =
                    text_bounds.intersection(&(range + translation))
                {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: selection,
                            ..renderer::Quad::default()
                        },
                        style.selection,
                    );
                }
            }
        }

        renderer.fill_editor(
            &state.editor,
            text_bounds.position(),
            style.color,
            text_bounds,
        );
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        _layout: Layout<'b>,
        renderer: &Renderer,
        _viewport: &Rectangle,
        _translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let state = tree.state.downcast_mut::<State<Renderer::Editor>>();

        state.menu.position()?;

        let State { editor, menu, .. } = state;

        let mut menu = Menu::new(menu, &self.entries)
            .font(self.font.unwrap_or_else(|| renderer.default_font()))
            .text_size(self.size.unwrap_or_else(|| renderer.default_size()))
            .on_dispatch(
                move |index, _renderer, clipboard, _shell| match index {
                    0 => {
                        if let Some(selection) = editor.copy() {
                            clipboard
                                .write(clipboard::Kind::Standard, selection);
                        }
                    }
                    1 => editor.perform(Action::SelectAll),
                    _ => {}
                },
            );

        if let Some(class) = self.menu_class.as_ref() {
            menu = menu.class(class);
        }

        Some(menu.overlay())
    }
}

impl<'a, Message, Theme, Renderer>
    From<SelectableText<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: Catalog + context_menu::Catalog + 'a,
    Renderer: text::Renderer + 'a,
{
    fn from(text: SelectableText<'a, Message, Theme, Renderer>) -> Self {
        Element::new(text)
    }
}

/// The appearance of a [`SelectableText`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// The [`Color`] of the text.
    pub color: Color,
    /// The [`Color`] of the selection.
    pub selection: Color,
}

/// The theme catalog of a [`SelectableText`].
pub trait Catalog {
    /// The item class of the [`Catalog`].
    type Class<'a>;

    /// The default class produced by the [`Catalog`].
    fn default<'a>() -> Self::Class<'a>;

    /// The [`Style`] of a class.
    fn style(&self, class: &Self::Class<'_>) -> Style;
}

/// A styling function for a [`SelectableText`].
pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme) -> Style + 'a>;

impl Catalog for iced_widget::Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &Self::Class<'_>) -> Style {
        class(self)
    }
}

/// The default style of a [`SelectableText`].
pub fn default(theme: &iced_widget::Theme) -> Style {
    let palette = theme.extended_palette();

    Style {
        color: palette.background.base.text,
        selection: palette.primary.weak.color,
    }
}

//! A reusable right-click context menu.
//!
//! This module is a small, self-contained component that can be attached to any
//! other widget—or driven directly by a custom widget that wants to keep
//! control of what its entries do.
//!
//! # Attaching a menu to any content
//!
//! [`context_menu`] wraps an [`Element`] and opens a menu when it is
//! right-clicked:
//!
//! ```no_run
//! use iced_component::context_menu::{self, Entry};
//!
//! #[derive(Debug, Clone)]
//! enum Message {
//!     Copy,
//!     Reload,
//! }
//!
//! fn view<'a>() -> iced_widget::core::Element<'a, Message, iced_widget::Theme, iced_widget::Renderer> {
//!     context_menu::context_menu(iced_widget::text("Right-click me"))
//!         .entry(Entry::new("Copy", || Message::Copy))
//!         .entry(Entry::new("Reload", || Message::Reload))
//!         .into()
//! }
//! ```
//!
//! # Driving a menu from a custom widget
//!
//! A widget that owns its own actions—like a text editor that copies to the
//! clipboard—can use [`State`] and [`Menu`] directly: keep a [`State`] around,
//! call [`State::open`] when a right-click lands, and build a [`Menu`] in
//! [`Widget::overlay`]. Entries created with [`Entry::dispatch`] are handed back
//! to the widget through [`Menu::on_dispatch`].
//!
//! [`Widget::overlay`]: iced_core::Widget::overlay

use std::borrow::Cow;

use iced_core::clipboard::Clipboard;
use iced_core::keyboard;
use iced_core::mouse;
use iced_core::text;
use iced_core::widget::tree::{self, Tree};
use iced_core::window;
use iced_core::{
    Background, Border, Color, Element, Event, Layout, Length, Pixels, Point,
    Rectangle, Shadow, Shell, Size, Vector, Widget, alignment, layout, overlay,
    renderer,
};

/// The spacing between the border of a menu and its entries.
pub const PADDING: f32 = 4.0;

/// The height of a single menu entry.
pub const ITEM_HEIGHT: f32 = 28.0;

/// The horizontal padding inside a menu entry.
pub const ITEM_PADDING_X: f32 = 12.0;

/// The minimum width of a menu.
pub const MIN_WIDTH: f32 = 140.0;

/// Roughly how wide a character is, relative to the text size. Used to guess
/// the width of a menu from its longest entry.
const CHARACTER_WIDTH_RATIO: f32 = 0.62;

/// The persistent state of a context menu.
///
/// A widget that wants a context menu stores one of these in its [`Tree`], opens
/// it with [`State::open`] and builds a [`Menu`] from it while it is open.
#[derive(Debug, Default)]
pub struct State {
    open: Option<Open>,
}

#[derive(Debug)]
struct Open {
    /// Where the menu was opened, in window coordinates.
    position: Point,
    hovered: Option<usize>,
}

impl State {
    /// Creates a new, closed [`State`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens the menu at the given position, in window coordinates.
    pub fn open(&mut self, position: Point) {
        self.open = Some(Open {
            position,
            hovered: None,
        });
    }

    /// Closes the menu.
    pub fn close(&mut self) {
        self.open = None;
    }

    /// Returns whether the menu is currently open.
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Returns where the menu was opened, in window coordinates.
    pub fn position(&self) -> Option<Point> {
        self.open.as_ref().map(|open| open.position)
    }

    /// Returns the index of the entry the cursor is currently over, if any.
    pub fn hovered(&self) -> Option<usize> {
        self.open.as_ref().and_then(|open| open.hovered)
    }
}

/// A single entry of a context menu.
pub struct Entry<'a, Message> {
    label: Cow<'a, str>,
    is_enabled: bool,
    action: Action<'a, Message>,
}

enum Action<'a, Message> {
    /// Produces a message when the entry is chosen.
    Message(Box<dyn Fn() -> Message + 'a>),
    /// Hands the entry over to the widget's [`Menu::on_dispatch`] callback.
    Dispatch,
}

impl<'a, Message> Entry<'a, Message> {
    /// Creates an entry that produces a message when it is chosen.
    pub fn new(
        label: impl Into<Cow<'a, str>>,
        on_select: impl Fn() -> Message + 'a,
    ) -> Self {
        Self {
            label: label.into(),
            is_enabled: true,
            action: Action::Message(Box::new(on_select)),
        }
    }

    /// Creates an entry that is handled by the widget's
    /// [`Menu::on_dispatch`] callback.
    pub fn dispatch(label: impl Into<Cow<'a, str>>) -> Self {
        Self {
            label: label.into(),
            is_enabled: true,
            action: Action::Dispatch,
        }
    }

    /// Sets whether the entry can be chosen.
    pub fn enabled(mut self, is_enabled: bool) -> Self {
        self.is_enabled = is_enabled;
        self
    }

    /// Returns the label of the entry.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns whether the entry can be chosen.
    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }
}

/// The callback that handles the [`Entry::dispatch`] entries of a [`Menu`].
///
/// It receives the index of the chosen entry, the [`Renderer`] of the runtime
/// and the [`Clipboard`].
pub type Dispatcher<'a, Message, Renderer> = Box<
    dyn FnMut(usize, &Renderer, &mut dyn Clipboard, &mut Shell<'_, Message>)
        + 'a,
>;

/// The overlay that draws and drives a context menu.
///
/// It is normally built from the [`State`] of the widget that owns the menu:
///
/// ```no_run
/// # use iced_component::context_menu::{Entry, Menu, State};
/// # fn build<'a, 'b, Message, Theme, Renderer>(
/// #     state: &'b mut State,
/// #     entries: &'b [Entry<'a, Message>],
/// # ) -> iced_core::overlay::Element<'b, Message, Theme, Renderer>
/// # where
/// #     Message: 'b,
/// #     Theme: iced_component::context_menu::Catalog + 'b,
/// #     Renderer: iced_core::text::Renderer + 'b,
/// # {
/// Menu::new(state, entries)
///     .on_dispatch(|index, _renderer, _clipboard, _shell| { /* ... */ })
///     .overlay()
/// # }
/// ```
pub struct Menu<'a, 'b, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    state: &'b mut State,
    entries: &'b [Entry<'a, Message>],
    dispatch: Option<Dispatcher<'b, Message, Renderer>>,
    font: Option<Renderer::Font>,
    text_size: Option<Pixels>,
    class: Option<&'b Theme::Class<'a>>,
    translation: Vector,
}

impl<'a, 'b, Message, Theme, Renderer> Menu<'a, 'b, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    /// Creates a [`Menu`] for the given [`State`] and entries.
    ///
    /// The font and the text size default to the ones of the [`Renderer`].
    pub fn new(
        state: &'b mut State,
        entries: &'b [Entry<'a, Message>],
    ) -> Self {
        Self {
            state,
            entries,
            dispatch: None,
            font: None,
            text_size: None,
            class: None,
            translation: Vector::ZERO,
        }
    }

    /// Offsets the menu from the coordinate space its position was recorded in
    /// to the coordinate space of the window.
    ///
    /// Ancestors that translate their contents—like a
    /// [`Scrollable`](iced_widget::scrollable)—hand their translation down to
    /// [`overlay`](Widget::overlay), while the position a widget records when
    /// it opens a menu is expressed in their content space. A menu that does
    /// not apply the translation would therefore be drawn as if the contents
    /// had never been scrolled.
    pub fn translation(mut self, translation: Vector) -> Self {
        self.translation = translation;
        self
    }

    /// Sets the callback used to handle [`Entry::dispatch`] entries.
    ///
    /// It receives the index of the entry that was chosen, the [`Renderer`] of
    /// the runtime and the [`Clipboard`]—everything a widget may need to carry
    /// out the action itself.
    pub fn on_dispatch(
        mut self,
        dispatch: impl FnMut(
            usize,
            &Renderer,
            &mut dyn Clipboard,
            &mut Shell<'_, Message>,
        ) + 'b,
    ) -> Self {
        self.dispatch = Some(Box::new(dispatch));
        self
    }

    /// Sets the font of the menu.
    pub fn font(mut self, font: Renderer::Font) -> Self {
        self.font = Some(font);
        self
    }

    /// Sets the text size of the menu.
    pub fn text_size(mut self, text_size: impl Into<Pixels>) -> Self {
        self.text_size = Some(text_size.into());
        self
    }

    /// Sets the style class of the menu.
    pub fn class(mut self, class: &'b Theme::Class<'a>) -> Self {
        self.class = Some(class);
        self
    }

    /// Turns the [`Menu`] into an [`overlay::Element`].
    pub fn overlay(self) -> overlay::Element<'b, Message, Theme, Renderer>
    where
        Message: 'b,
        Theme: 'b,
        Renderer: 'b,
    {
        overlay::Element::new(Box::new(self))
    }

    fn size(&self, renderer: &Renderer) -> Size {
        let text_size =
            self.text_size.unwrap_or_else(|| renderer.default_size());
        let longest = self
            .entries
            .iter()
            .map(|entry| entry.label().chars().count())
            .max()
            .unwrap_or(0) as f32;

        let width = (longest * text_size.0 * CHARACTER_WIDTH_RATIO
            + ITEM_PADDING_X * 2.0
            + PADDING * 2.0)
            .max(MIN_WIDTH);

        let height = self.entries.len() as f32 * ITEM_HEIGHT + PADDING * 2.0;

        Size::new(width, height)
    }

    /// The bounds of the entry at the given index, in window coordinates.
    pub fn entry_bounds(&self, bounds: Rectangle, index: usize) -> Rectangle {
        Rectangle {
            x: bounds.x + PADDING,
            y: bounds.y + PADDING + index as f32 * ITEM_HEIGHT,
            width: bounds.width - PADDING * 2.0,
            height: ITEM_HEIGHT,
        }
    }

    fn entry_at(&self, bounds: Rectangle, position: Point) -> Option<usize> {
        if !bounds.contains(position) {
            return None;
        }

        let index = ((position.y - bounds.y - PADDING) / ITEM_HEIGHT)
            .floor()
            .max(0.0) as usize;

        (index < self.entries.len()).then_some(index)
    }

    fn style(&self, theme: &Theme) -> Style {
        match self.class {
            Some(class) => theme.style(class),
            None => {
                let class = <Theme as Catalog>::default();

                theme.style(&class)
            }
        }
    }

    fn choose(
        &mut self,
        index: usize,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        self.state.close();

        let Some(entry) = self.entries.get(index) else {
            return;
        };

        if !entry.is_enabled() {
            return;
        }

        match &entry.action {
            Action::Message(on_select) => shell.publish(on_select()),
            Action::Dispatch => {
                if let Some(dispatch) = self.dispatch.as_mut() {
                    dispatch(index, renderer, clipboard, shell);
                }
            }
        }
    }
}

impl<Message, Theme, Renderer> overlay::Overlay<Message, Theme, Renderer>
    for Menu<'_, '_, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let size = self.size(renderer);
        let viewport = Rectangle::with_size(bounds);

        let position =
            self.state.position().unwrap_or(Point::ORIGIN) + self.translation;

        let mut x = position.x;
        let mut y = position.y;

        if x + size.width > viewport.x + viewport.width {
            x = viewport.x + viewport.width - size.width;
        }

        if y + size.height > viewport.y + viewport.height {
            y = viewport.y + viewport.height - size.height;
        }

        x = x.max(viewport.x);
        y = y.max(viewport.y);

        layout::Node::new(size).translate(Vector::new(x, y))
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
    ) {
        if !self.state.is_open() {
            return;
        }

        let style = self.style(theme);
        let bounds = layout.bounds();
        let font = self.font.unwrap_or_else(|| renderer.default_font());
        let text_size =
            self.text_size.unwrap_or_else(|| renderer.default_size());

        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: style.border,
                shadow: style.shadow,
                ..renderer::Quad::default()
            },
            style.background,
        );

        for (index, entry) in self.entries.iter().enumerate() {
            let item = self.entry_bounds(bounds, index);
            let hovered = self.state.hovered() == Some(index);

            let (background, text_color) = if !entry.is_enabled() {
                (None, style.disabled_text_color)
            } else if hovered {
                (Some(style.selected_background), style.selected_text_color)
            } else {
                (None, style.text_color)
            };

            if let Some(background) = background {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: item,
                        ..renderer::Quad::default()
                    },
                    background,
                );
            }

            // `fill_text` anchors the text on the point described by the
            // alignments, so a vertically centered label is anchored on the
            // center of its entry.
            renderer.fill_text(
                text::Text {
                    content: String::from(entry.label()),
                    bounds: Size::new(f32::INFINITY, item.height),
                    size: text_size,
                    line_height: text::LineHeight::default(),
                    font,
                    align_x: text::Alignment::Left,
                    align_y: alignment::Vertical::Center,
                    shaping: text::Shaping::default(),
                    wrapping: text::Wrapping::None,
                },
                Point::new(item.x + ITEM_PADDING_X, item.center_y()),
                text_color,
                bounds,
            );
        }
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        if !self.state.is_open() {
            return;
        }

        let bounds = layout.bounds();

        match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let hovered = self.entry_at(bounds, *position);

                if let Some(open) = self.state.open.as_mut()
                    && open.hovered != hovered
                {
                    open.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(position) = cursor.position() else {
                    return;
                };

                if !bounds.contains(position) {
                    // Clicking outside dismisses the menu, but the click must
                    // still reach the widget underneath.
                    self.state.close();
                    shell.request_redraw();
                    return;
                }

                let Some(index) = self.entry_at(bounds, position) else {
                    // A click on the menu padding should not fall through.
                    shell.capture_event();
                    return;
                };

                shell.request_redraw();
                self.choose(index, renderer, clipboard, shell);
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonPressed(_)) => {
                // Another button (typically a second right-click) should reopen
                // the menu where it landed, so the press is left for the widget
                // underneath to see.
                self.state.close();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::WheelScrolled { .. })
            | Event::Touch(_)
            | Event::Keyboard(keyboard::Event::KeyPressed { .. })
            | Event::Window(window::Event::Unfocused) => {
                self.state.close();
                shell.request_redraw();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();

        cursor
            .position()
            .and_then(|position| self.entry_at(bounds, position))
            .map(|index| {
                if self.entries[index].is_enabled() {
                    mouse::Interaction::Pointer
                } else {
                    // Keep the arrow over a disabled entry instead of letting
                    // the cursor of the widget underneath shine through.
                    mouse::Interaction::Idle
                }
            })
            .unwrap_or_default()
    }
}

/// A widget that opens a context menu when it is right-clicked.
///
/// Create it with [`context_menu`] and fill it with [`Entry`] values.
pub struct ContextMenu<
    'a,
    Message,
    Theme = iced_widget::Theme,
    Renderer = iced_widget::Renderer,
> where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    content: Element<'a, Message, Theme, Renderer>,
    entries: Vec<Entry<'a, Message>>,
    context_menu: bool,
    font: Option<Renderer::Font>,
    text_size: Option<Pixels>,
    class: Option<Theme::Class<'a>>,
}

impl<'a, Message, Theme, Renderer> ContextMenu<'a, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    /// Creates a new [`ContextMenu`] with the given content.
    pub fn new(
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self {
            content: content.into(),
            entries: Vec::new(),
            context_menu: true,
            font: None,
            text_size: None,
            class: None,
        }
    }

    /// Adds an entry to the menu.
    pub fn entry(mut self, entry: Entry<'a, Message>) -> Self {
        self.entries.push(entry);
        self
    }

    /// Adds several entries to the menu.
    pub fn entries(
        mut self,
        entries: impl IntoIterator<Item = Entry<'a, Message>>,
    ) -> Self {
        self.entries.extend(entries);
        self
    }

    /// Enables or disables the context menu. It is enabled by default.
    pub fn context_menu(mut self, context_menu: bool) -> Self {
        self.context_menu = context_menu;
        self
    }

    /// Sets the font of the menu.
    pub fn font(mut self, font: Renderer::Font) -> Self {
        self.font = Some(font);
        self
    }

    /// Sets the text size of the menu.
    pub fn text_size(mut self, text_size: impl Into<Pixels>) -> Self {
        self.text_size = Some(text_size.into());
        self
    }

    /// Sets the style of the menu.
    #[must_use]
    pub fn menu_style(mut self, style: impl Fn(&Theme) -> Style + 'a) -> Self
    where
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.class = Some((Box::new(style) as StyleFn<'a, Theme>).into());
        self
    }
}

/// Creates a widget that opens a context menu when it is right-clicked.
pub fn context_menu<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> ContextMenu<'a, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    ContextMenu::new(content)
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for ContextMenu<'_, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            limits,
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn iced_core::widget::Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            layout,
            renderer,
            operation,
        );
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
        if self.context_menu
            && !self.entries.is_empty()
            && let Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right,
            )) = event
            && let Some(position) = cursor.position_over(layout.bounds())
        {
            tree.state.downcast_mut::<State>().open(position);

            shell.capture_event();
            shell.request_redraw();

            return;
        }

        self.content.as_widget_mut().update(
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
        self.content.as_widget().draw(
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
        self.content.as_widget().mouse_interaction(
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
        _layout: Layout<'b>,
        renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let state = tree.state.downcast_mut::<State>();

        if !state.is_open() {
            return None;
        }

        let mut menu = Menu::new(state, &self.entries)
            .translation(translation)
            .font(self.font.unwrap_or_else(|| renderer.default_font()))
            .text_size(
                self.text_size.unwrap_or_else(|| renderer.default_size()),
            );

        if let Some(class) = self.class.as_ref() {
            menu = menu.class(class);
        }

        Some(menu.overlay())
    }
}

impl<'a, Message, Theme, Renderer>
    From<ContextMenu<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: Catalog + 'a,
    Renderer: text::Renderer + 'a,
{
    fn from(context_menu: ContextMenu<'a, Message, Theme, Renderer>) -> Self {
        Element::new(context_menu)
    }
}

/// The appearance of a context menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// The [`Background`] of the menu.
    pub background: Background,
    /// The [`Border`] of the menu.
    pub border: Border,
    /// The [`Shadow`] of the menu.
    pub shadow: Shadow,
    /// The text [`Color`] of an entry.
    pub text_color: Color,
    /// The text [`Color`] of a disabled entry.
    pub disabled_text_color: Color,
    /// The text [`Color`] of the hovered entry.
    pub selected_text_color: Color,
    /// The [`Background`] of the hovered entry.
    pub selected_background: Background,
}

/// The theme catalog of a [`ContextMenu`].
pub trait Catalog {
    /// The item class of the [`Catalog`].
    type Class<'a>;

    /// The default class produced by the [`Catalog`].
    fn default<'a>() -> Self::Class<'a>;

    /// The [`Style`] of a class.
    fn style(&self, class: &Self::Class<'_>) -> Style;
}

/// A styling function for a [`ContextMenu`].
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

/// The default style of a [`ContextMenu`].
pub fn default(theme: &iced_widget::Theme) -> Style {
    let palette = theme.extended_palette();

    Style {
        background: palette.background.weak.color.into(),
        border: Border {
            width: 1.0,
            radius: 2.0.into(),
            color: palette.background.strong.color,
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.15),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        text_color: palette.background.weak.text,
        disabled_text_color: palette.background.weak.text.scale_alpha(0.5),
        selected_text_color: palette.primary.strong.text,
        selected_background: palette.primary.strong.color.into(),
    }
}

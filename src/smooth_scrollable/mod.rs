//! Smooth scrolling for [`iced`]'s [`Scrollable`], modeled after Firefox's
//! `ScrollMode::Smooth`.
//!
//! # Usage
//!
//! A single call is all you need—[`smooth_scrollable`] already returns a
//! complete, smoothly scrolling widget:
//!
//! ```no_run
//! use iced_component::smooth_scrollable::smooth_scrollable;
//! use iced_widget::{column, text};
//!
//! fn view<'a>() -> iced_widget::core::Element<'a, (), iced_widget::Theme, iced_widget::Renderer> {
//!     smooth_scrollable(column![text("Hello"), text("World")]).into()
//! }
//! ```
//!
//! Anything else is *opt-in* and simply chained on top:
//!
//! ```no_run
//! use iced_component::smooth_scrollable::{ScrollMode, smooth_scrollable};
//! use iced_widget::core::Length;
//! use iced_widget::{column, text};
//!
//! fn view<'a>() -> iced_widget::core::Element<'a, (), iced_widget::Theme, iced_widget::Renderer> {
//!     smooth_scrollable(column![text("Hello"), text("World")])
//!         .id("my-list")
//!         .height(Length::Fill)
//!         .mode(ScrollMode::Smooth)
//!         .on_scroll(|_viewport| ())
//!         .into()
//! }
//! ```
//!
//! # Overview
//!
//! [`SmoothScrollable`] is a thin, drop-in wrapper around
//! [`iced_widget::scrollable::Scrollable`]. It forwards every widget operation
//! to an inner [`Scrollable`] and only takes over *mouse wheel* handling when
//! [`ScrollMode::Smooth`] is selected (the default).
//!
//! When the user scrolls the wheel, instead of jumping directly to the new
//! offset, the wrapper animates towards it using the same cubic-Bézier model
//! Firefox uses for `ScrollMode::Smooth` (see [`physics`]). Consecutive wheel
//! events extend the in-flight animation and carry its velocity over, so a
//! quick flick becomes one continuous, snappy motion.
//!
//! # Nested scrollables
//!
//! A [`Scrollable`] nested inside a [`SmoothScrollable`] keeps receiving wheel
//! events: the wrapper only takes the wheel over when it would have reached the
//! wrapper itself. See [Wheel arbitration](#wheel-arbitration) below.
//!
//! # Wheel arbitration
//!
//! A plain [`Scrollable`] forwards a wheel event to its content *before*
//! scrolling itself, so that a nested [`Scrollable`] gets the first chance to
//! consume it. [`SmoothScrollable`] reproduces that order:
//!
//! 1. When the wheel is over a nested [`Scrollable`] that has something left to
//!    scroll, the event is forwarded to the inner widget untouched, so iced's
//!    own chaining rules apply and the nested scrollable scrolls normally.
//! 2. Otherwise the wrapper animates the offset itself, as described above.
//! 3. If the wheel chains *past* an exhausted nested scrollable, the instant
//!    jump performed by the inner [`Scrollable`] is undone and replayed as a
//!    smooth animation, so the wrapper never jumps. The wrapper does report the
//!    intermediate offset it undoes, because [`Scrollable`] notifies before the
//!    jump can be intercepted.
//!
//! # Scope
//!
//! Scrollbars, dragging, touch, keyboard navigation and external
//! [`scrollable::scroll_to`] operations all work exactly as they do on a plain
//! [`Scrollable`]. Only [`ScrollMode::Instant`] and [`ScrollMode::Smooth`] are
//! implemented; Firefox's `SmoothMsd` (mass-spring-damper) and `Normal` are out
//! of scope.

pub mod physics;

pub use physics::{
    BezierPhysics, CURRENT_VELOCITY_WEIGHTING, DURATION_TO_INTERVAL_RATIO,
    STOP_DECELERATION_WEIGHTING, SmoothScrollSettings,
};

pub use iced_widget::scrollable::{
    AbsoluteOffset, Anchor, Direction, RelativeOffset, Scrollbar, Status,
    Style, Viewport,
};

use iced_core::clipboard::Clipboard;
use iced_core::keyboard;
use iced_core::mouse;
use iced_core::text;
use iced_core::time::Instant;
use iced_core::widget::operation::{self, Operation};
use iced_core::widget::tree::{self, Tree};
use iced_core::window;
use iced_core::{
    Element, Event, Layout, Length, Pixels, Point, Rectangle, Shell, Size,
    Vector, Widget, overlay,
};
use iced_widget::scrollable::{self, Scrollable};

/// How a [`SmoothScrollable`] reacts to scrolling.
///
/// This is a subset of Firefox's `ScrollMode` (see `layout/base/ScrollTypes.h`)
/// containing the two modes that are meaningful for a widget:
///
/// - [`ScrollMode::Instant`] scrolls immediately, exactly like a regular
///   [`Scrollable`].
/// - [`ScrollMode::Smooth`] animates towards the destination with a symmetrical
///   acceleration/deceleration curve over a fixed interval, just like
///   Firefox's `ScrollMode::Smooth`.
///
/// Firefox also defines `SmoothMsd` (a mass-spring-damper model) and `Normal`
/// (a non-synchronous, non-animated scroll). Neither is implemented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollMode {
    /// Scroll instantly, with no animation.
    Instant,
    /// Animate scrolling with Firefox's `ScrollMode::Smooth` physics.
    #[default]
    Smooth,
}

/// A [`Scrollable`] with Firefox-style smooth scrolling.
///
/// Construct it with [`smooth_scrollable`] and chain any of the builder
/// methods below only when you need to deviate from the defaults.
pub struct SmoothScrollable<
    'a,
    Message,
    Theme = iced_widget::Theme,
    Renderer = iced_widget::Renderer,
> where
    Theme: scrollable::Catalog,
    Renderer: text::Renderer,
{
    inner: Scrollable<'a, Message, Theme, Renderer>,
    mode: ScrollMode,
    settings: Option<SmoothScrollSettings>,
}

impl<'a, Message, Theme, Renderer>
    SmoothScrollable<'a, Message, Theme, Renderer>
where
    Theme: scrollable::Catalog,
    Renderer: text::Renderer,
{
    /// Creates a new [`SmoothScrollable`] with the given content.
    ///
    /// Prefer the [`smooth_scrollable`] function, which does the same thing
    /// with a shorter name.
    pub fn new(
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self {
            inner: Scrollable::new(content),
            mode: ScrollMode::Smooth,
            settings: None,
        }
    }

    /// Creates a new [`SmoothScrollable`] with the given content and
    /// [`Direction`].
    pub fn with_direction(
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
        direction: impl Into<Direction>,
    ) -> Self {
        Self::new(content).direction(direction)
    }

    /// Sets the [`ScrollMode`] of this [`SmoothScrollable`].
    ///
    /// Defaults to [`ScrollMode::Smooth`].
    pub fn mode(mut self, mode: ScrollMode) -> Self {
        self.mode = mode;
        self
    }

    /// Alias of [`SmoothScrollable::mode`].
    pub fn scroll_mode(self, mode: ScrollMode) -> Self {
        self.mode(mode)
    }

    /// Enables Firefox-style smooth scrolling. This is the default.
    pub fn smooth(self) -> Self {
        self.mode(ScrollMode::Smooth)
    }

    /// Disables the animation, making the widget behave exactly like a plain
    /// [`Scrollable`].
    pub fn instant(self) -> Self {
        self.mode(ScrollMode::Instant)
    }

    /// Overrides the [`SmoothScrollSettings`] used for the animation.
    ///
    /// By default, the settings are chosen automatically from the kind of
    /// scroll delta: [`SmoothScrollSettings::mouse_wheel`] for line-based wheel
    /// events and [`SmoothScrollSettings::pixels`] for pixel-based ones
    /// (trackpads), matching Firefox.
    pub fn smooth_scroll_settings(
        mut self,
        settings: SmoothScrollSettings,
    ) -> Self {
        self.settings = Some(settings);
        self
    }

    /// Sets the [`Direction`] of the [`Scrollable`].
    pub fn direction(mut self, direction: impl Into<Direction>) -> Self {
        self.inner = self.inner.direction(direction);
        self
    }

    /// Makes the [`Scrollable`] scroll horizontally, with default
    /// [`Scrollbar`] settings.
    pub fn horizontal(mut self) -> Self {
        self.inner = self.inner.horizontal();
        self
    }

    /// Makes the [`Scrollable`] scroll vertically, with default [`Scrollbar`]
    /// settings. This is the default.
    pub fn vertical(mut self) -> Self {
        self.inner = self
            .inner
            .direction(Direction::Vertical(Scrollbar::default()));
        self
    }

    /// Sets the [`iced_core::widget::Id`] of the [`Scrollable`].
    pub fn id(mut self, id: impl Into<iced_core::widget::Id>) -> Self {
        self.inner = self.inner.id(id);
        self
    }

    /// Sets the width of the [`Scrollable`].
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }

    /// Sets the height of the [`Scrollable`].
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.inner = self.inner.height(height);
        self
    }

    /// Sets a handler to call when the [`Scrollable`] is scrolled.
    pub fn on_scroll(mut self, f: impl Fn(Viewport) -> Message + 'a) -> Self {
        self.inner = self.inner.on_scroll(f);
        self
    }

    /// Anchors the vertical [`Scrollable`] direction to the top.
    pub fn anchor_top(mut self) -> Self {
        self.inner = self.inner.anchor_top();
        self
    }

    /// Anchors the vertical [`Scrollable`] direction to the bottom.
    pub fn anchor_bottom(mut self) -> Self {
        self.inner = self.inner.anchor_bottom();
        self
    }

    /// Anchors the horizontal [`Scrollable`] direction to the left.
    pub fn anchor_left(mut self) -> Self {
        self.inner = self.inner.anchor_left();
        self
    }

    /// Anchors the horizontal [`Scrollable`] direction to the right.
    pub fn anchor_right(mut self) -> Self {
        self.inner = self.inner.anchor_right();
        self
    }

    /// Sets the [`Anchor`] of the horizontal direction of the [`Scrollable`],
    /// if applicable.
    pub fn anchor_x(mut self, alignment: Anchor) -> Self {
        self.inner = self.inner.anchor_x(alignment);
        self
    }

    /// Sets the [`Anchor`] of the vertical direction of the [`Scrollable`], if
    /// applicable.
    pub fn anchor_y(mut self, alignment: Anchor) -> Self {
        self.inner = self.inner.anchor_y(alignment);
        self
    }

    /// Embeds the [`Scrollbar`] into the [`Scrollable`], instead of floating on
    /// top of the content.
    pub fn spacing(mut self, new_spacing: impl Into<Pixels>) -> Self {
        self.inner = self.inner.spacing(new_spacing);
        self
    }

    /// Sets whether the user should be allowed to auto-scroll the
    /// [`Scrollable`] with the middle mouse button.
    pub fn auto_scroll(mut self, auto_scroll: bool) -> Self {
        self.inner = self.inner.auto_scroll(auto_scroll);
        self
    }

    /// Sets the style of this [`Scrollable`].
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self
    where
        Theme::Class<'a>: From<scrollable::StyleFn<'a, Theme>>,
    {
        self.inner = self.inner.style(style);
        self
    }
}

/// Creates a new [`SmoothScrollable`] with the given content.
///
/// This is the one call you need: the result is a ready-to-use widget that
/// scrolls vertically and smoothly, just like Firefox does. Chain any of the
/// [`SmoothScrollable`] methods to customize it further.
pub fn smooth_scrollable<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> SmoothScrollable<'a, Message, Theme, Renderer>
where
    Theme: scrollable::Catalog,
    Renderer: text::Renderer,
{
    SmoothScrollable::new(content)
}

/// Creates a new horizontal [`SmoothScrollable`] with the given content.
///
/// Shorthand for [`smooth_scrollable`] followed by
/// [`SmoothScrollable::horizontal`].
pub fn horizontal_smooth_scrollable<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> SmoothScrollable<'a, Message, Theme, Renderer>
where
    Theme: scrollable::Catalog,
    Renderer: text::Renderer,
{
    SmoothScrollable::new(content).horizontal()
}

/// The persistent state of a [`SmoothScrollable`].
#[derive(Debug, Default)]
struct State {
    animation: Option<Animation>,
    keyboard_modifiers: keyboard::Modifiers,
}

/// An in-flight smooth scroll animation.
#[derive(Debug)]
struct Animation {
    physics: BezierPhysics,
    target: Vector,
    anchors: (Anchor, Anchor),
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for SmoothScrollable<'_, Message, Theme, Renderer>
where
    Theme: scrollable::Catalog,
    Renderer: text::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&self, tree: &mut Tree) {
        ensure_inner_tree(tree, &self.inner);
        self.inner.diff(&mut tree.children[0]);
    }

    fn size(&self) -> Size<Length> {
        self.inner.size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &iced_core::layout::Limits,
    ) -> iced_core::layout::Node {
        ensure_inner_tree(tree, &self.inner);
        self.inner.layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &iced_core::renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.inner.draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
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
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) =
            event
        {
            tree.state.downcast_mut::<State>().keyboard_modifiers = *modifiers;
        }

        if self.mode == ScrollMode::Smooth {
            match event {
                Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                    if cursor.position_over(layout.bounds()).is_some() {
                        let shift = tree
                            .state
                            .downcast_ref::<State>()
                            .keyboard_modifiers
                            .shift();
                        let now = Instant::now();

                        let state = tree.state.downcast_mut::<State>();
                        let child = &mut tree.children[0];

                        // A nested `Scrollable` must get the chance to handle
                        // the wheel first. `Scrollable` already forwards wheel
                        // events to its content, so stepping aside is enough.
                        if has_nested_scrollable(
                            &mut self.inner,
                            child,
                            layout,
                            renderer,
                            cursor,
                        ) {
                            let before = current_translation(
                                &mut self.inner,
                                child,
                                layout,
                                renderer,
                            );

                            self.inner.update(
                                child, event, layout, cursor, renderer,
                                clipboard, shell, viewport,
                            );

                            if shell.is_event_captured() {
                                let after = current_translation(
                                    &mut self.inner,
                                    child,
                                    layout,
                                    renderer,
                                );

                                if after == before {
                                    // The nested scrollable consumed the event.
                                    return;
                                }

                                // The wheel chained past the nested scrollable
                                // and scrolled us instantly; undo the jump so
                                // that the motion stays smooth.
                                let maximum = maximum_offset(layout);
                                let (anchors, _) = detect_anchors(
                                    &mut self.inner,
                                    child,
                                    layout,
                                    renderer,
                                    maximum,
                                );
                                let offset =
                                    to_absolute(before, anchors, maximum);

                                set_offset(
                                    &mut self.inner,
                                    child,
                                    layout,
                                    renderer,
                                    Some(offset.x),
                                    Some(offset.y),
                                );
                            }
                        }

                        self.begin_smooth_scroll(
                            state, child, layout, renderer, *delta, now, shift,
                        );

                        shell.capture_event();
                        shell.request_redraw();

                        return;
                    }
                }
                Event::Window(window::Event::RedrawRequested(now)) => {
                    let sampled = {
                        let state = tree.state.downcast_mut::<State>();

                        state.animation.as_mut().map(|animation| {
                            let position = animation.physics.position_at(*now);
                            let finished = animation.physics.is_finished(*now);

                            (position, finished, animation.target)
                        })
                    };

                    if let Some((position, finished, target)) = sampled {
                        let child = &mut tree.children[0];
                        let position = if finished { target } else { position };

                        apply_offset(
                            &mut self.inner,
                            child,
                            layout,
                            renderer,
                            position,
                        );

                        if finished {
                            tree.state.downcast_mut::<State>().animation = None;
                        } else {
                            shell.request_redraw();
                        }
                    }
                }
                Event::Mouse(mouse::Event::ButtonPressed(_))
                | Event::Touch(_) => {
                    tree.state.downcast_mut::<State>().animation = None;
                }
                Event::Keyboard(keyboard::Event::ModifiersChanged(_)) => {}
                Event::Keyboard(_) => {
                    tree.state.downcast_mut::<State>().animation = None;
                }
                _ => {}
            }
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
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let child = tree.children.first_mut()?;

        self.inner
            .overlay(child, layout, renderer, viewport, translation)
    }
}

impl<'a, Message, Theme, Renderer>
    SmoothScrollable<'a, Message, Theme, Renderer>
where
    Theme: scrollable::Catalog,
    Renderer: text::Renderer,
{
    /// Starts, extends, or retargets the smooth scroll animation for a wheel
    /// event.
    #[allow(clippy::too_many_arguments)]
    fn begin_smooth_scroll(
        &mut self,
        state: &mut State,
        child: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        delta: mouse::ScrollDelta,
        now: Instant,
        shift: bool,
    ) {
        let maximum = maximum_offset(layout);

        let is_lines = matches!(delta, mouse::ScrollDelta::Lines { .. });

        let settings = self.settings.unwrap_or_else(|| {
            if is_lines {
                SmoothScrollSettings::mouse_wheel()
            } else {
                SmoothScrollSettings::pixels()
            }
        });

        // Extend an in-flight animation, reusing the anchors we already
        // detected, so consecutive notches accumulate smoothly.
        if let Some(animation) = state.animation.as_mut() {
            let movement =
                align(animation.anchors, normalize_delta(delta, shift));
            let target = clamp(animation.target + movement, maximum);

            animation.target = target;
            animation.physics.update(now, target);

            return;
        }

        let (anchors, current) =
            detect_anchors(&mut self.inner, child, layout, renderer, maximum);

        let movement = align(anchors, normalize_delta(delta, shift));
        let target = clamp(current + movement, maximum);

        let mut physics = BezierPhysics::new(settings, current);
        physics.update(now, target);

        state.animation = Some(Animation {
            physics,
            target,
            anchors,
        });
    }
}

impl<'a, Message, Theme, Renderer>
    From<SmoothScrollable<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: scrollable::Catalog + 'a,
    Renderer: text::Renderer + 'a,
{
    fn from(widget: SmoothScrollable<'a, Message, Theme, Renderer>) -> Self {
        Element::new(widget)
    }
}

/// Ensures the inner [`Scrollable`] has a child [`Tree`] of the correct shape.
///
/// The child must be complete—including the subtree of the scrollable's own
/// content—because [`Scrollable`] reaches into it as soon as it is laid out.
fn ensure_inner_tree<Message, Theme, Renderer>(
    tree: &mut Tree,
    inner: &Scrollable<'_, Message, Theme, Renderer>,
) where
    Theme: scrollable::Catalog,
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

/// A no-op [`Operation`] that only records the current translation of the
/// scrollable it is applied to.
struct Probe {
    translation: Vector,
}

impl<T> Operation<T> for Probe {
    fn traverse(&mut self, _operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        // The wrapper already knows which scrollable it targets; there is no
        // need to descend into its contents.
    }

    fn scrollable(
        &mut self,
        _id: Option<&iced_core::widget::Id>,
        _bounds: Rectangle,
        _content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn operation::Scrollable,
    ) {
        self.translation = translation;
    }
}

/// An [`Operation`] that reports whether the pointer is over a [`Scrollable`]
/// nested in the wrapper with something left to scroll.
///
/// The wrapper's own scrollable is the first one reported by `operate`, at
/// depth `0`; everything reported deeper is content.
///
/// Layout bounds live in the coordinate space of the *content* of the
/// scrollables above them, while the pointer lives in window coordinates—so
/// the translations of every scrollable we descend into have to be accumulated
/// before comparing the two. Otherwise a nested scrollable would only ever be
/// found while its ancestors sit at the very start of their range.
struct NestedScrollProbe {
    position: Point,
    depth: usize,
    translation: Vector,
    pending: Vector,
    found: bool,
}

impl<T> Operation<T> for NestedScrollProbe {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        let outer = self.translation;

        self.translation += self.pending;
        self.pending = Vector::ZERO;
        self.depth += 1;

        operate(self);

        self.depth -= 1;
        self.translation = outer;
    }

    fn scrollable(
        &mut self,
        _id: Option<&iced_core::widget::Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn operation::Scrollable,
    ) {
        self.pending = translation;

        if self.depth == 0 || self.found {
            return;
        }

        if !bounds.contains(self.position + self.translation) {
            return;
        }

        self.found = content_bounds.width > bounds.width
            || content_bounds.height > bounds.height;
    }
}

/// Returns whether the pointer is over a nested [`Scrollable`] that can still
/// scroll.
fn has_nested_scrollable<Message, Theme, Renderer>(
    inner: &mut Scrollable<'_, Message, Theme, Renderer>,
    tree: &mut Tree,
    layout: Layout<'_>,
    renderer: &Renderer,
    cursor: mouse::Cursor,
) -> bool
where
    Theme: scrollable::Catalog,
    Renderer: text::Renderer,
{
    let Some(position) = cursor.position() else {
        return false;
    };

    let mut probe = NestedScrollProbe {
        position,
        depth: 0,
        translation: Vector::ZERO,
        pending: Vector::ZERO,
        found: false,
    };

    inner.operate(tree, layout, renderer, &mut probe);

    probe.found
}

/// Reads the current translation of the wrapped [`Scrollable`].
fn current_translation<Message, Theme, Renderer>(
    inner: &mut Scrollable<'_, Message, Theme, Renderer>,
    tree: &mut Tree,
    layout: Layout<'_>,
    renderer: &Renderer,
) -> Vector
where
    Theme: scrollable::Catalog,
    Renderer: text::Renderer,
{
    let mut probe = Probe {
        translation: Vector::ZERO,
    };

    inner.operate(tree, layout, renderer, &mut probe);

    probe.translation
}

/// The maximum absolute offset of the wrapped [`Scrollable`].
fn maximum_offset(layout: Layout<'_>) -> Vector {
    let bounds = layout.bounds();
    let content_bounds = layout
        .children()
        .next()
        .expect("scrollable content must have a layout")
        .bounds();

    Vector::new(
        (content_bounds.width - bounds.width).max(0.0),
        (content_bounds.height - bounds.height).max(0.0),
    )
}

/// An [`Operation`] that scrolls the target scrollable to an absolute offset.
struct SetOffset {
    offset: AbsoluteOffset<Option<f32>>,
}

impl<T> Operation<T> for SetOffset {
    fn traverse(&mut self, _operate: &mut dyn FnMut(&mut dyn Operation<T>)) {}

    fn scrollable(
        &mut self,
        _id: Option<&iced_core::widget::Id>,
        _bounds: Rectangle,
        _content_bounds: Rectangle,
        _translation: Vector,
        state: &mut dyn operation::Scrollable,
    ) {
        operation::Scrollable::scroll_to(state, self.offset);
    }
}

/// Applies an absolute scroll offset to the inner [`Scrollable`].
fn apply_offset<Message, Theme, Renderer>(
    inner: &mut Scrollable<'_, Message, Theme, Renderer>,
    tree: &mut Tree,
    layout: Layout<'_>,
    renderer: &Renderer,
    position: Vector,
) where
    Theme: scrollable::Catalog,
    Renderer: text::Renderer,
{
    let mut operation = SetOffset {
        offset: AbsoluteOffset {
            x: Some(position.x),
            y: Some(position.y),
        },
    };

    inner.operate(tree, layout, renderer, &mut operation);
}

/// Determines the [`Anchor`] of each axis of the inner [`Scrollable`] and the
/// current absolute offset.
///
/// [`Scrollbar`] does not expose its [`Anchor`], so we infer it by forcing the
/// offset to `0` and observing the resulting translation: a start-anchored
/// scrollable renders `0` at the top, while an end-anchored one renders the
/// maximum translation there. The original offset is restored before
/// returning, all within a single `update`, so the user never sees the probe.
fn detect_anchors<Message, Theme, Renderer>(
    inner: &mut Scrollable<'_, Message, Theme, Renderer>,
    tree: &mut Tree,
    layout: Layout<'_>,
    renderer: &Renderer,
    maximum: Vector,
) -> ((Anchor, Anchor), Vector)
where
    Theme: scrollable::Catalog,
    Renderer: text::Renderer,
{
    let translation = current_translation(inner, tree, layout, renderer);

    let anchor_x = if maximum.x <= 0.0 {
        Anchor::Start
    } else {
        set_offset(inner, tree, layout, renderer, Some(0.0), None);

        if current_translation(inner, tree, layout, renderer).x
            <= maximum.x * 0.5
        {
            Anchor::Start
        } else {
            Anchor::End
        }
    };

    let anchor_y = if maximum.y <= 0.0 {
        Anchor::Start
    } else {
        set_offset(inner, tree, layout, renderer, None, Some(0.0));

        if current_translation(inner, tree, layout, renderer).y
            <= maximum.y * 0.5
        {
            Anchor::Start
        } else {
            Anchor::End
        }
    };

    let anchors = (anchor_x, anchor_y);
    let absolute = to_absolute(translation, anchors, maximum);

    // Restore the real scroll position.
    set_offset(
        inner,
        tree,
        layout,
        renderer,
        Some(absolute.x),
        Some(absolute.y),
    );

    (anchors, absolute)
}

fn set_offset<Message, Theme, Renderer>(
    inner: &mut Scrollable<'_, Message, Theme, Renderer>,
    tree: &mut Tree,
    layout: Layout<'_>,
    renderer: &Renderer,
    x: Option<f32>,
    y: Option<f32>,
) where
    Theme: scrollable::Catalog,
    Renderer: text::Renderer,
{
    let mut operation = SetOffset {
        offset: AbsoluteOffset { x, y },
    };

    inner.operate(tree, layout, renderer, &mut operation);
}

/// Converts a rendered translation back into an absolute offset for the given
/// anchors.
fn to_absolute(
    translation: Vector,
    anchors: (Anchor, Anchor),
    maximum: Vector,
) -> Vector {
    let x = match anchors.0 {
        Anchor::Start => translation.x,
        Anchor::End => maximum.x - translation.x,
    };

    let y = match anchors.1 {
        Anchor::Start => translation.y,
        Anchor::End => maximum.y - translation.y,
    };

    Vector::new(x.clamp(0.0, maximum.x), y.clamp(0.0, maximum.y))
}

/// Clamps a vector into the valid offset range of a scrollable.
fn clamp(position: Vector, maximum: Vector) -> Vector {
    Vector::new(
        position.x.clamp(0.0, maximum.x),
        position.y.clamp(0.0, maximum.y),
    )
}

/// Flips the sign of a delta on end-anchored axes, mirroring
/// `scrollable::Direction::align`.
fn align(anchors: (Anchor, Anchor), delta: Vector) -> Vector {
    Vector::new(
        match anchors.0 {
            Anchor::Start => delta.x,
            Anchor::End => -delta.x,
        },
        match anchors.1 {
            Anchor::Start => delta.y,
            Anchor::End => -delta.y,
        },
    )
}

/// Normalizes a [`mouse::ScrollDelta`] into pixels, mirroring the logic in
/// [`iced_widget::scrollable`].
fn normalize_delta(delta: mouse::ScrollDelta, shift: bool) -> Vector {
    match delta {
        mouse::ScrollDelta::Lines { x, y } => {
            // macOS automatically inverts the axes when Shift is pressed.
            let (x, y) = if cfg!(target_os = "macos") && shift {
                (y, x)
            } else {
                (x, y)
            };

            let movement = if !shift {
                Vector::new(x, y)
            } else {
                Vector::new(y, x)
            };

            -movement * 60.0
        }
        mouse::ScrollDelta::Pixels { x, y } => -Vector::new(x, y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_flips_end_anchors() {
        let delta = Vector::new(3.0, -7.0);

        assert_eq!(align((Anchor::Start, Anchor::Start), delta), delta);
        assert_eq!(
            align((Anchor::End, Anchor::End), delta),
            Vector::new(-3.0, 7.0)
        );
        assert_eq!(
            align((Anchor::Start, Anchor::End), delta),
            Vector::new(3.0, 7.0)
        );
    }

    #[test]
    fn absolute_round_trips_for_both_anchors() {
        let maximum = Vector::new(100.0, 400.0);

        // Start-anchored axes render the absolute offset directly.
        assert_eq!(
            to_absolute(
                Vector::new(25.0, 300.0),
                (Anchor::Start, Anchor::Start),
                maximum
            ),
            Vector::new(25.0, 300.0)
        );

        // End-anchored axes render the reversed offset.
        assert_eq!(
            to_absolute(
                Vector::new(75.0, 100.0),
                (Anchor::End, Anchor::End),
                maximum
            ),
            Vector::new(25.0, 300.0)
        );
    }

    #[test]
    fn clamp_limits_to_scroll_range() {
        let maximum = Vector::new(50.0, 0.0);

        assert_eq!(
            clamp(Vector::new(-10.0, 20.0), maximum),
            Vector::new(0.0, 0.0)
        );
        assert_eq!(
            clamp(Vector::new(10.0, 20.0), maximum),
            Vector::new(10.0, 0.0)
        );
        assert_eq!(
            clamp(Vector::new(100.0, 20.0), maximum),
            Vector::new(50.0, 0.0)
        );
    }

    #[test]
    fn line_deltas_are_scaled_and_inverted() {
        let delta = normalize_delta(
            mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
            false,
        );

        assert_eq!(delta, Vector::new(0.0, -60.0));
    }

    #[test]
    fn pixel_deltas_are_only_inverted() {
        let delta = normalize_delta(
            mouse::ScrollDelta::Pixels { x: 2.0, y: -3.0 },
            false,
        );

        assert_eq!(delta, Vector::new(-2.0, 3.0));
    }

    #[test]
    fn shift_swaps_axes() {
        let delta =
            normalize_delta(mouse::ScrollDelta::Lines { x: 1.0, y: 0.0 }, true);

        assert_eq!(delta, Vector::new(0.0, -60.0));
    }
}

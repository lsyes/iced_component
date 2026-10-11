//! Demonstrates several levels of overlay stacked on top of each other—and
//! that this crate's [`ContextMenuTextInput`] behaves at every level.
//!
//! The window is built out of four layers:
//!
//! 1. The **base** layer: a card that opens a floating panel.
//! 2. The **panel** (first overlay): a card that floats above the base. It has
//!    its own context menu text input, and a button that opens an inspector.
//! 3. The **inspector** (second overlay): a card nested *inside* the panel
//!    overlay, with another context menu text input.
//! 4. The **context menu** (last overlay): right-clicking either input opens it
//!    on top of everything else, no matter how deep the input is buried.
//!
//! Because each level is a real overlay, the menu keeps working even though the
//! input it belongs to is drawn by two nested overlays.
//!
//! Both panels can also be dragged around by grabbing them anywhere that is not
//! an interactive control, which is what a floating panel is for. While a panel
//! is dragged, the application listens to the mouse through a
//! [`Subscription`]—the only way to keep following the pointer once it leaves
//! the panel.
//!
//! Run with:
//!
//! ```text
//! cargo run --example multi_level_overlay
//! ```

use iced::widget::{
    Space, button, column, container, float, mouse_area, row, stack, text,
};
use iced::{
    Alignment, Color, Element, Event, Fill, Length, Point, Rectangle, Shadow,
    Subscription, Task, Vector, Widget, border, mouse,
};
use iced_component::text_input::context_menu_text_input;

pub fn main() -> iced::Result {
    iced::application(Demo::default, Demo::update, Demo::view)
        .subscription(Demo::subscription)
        .window_size((760.0, 560.0))
        .centered()
        .run()
}

struct Demo {
    /// The value of the text input of the panel.
    value: String,
    /// The value of the text input of the inspector.
    notes: String,
    /// Whether the first overlay level is open.
    panel: bool,
    /// Whether the second overlay level is open.
    inspector: bool,
    /// Where the panel is currently floating, relative to the window.
    panel_offset: Vector,
    /// Where the inspector is currently floating, relative to the panel.
    inspector_offset: Vector,
    /// The drag that is currently in progress, if any.
    dragging: Option<Drag>,
}

impl Default for Demo {
    fn default() -> Self {
        Self {
            value: String::new(),
            notes: String::new(),
            panel: false,
            inspector: false,
            panel_offset: Vector::new(56.0, 56.0),
            inspector_offset: Vector::new(320.0, 220.0),
            dragging: None,
        }
    }
}

/// One of the two floating panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    /// The first overlay level.
    Main,
    /// The second overlay level, nested inside the first one.
    Inspector,
}

/// A drag that is in progress.
#[derive(Debug, Clone, Copy)]
struct Drag {
    panel: Panel,
    /// Where the pointer grabbed the panel. `None` until the first mouse
    /// movement is observed, since a press alone does not report its position.
    grab: Option<Vector>,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePanel,
    ToggleInspector,
    ValueChanged(String),
    NotesChanged(String),
    DragStart(Panel),
    DragMove(Point),
    DragEnd,
}

impl Demo {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TogglePanel => {
                self.panel = !self.panel;

                // Closing the panel takes its nested overlay with it.
                self.inspector = false;
            }
            Message::ToggleInspector => self.inspector = !self.inspector,
            Message::ValueChanged(value) => self.value = value,
            Message::NotesChanged(notes) => self.notes = notes,
            Message::DragStart(panel) => {
                self.dragging = Some(Drag { panel, grab: None });
            }
            Message::DragMove(position) => {
                if let Some(drag) = self.dragging {
                    // Where the pointer sits relative to the panel. It is
                    // remembered the first time, so that the panel does not
                    // jump under the cursor.
                    let offset = self.offset(drag.panel);
                    let grab = drag.grab.unwrap_or_else(|| {
                        Vector::new(
                            position.x - offset.x,
                            position.y - offset.y,
                        )
                    });

                    self.set_offset(
                        drag.panel,
                        Vector::new(position.x - grab.x, position.y - grab.y),
                    );

                    self.dragging = Some(Drag {
                        grab: Some(grab),
                        ..drag
                    });
                }
            }
            Message::DragEnd => self.dragging = None,
        }

        Task::none()
    }

    /// Follows the pointer while a panel is being dragged. A press or a release
    /// only reaches the panel through a [`mouse_area`], but the pointer itself
    /// has to be tracked globally—it leaves the panel as soon as the drag
    /// starts moving.
    fn subscription(&self) -> Subscription<Message> {
        if self.dragging.is_none() {
            return Subscription::none();
        }

        iced::event::listen_with(|event, _status, _window| match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Message::DragMove(position))
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Window(iced::window::Event::Unfocused) => {
                Some(Message::DragEnd)
            }
            _ => None,
        })
    }

    fn offset(&self, panel: Panel) -> Vector {
        match panel {
            Panel::Main => self.panel_offset,
            Panel::Inspector => self.inspector_offset,
        }
    }

    fn set_offset(&mut self, panel: Panel, offset: Vector) {
        match panel {
            Panel::Main => self.panel_offset = offset,
            Panel::Inspector => self.inspector_offset = offset,
        }
    }

    fn view(&self) -> Element<'_, Message> {
        // Every overlay is just another layer of a `stack`, so the interface is
        // described by the list of layers that are currently open.
        let mut layers: Vec<Element<'_, Message>> = vec![self.base().boxed()];

        if self.panel {
            layers.push(self.panel().boxed());
        }

        stack(layers).boxed()
    }

    /// The layer underneath everything.
    fn base(&self) -> impl Widget<Message> + '_ {
        container(
            column![
                text("Multi-level overlays").size(26),
                text(
                    "Open the panel, then the inspector inside it, and \
                     right-click either text field. Grab a panel anywhere \
                     to drag it around."
                )
                .size(15)
                .center(),
                text(
                    "The menu always ends up on top—even when the field that \
                     opened it is drawn by two other overlays."
                )
                .size(13)
                .center(),
                button(text("Open the panel")).on_press(Message::TogglePanel),
            ]
            .spacing(16)
            .align_x(Alignment::Center),
        )
        .center(Fill)
        .padding(32)
    }

    /// The first overlay level: a card floating above the base.
    fn panel(&self) -> impl Widget<Message> + '_ {
        let mut children: Vec<Element<'_, Message>> =
            vec![self.draggable(Panel::Main, self.panel_card()).boxed()];

        if self.inspector {
            children.push(self.inspector().boxed());
        }

        // The inspector is a *nested* overlay: `stack` lives inside the content
        // of the `float` below, so the overlays pile up instead of replacing
        // each other.
        let content = stack(children);

        float(content)
            .translate(|bounds, viewport| {
                clamp(self.panel_offset, bounds, viewport)
            })
            .style(shadow)
    }

    /// Grabbing a panel anywhere that is not an interactive control drags it
    /// around—the whole card is the drag handle.
    fn draggable<'a>(
        &'a self,
        panel: Panel,
        content: impl Widget<Message> + 'a,
    ) -> impl Widget<Message> + 'a {
        let is_being_dragged =
            self.dragging.map(|drag| drag.panel) == Some(panel);

        mouse_area(content)
            .on_press(Message::DragStart(panel))
            .on_release(Message::DragEnd)
            .interaction(if is_being_dragged {
                mouse::Interaction::Grabbing
            } else {
                mouse::Interaction::Grab
            })
    }

    fn panel_card(&self) -> impl Widget<Message> + '_ {
        container(
            column![
                row![
                    text("Panel").size(18),
                    Space::new().width(Fill),
                    button(text("Close")).on_press(Message::TogglePanel),
                ]
                .align_y(Alignment::Center),
                text("Overlay #1").size(12),
                self.input(
                    "Right-click me…",
                    &self.value,
                    Message::ValueChanged
                ),
                row![
                    button(text("Open the inspector"))
                        .on_press(Message::ToggleInspector),
                    Space::new().width(8.0),
                    text(if self.inspector {
                        "The inspector is open"
                    } else {
                        "The inspector is closed"
                    })
                    .size(12),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(12),
        )
        .padding(20)
        .width(Length::Fixed(360.0))
        .style(container::rounded_box)
    }

    /// The second overlay level: a card floating above the panel.
    fn inspector(&self) -> impl Widget<Message> + '_ {
        let card = self.draggable(
            Panel::Inspector,
            container(
                column![
                    row![
                        text("Inspector").size(18),
                        Space::new().width(Fill),
                        button(text("Close"))
                            .on_press(Message::ToggleInspector),
                    ]
                    .align_y(Alignment::Center),
                    text("Overlay #2, nested in the panel").size(12),
                    self.input(
                        "Right-click me too…",
                        &self.notes,
                        Message::NotesChanged
                    ),
                ]
                .spacing(12),
            )
            .padding(20)
            .width(Length::Fixed(320.0))
            .style(container::rounded_box),
        );

        float(card)
            .translate(|bounds, viewport| {
                clamp(self.inspector_offset, bounds, viewport)
            })
            .style(shadow)
    }

    /// A [`ContextMenuTextInput`] that opens the context menu—the last overlay
    /// level—when it is right-clicked.
    fn input<'a>(
        &'a self,
        placeholder: &'a str,
        value: &'a str,
        on_input: fn(String) -> Message,
    ) -> Element<'a, Message> {
        context_menu_text_input(placeholder, value)
            .on_input(on_input)
            .padding(10)
            .into()
    }
}

/// Keeps a floating panel inside the window it belongs to.
///
/// The `bounds` and `viewport` handed to a `float` translation are already
/// expressed in the same coordinate space, so this works no matter how deeply
/// the panel is nested—a panel can never be dragged out of reach.
fn clamp(offset: Vector, bounds: Rectangle, viewport: Rectangle) -> Vector {
    fn axis(value: f32, min: f32, max: f32) -> f32 {
        value.clamp(min, max.max(min))
    }

    Vector::new(
        axis(
            offset.x,
            viewport.x - bounds.x,
            viewport.x + viewport.width - bounds.width - bounds.x,
        ),
        axis(
            offset.y,
            viewport.y - bounds.y,
            viewport.y + viewport.height - bounds.height - bounds.y,
        ),
    )
}

/// The shadow that lifts a floating panel off the layers underneath.
fn shadow(_theme: &iced::Theme) -> iced::widget::float::Style {
    iced::widget::float::Style {
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.25),
            offset: Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
        shadow_border_radius: border::radius(12.0),
    }
}

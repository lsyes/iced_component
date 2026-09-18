//! Demonstrates Firefox-style smooth scrolling.
//!
//! Scroll the outer list with the mouse wheel while `Smooth` is selected, then
//! switch to `Instant` to compare it with a regular `Scrollable`.
//!
//! The **Nested list** card in the middle contains a plain
//! [`scrollable::Scrollable`]. Wheeling over it scrolls *that* list, not the
//! outer one; once the nested list is exhausted, the wheel chains back to the
//! outer list—still smoothly.
//!
//! Run with:
//!
//! ```text
//! cargo run --example smooth_scroll
//! ```

use iced::widget::{Space, column, container, radio, row, scrollable, text};
use iced::{Center, Element, Fill, Task};
use iced_component::smooth_scrollable::{
    Direction, ScrollMode, Scrollbar, Viewport, smooth_scrollable,
};

pub fn main() -> iced::Result {
    iced::application(Demo::default, Demo::update, Demo::view)
        .window_size((880.0, 680.0))
        .centered()
        .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Smooth,
    Instant,
}

#[derive(Default)]
struct Demo {
    mode: Mode,
    offset: f32,
    nested_offset: f32,
}

#[derive(Debug, Clone)]
enum Message {
    ModeChanged(Mode),
    Scrolled(Viewport),
    NestedScrolled(Viewport),
}

impl Demo {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ModeChanged(mode) => self.mode = mode,
            Message::Scrolled(viewport) => {
                self.offset = viewport.absolute_offset().y
            }
            Message::NestedScrolled(viewport) => {
                self.nested_offset = viewport.absolute_offset().y;
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let header = column![
            text("Smooth scrolling").size(26),
            text(
                "Firefox-style inertial scrolling for iced's Scrollable. \
                 Wheel over the list, then over the nested list inside the \
                 highlighted card."
            )
            .size(14),
        ]
        .spacing(6);

        let controls = row![
            text("Wheel:").size(14),
            radio(
                "Smooth",
                Mode::Smooth,
                Some(self.mode),
                Message::ModeChanged
            ),
            radio(
                "Instant",
                Mode::Instant,
                Some(self.mode),
                Message::ModeChanged
            ),
            Space::new().width(Fill),
            text!("outer {:.0}px", self.offset).size(14),
            text!("nested {:.0}px", self.nested_offset).size(14),
        ]
        .spacing(16)
        .align_y(Center);

        container(
            column![header, controls, self.list()]
                .spacing(18)
                .height(Fill),
        )
        .padding(24)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn list(&self) -> Element<'_, Message> {
        let items = (0..24).map(|index| {
            if index == 3 {
                nested_list()
            } else {
                card(index)
            }
        });

        smooth_scrollable(column(items).spacing(12))
            .id("smooth-scroll-demo")
            .scroll_mode(match self.mode {
                Mode::Smooth => ScrollMode::Smooth,
                Mode::Instant => ScrollMode::Instant,
            })
            .direction(Direction::Vertical(
                Scrollbar::default().width(10).scroller_width(10),
            ))
            .height(Fill)
            .on_scroll(Message::Scrolled)
            .into()
    }
}

/// A plain, smoothly styled entry of the outer list.
fn card(index: usize) -> Element<'static, Message> {
    container(
        row![
            text!("Card #{index}").size(16),
            Space::new().width(Fill),
            text!("scroll me with the wheel").size(12),
        ]
        .align_y(Center),
    )
    .padding(16)
    .width(Fill)
    .style(container::rounded_box)
    .into()
}

/// A card holding a plain [`scrollable::Scrollable`], to show that wheel
/// events reach a nested scrollable first.
fn nested_list() -> Element<'static, Message> {
    let rows = column(
        (0..60).map(|index| text!("nested row #{index}").size(13).into()),
    )
    .spacing(8);

    let nested = scrollable(rows)
        .id("nested-scroll-demo")
        .height(220)
        .width(Fill)
        .direction(Direction::Vertical(
            Scrollbar::default().width(8).scroller_width(8),
        ))
        .on_scroll(Message::NestedScrolled);

    container(
        column![
            text("Nested list").size(16),
            text(
                "The wheel scrolls this list first. When it reaches the end, \
                 the outer list takes over."
            )
            .size(12),
            nested,
        ]
        .spacing(10),
    )
    .padding(16)
    .width(Fill)
    .style(container::bordered_box)
    .into()
}

//! End-to-end tests for the standalone [`context_menu`] widget, driven by the
//! headless [`iced_test::Simulator`].
//!
//! Unlike the text widgets, this one produces messages, so these tests assert
//! on the messages that the menu entries publish.

use iced_component::context_menu::{self, Entry, ITEM_HEIGHT, PADDING};
use iced_test::core::{Element, Event, Point, Settings, Size, event, mouse};
use iced_test::simulator::{self, Simulator};
use iced_widget::core::Length;
use iced_widget::{Renderer, Theme, container, text};

const VIEWPORT: Size = Size::new(800.0, 400.0);
const ORIGIN: Point = Point::new(40.0, 40.0);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Message {
    Copy,
    Reload,
}

fn view() -> Element<'static, Message, Theme, Renderer> {
    context_menu::context_menu(
        container(text("Right-click me"))
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .entry(Entry::new("Copy", || Message::Copy))
    .entry(Entry::new("Reload", || Message::Reload).enabled(false))
    .into()
}

/// The center of the menu entry with the given index, for a menu anchored at
/// `origin`.
fn entry_center(origin: Point, index: usize) -> Point {
    Point::new(
        origin.x + 20.0,
        origin.y + PADDING + index as f32 * ITEM_HEIGHT + ITEM_HEIGHT / 2.0,
    )
}

fn right_click(ui: &mut Simulator<'_, Message, Theme, Renderer>, at: Point) {
    ui.point_at(at);

    let statuses = ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Right,
    ))]);

    assert_eq!(
        statuses[0],
        event::Status::Captured,
        "a right-click should open the menu"
    );
}

fn left_click(ui: &mut Simulator<'_, Message, Theme, Renderer>, at: Point) {
    ui.point_at(at);

    let _ = ui.simulate(simulator::click());
}

#[test]
fn choosing_an_entry_publishes_its_message() {
    let mut ui = Simulator::with_size(Settings::default(), VIEWPORT, view());

    right_click(&mut ui, ORIGIN);
    left_click(&mut ui, entry_center(ORIGIN, 0));

    let messages: Vec<Message> = ui.into_messages().collect();

    assert_eq!(messages, vec![Message::Copy]);
}

#[test]
fn a_disabled_entry_does_nothing() {
    let mut ui = Simulator::with_size(Settings::default(), VIEWPORT, view());

    right_click(&mut ui, ORIGIN);
    left_click(&mut ui, entry_center(ORIGIN, 1));

    let messages: Vec<Message> = ui.into_messages().collect();

    assert!(
        messages.is_empty(),
        "a disabled entry should not publish anything, got {messages:?}"
    );
}

#[test]
fn clicking_outside_the_menu_closes_it() {
    let mut ui = Simulator::with_size(Settings::default(), VIEWPORT, view());

    right_click(&mut ui, ORIGIN);

    // A click far away from the menu dismisses it...
    let elsewhere = Point::new(600.0, 300.0);
    left_click(&mut ui, elsewhere);

    // ...so the entries are gone, but the widget can still be right-clicked.
    right_click(&mut ui, ORIGIN);
    left_click(&mut ui, entry_center(ORIGIN, 0));

    let messages: Vec<Message> = ui.into_messages().collect();

    assert_eq!(messages, vec![Message::Copy]);
}

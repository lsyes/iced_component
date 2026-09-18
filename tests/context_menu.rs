//! End-to-end tests for the right-click context menu, driven by the headless
//! [`iced_test::Simulator`].
//!
//! The context menu is drawn by a custom [`overlay`](iced_widget::core::overlay),
//! so its items are not discoverable through widget operations. Instead, these
//! tests compute the menu geometry deterministically and drive real mouse
//! events at those coordinates.

use iced_component::text_input::context_menu_text_input;
use iced_test::core::{Element, Event, Point, Settings, Size, event, mouse};
use iced_test::simulator::{self, Simulator};
use iced_widget::{Renderer, Theme};

const VIEWPORT: Size = Size::new(1024.0, 768.0);

/// The menu is anchored at the right-click position and laid out with these
/// constants (see `text_input`):
const MENU_PADDING: f32 = 4.0;
const MENU_ITEM_HEIGHT: f32 = 28.0;

/// A point that is inside the text input of every [`view`] below.
const INSIDE: Point = Point::new(10.0, 10.0);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Message {
    Changed(String),
}

fn view(value: &'static str) -> Element<'static, Message, Theme, Renderer> {
    context_menu_text_input("Type here", value)
        .on_input(Message::Changed)
        .into()
}

fn view_without_menu(
    value: &'static str,
) -> Element<'static, Message, Theme, Renderer> {
    context_menu_text_input("Type here", value)
        .context_menu(false)
        .on_input(Message::Changed)
        .into()
}

fn secure_view(
    value: &'static str,
) -> Element<'static, Message, Theme, Renderer> {
    context_menu_text_input("Type here", value)
        .secure(true)
        .on_input(Message::Changed)
        .into()
}

/// The vertical center of the menu item with the given index, for a menu
/// anchored at `origin`.
fn item_center(origin: Point, index: usize) -> Point {
    Point::new(
        origin.x + 70.0,
        origin.y
            + MENU_PADDING
            + index as f32 * MENU_ITEM_HEIGHT
            + MENU_ITEM_HEIGHT / 2.0,
    )
}

fn right_click(
    ui: &mut Simulator<'_, Message, Theme, Renderer>,
    at: Point,
) -> event::Status {
    ui.point_at(at);

    ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Right,
    ))])[0]
}

fn left_click(ui: &mut Simulator<'_, Message, Theme, Renderer>, at: Point) {
    ui.point_at(at);

    let _ = ui.simulate(simulator::click());
}

fn open_menu(ui: &mut Simulator<'_, Message, Theme, Renderer>, origin: Point) {
    assert_eq!(
        right_click(ui, origin),
        event::Status::Captured,
        "a right-click should open the context menu"
    );
}

#[test]
fn selecting_all_then_cutting_clears_the_input() {
    let mut ui = Simulator::with_size(
        Settings::default(),
        VIEWPORT,
        view("hello world"),
    );

    // Right-click to open the menu; "Select All" is the fourth item.
    open_menu(&mut ui, INSIDE);
    left_click(&mut ui, item_center(INSIDE, 3));

    // Re-open the menu and "Cut" the selection; "Cut" is the first item. The
    // selection must have survived the second right-click.
    open_menu(&mut ui, INSIDE);
    left_click(&mut ui, item_center(INSIDE, 0));

    let messages: Vec<Message> = ui.into_messages().collect();

    assert_eq!(messages, vec![Message::Changed(String::new())]);
}

#[test]
fn copy_does_not_change_the_value() {
    let mut ui = Simulator::with_size(
        Settings::default(),
        VIEWPORT,
        view("hello world"),
    );

    open_menu(&mut ui, INSIDE);
    left_click(&mut ui, item_center(INSIDE, 3)); // Select All

    open_menu(&mut ui, INSIDE);
    left_click(&mut ui, item_center(INSIDE, 1)); // Copy

    let messages: Vec<Message> = ui.into_messages().collect();

    assert!(
        messages.is_empty(),
        "copying should not produce an input message, got {messages:?}"
    );
}

#[test]
fn clicking_outside_the_menu_closes_it() {
    let mut ui = Simulator::with_size(
        Settings::default(),
        VIEWPORT,
        view("hello world"),
    );

    open_menu(&mut ui, INSIDE);

    // A left click far away from the menu should dismiss it and reach the
    // text input (which simply unfocuses).
    left_click(&mut ui, Point::new(900.0, 700.0));

    // Opening the menu again and clicking "Select All" must still work, which
    // proves the previous menu was dismissed and did not swallow the click.
    open_menu(&mut ui, INSIDE);
    left_click(&mut ui, item_center(INSIDE, 3)); // Select All

    open_menu(&mut ui, INSIDE);
    left_click(&mut ui, item_center(INSIDE, 0)); // Cut

    let messages: Vec<Message> = ui.into_messages().collect();

    assert_eq!(messages, vec![Message::Changed(String::new())]);
}

#[test]
fn the_input_keeps_working_like_the_built_in_one() {
    let mut ui = Simulator::with_size(Settings::default(), VIEWPORT, view(""));

    // Focus the input by clicking it...
    left_click(&mut ui, INSIDE);

    // ...then type, exactly as with the built-in text input.
    let _ = ui.typewrite("hi");

    let messages: Vec<Message> = ui.into_messages().collect();

    assert_eq!(
        messages,
        vec![
            Message::Changed(String::from("h")),
            Message::Changed(String::from("hi"))
        ]
    );
}

#[test]
fn the_context_menu_can_be_disabled() {
    let mut ui = Simulator::with_size(
        Settings::default(),
        VIEWPORT,
        view_without_menu("hello"),
    );

    assert_eq!(
        right_click(&mut ui, INSIDE),
        event::Status::Ignored,
        "a disabled context menu should not capture the right-click"
    );

    // Nothing was opened, so clicking where a menu item would be does nothing.
    left_click(&mut ui, item_center(INSIDE, 0));

    let messages: Vec<Message> = ui.into_messages().collect();

    assert!(
        messages.is_empty(),
        "a disabled context menu should not produce messages, got {messages:?}"
    );
}

#[test]
fn a_secure_input_disables_cut_and_copy() {
    let mut ui = Simulator::with_size(
        Settings::default(),
        VIEWPORT,
        secure_view("hunter2"),
    );

    open_menu(&mut ui, INSIDE);
    left_click(&mut ui, item_center(INSIDE, 3)); // Select All

    open_menu(&mut ui, INSIDE);
    left_click(&mut ui, item_center(INSIDE, 0)); // Cut (disabled)

    let messages: Vec<Message> = ui.into_messages().collect();

    assert!(
        messages.is_empty(),
        "cutting a secure value should do nothing, got {messages:?}"
    );
}

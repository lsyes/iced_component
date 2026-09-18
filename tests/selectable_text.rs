//! End-to-end tests for [`selectable_text`], driven by the headless
//! [`iced_test::Simulator`].
//!
//! The selection itself lives inside the renderer's editor and cannot be
//! inspected from the outside, so these tests assert what *is* observable: the
//! widget is discoverable by its text, it reacts to the pointer and to the
//! keyboard only while it is focused, and its context menu opens, swallows the
//! clicks that land on it, and closes again.

use iced_component::context_menu::{ITEM_HEIGHT, PADDING};
use iced_component::selectable_text::selectable_text;
use iced_test::core::{
    Element, Event, Point, Settings, Size, event, keyboard, mouse,
};
use iced_test::simulator::{self, Simulator};
use iced_widget::core::Length;
use iced_widget::{Renderer, Theme, container};

const VIEWPORT: Size = Size::new(800.0, 400.0);
const TEXT: &str = "hello selectable world";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Message {}

fn view() -> Element<'static, Message, Theme, Renderer> {
    container(selectable_text(TEXT).size(20))
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn view_without_menu() -> Element<'static, Message, Theme, Renderer> {
    container(selectable_text(TEXT).size(20).context_menu(false))
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
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

fn left_click(
    ui: &mut Simulator<'_, Message, Theme, Renderer>,
    at: Point,
) -> event::Status {
    ui.point_at(at);

    ui.simulate(simulator::click())
        .first()
        .copied()
        .unwrap_or(event::Status::Ignored)
}

/// A `Ctrl`/`Cmd` + `key` shortcut, preceded by the matching modifier event.
fn shortcut(key: char) -> [Event; 2] {
    use keyboard::key::Code;

    let code = match key {
        'a' => Code::KeyA,
        'c' => Code::KeyC,
        _ => panic!("unsupported shortcut"),
    };
    let key = keyboard::Key::Character(key.to_string().into());

    [
        Event::Keyboard(keyboard::Event::ModifiersChanged(
            keyboard::Modifiers::COMMAND,
        )),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Code(code),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::COMMAND,
            text: None,
            repeat: false,
        }),
    ]
}

#[test]
fn the_text_is_discoverable() {
    let mut ui = Simulator::with_size(Settings::default(), VIEWPORT, view());

    let target = ui.find(TEXT).expect("the text should be discoverable");

    assert!(
        target.visible_bounds().is_some(),
        "the text should be visible"
    );
}

#[test]
fn the_keyboard_only_reacts_while_focused() {
    let mut ui = Simulator::with_size(Settings::default(), VIEWPORT, view());

    let bounds = ui
        .find(TEXT)
        .expect("the text should be discoverable")
        .bounds();

    // Nothing is focused yet.
    assert_eq!(
        ui.simulate(shortcut('a'))[1],
        event::Status::Ignored,
        "an unfocused text should ignore keyboard shortcuts"
    );

    // Clicking the text focuses it...
    assert_eq!(
        left_click(&mut ui, bounds.center()),
        event::Status::Captured
    );

    // ...and then the shortcuts are handled.
    assert_eq!(
        ui.simulate(shortcut('a'))[1],
        event::Status::Captured,
        "a focused text should handle `Select All`"
    );

    assert_eq!(
        ui.simulate(shortcut('c'))[1],
        event::Status::Captured,
        "a focused text should handle `Copy`"
    );

    // Clicking somewhere else takes the focus away again.
    let elsewhere = Point::new(700.0, 300.0);

    assert_eq!(left_click(&mut ui, elsewhere), event::Status::Ignored);

    assert_eq!(
        ui.simulate(shortcut('c'))[1],
        event::Status::Ignored,
        "clicking outside should unfocus the text"
    );
}

#[test]
fn the_context_menu_swallows_the_clicks_that_land_on_it() {
    let mut ui = Simulator::with_size(Settings::default(), VIEWPORT, view());

    let bounds = ui
        .find(TEXT)
        .expect("the text should be discoverable")
        .bounds();

    // Open the menu close to the right edge of the text, so that it extends
    // beyond it.
    let origin = Point::new(bounds.x + bounds.width - 4.0, bounds.y + 4.0);

    assert_eq!(
        right_click(&mut ui, origin),
        event::Status::Captured,
        "a right-click should open the context menu"
    );

    // "Copy" is the first entry, "Select All" the second.
    let select_all = Point::new(
        origin.x + 20.0,
        origin.y + PADDING + ITEM_HEIGHT + ITEM_HEIGHT / 2.0,
    );

    assert!(
        !bounds.contains(select_all),
        "the entry should sit outside the text, so that the assertion below is meaningful"
    );

    assert_eq!(
        left_click(&mut ui, select_all),
        event::Status::Captured,
        "a click on a menu entry should be swallowed"
    );

    assert_eq!(
        left_click(&mut ui, select_all),
        event::Status::Ignored,
        "choosing an entry should close the menu"
    );
}

#[test]
fn the_context_menu_can_be_disabled() {
    let mut ui = Simulator::with_size(
        Settings::default(),
        VIEWPORT,
        view_without_menu(),
    );

    let bounds = ui
        .find(TEXT)
        .expect("the text should be discoverable")
        .bounds();

    assert_eq!(
        right_click(&mut ui, bounds.center()),
        event::Status::Ignored,
        "a disabled context menu should not capture the right-click"
    );
}

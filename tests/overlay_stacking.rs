//! Regression tests for how the context menu is layered with other overlays.
//!
//! The menu is a transient, top-most surface: it has to be visible above
//! everything else and it has to keep working when the widget it is attached to
//! happens to sit under—or inside—another overlay.

use iced::Vector;
use iced_component::context_menu::{self, Entry};
use iced_component::text_input::context_menu_text_input;
use iced_test::core::{Event, Point, Settings, Size, event, mouse};
use iced_test::simulator::{self, Simulator};
use iced_widget::core::Length;
use iced_widget::{
    Renderer, Theme, Widget, column, container, float, pick_list, stack, text,
};

const VIEWPORT: Size = Size::new(800.0, 600.0);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Message {
    Copy,
    Selected(String),
    Text(String),
}

/// A menu anchored at `origin` lays its entries out after a [`PADDING`] pixels
/// border, each of them [`ITEM_HEIGHT`] pixels tall. This is the center of the
/// first one.
///
/// [`ITEM_HEIGHT`]: iced_component::context_menu::ITEM_HEIGHT
/// [`PADDING`]: iced_component::context_menu::PADDING
fn first_entry(origin: Point) -> Point {
    use iced_component::context_menu::{ITEM_HEIGHT, PADDING};

    Point::new(origin.x + 100.0, origin.y + PADDING + ITEM_HEIGHT / 2.0)
}

fn menu() -> impl Widget<Message, Theme, Renderer> {
    context_menu::context_menu(container(text("Right-click me")).padding(20))
        .entry(Entry::new("Copy", || Message::Copy))
}

/// The menu must still open when another overlay—here, an open pick list
/// dropdown—sits between the cursor and the widget it is attached to. Its
/// levitating cursor must not stop the menu from appearing.
#[test]
fn the_menu_opens_underneath_another_overlay() {
    let view = || {
        column![
            pick_list(Some("one"), &["one", "two"][..], |s: &&str| {
                s.to_string()
            })
            .on_select(|s: &str| Message::Selected(s.to_string())),
            menu(),
        ]
        .padding(20)
    };

    let mut ui = Simulator::with_size(Settings::default(), VIEWPORT, view());

    // Open the pick list dropdown...
    let pick = ui
        .find("one")
        .expect("the pick list is discoverable")
        .bounds();
    ui.point_at(pick.center());
    let _ = ui.simulate(simulator::click());

    let dropdown = ui
        .find("two")
        .expect("the dropdown should be open")
        .bounds();

    // ...and right-click the menu target, right where the dropdown covers it.
    let target = ui
        .find("Right-click me")
        .expect("the target is discoverable")
        .bounds();

    let origin = target
        .intersection(&dropdown)
        .expect("the dropdown should cover the target")
        .center();

    assert!(
        target.contains(origin) && dropdown.contains(origin),
        "the cursor should sit under the dropdown for this test to mean anything"
    );

    ui.point_at(origin);

    assert_eq!(
        ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Right,
        ))])[0],
        event::Status::Captured,
        "the menu should open even with an overlay above the cursor"
    );

    // Click the first entry of the menu, away from the dropdown.
    let entry = first_entry(origin);

    assert!(
        !dropdown.contains(entry),
        "the entry should sit outside the dropdown, so that the click can only reach the menu"
    );

    ui.point_at(entry);
    let _ = ui.simulate(simulator::click());

    let messages: Vec<Message> = ui.into_messages().collect();

    assert_eq!(messages, vec![Message::Copy]);
}

/// The menu must be reachable when the whole [`context_menu`] widget is itself
/// rendered by another overlay, like a translated [`float`].
#[test]
fn the_menu_works_inside_another_overlay() {
    let view = || {
        float(menu()).translate(|_bounds, _viewport| Vector::new(10.0, 10.0))
    };

    let mut ui = Simulator::with_size(Settings::default(), VIEWPORT, view());

    let origin = ui
        .find("Right-click me")
        .expect("the target is discoverable")
        .bounds()
        .center();

    ui.point_at(origin);

    assert_eq!(
        ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Right,
        ))])[0],
        event::Status::Captured,
        "the menu should open inside another overlay"
    );

    ui.point_at(first_entry(origin));
    let _ = ui.simulate(simulator::click());

    let messages: Vec<Message> = ui.into_messages().collect();

    assert_eq!(messages, vec![Message::Copy]);
}

/// A menu opened from a text input that is itself drawn by *two* nested
/// overlays—a floating panel, and a floating inspector inside it—must still
/// open, on top of everything, and stay wired to the input.
#[test]
fn the_menu_works_from_a_text_input_inside_two_overlays() {
    /// Where the inspector floats, relative to the panel.
    const INSPECTOR: Vector = Vector::new(240.0, 160.0);
    /// Where the panel floats, relative to the window.
    const PANEL: Vector = Vector::new(40.0, 40.0);

    fn view() -> impl Widget<Message, Theme, Renderer> {
        float(stack![
            container(text("Right-click me"))
                .padding(20)
                .width(Length::Fixed(200.0)),
            float(
                context_menu_text_input("Type here", "")
                    .on_input(Message::Text)
            )
            .translate(|_bounds, _viewport| INSPECTOR),
        ])
        .translate(|_bounds, _viewport| PANEL)
    }

    let mut ui = Simulator::with_size(Settings::default(), VIEWPORT, view());

    // Widget operations do not report the transformation of a `float` overlay,
    // so the input is discovered in the coordinate space of the card it lives
    // in. Both translations have to be added back to reach the window.
    let local = ui
        .find("Type here")
        .expect("the input is discoverable")
        .bounds();

    let origin = local.center() + PANEL + INSPECTOR;

    // Focus the input and type, which can only work if the click travelled
    // through both overlays.
    ui.point_at(origin);
    let _ = ui.simulate(simulator::click());
    let _ = ui.typewrite("hi");

    assert!(
        ui.drain()
            .any(|message| message == Message::Text(String::from("hi"))),
        "the input inside two overlays should receive the keyboard"
    );

    // Right-click it: the menu is the third overlay level.
    ui.point_at(origin);

    assert_eq!(
        ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Right,
        ))])[0],
        event::Status::Captured,
        "the menu should open on top of both overlays"
    );

    // "Select All" is the fourth entry; "Cut" is the first.
    let entry = |index: usize| {
        use iced_component::context_menu::{ITEM_HEIGHT, PADDING};

        Point::new(
            origin.x + 100.0,
            origin.y + PADDING + index as f32 * ITEM_HEIGHT + ITEM_HEIGHT / 2.0,
        )
    };

    for index in [3, 0] {
        ui.point_at(entry(index));
        let _ = ui.simulate(simulator::click());

        if index == 3 {
            ui.point_at(origin);
            let _ = ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right,
            ))]);
        }
    }

    let messages: Vec<Message> = ui.into_messages().collect();

    assert!(
        messages.contains(&Message::Text(String::new())),
        "the menu should have cut the value of the buried input, got {messages:?}"
    );
}

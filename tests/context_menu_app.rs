//! End-to-end tests for the application-managed context menu.
//!
//! The menu is drawn by layers of the root stack, so these tests build a small
//! application state around it and rebuild the interface between simulated
//! events, exactly like an `iced` application does.

use iced_component::context_menu::app::{self, Event as MenuEvent};
use iced_test::core::window as core_window;
use iced_test::core::{Element, Event, Point, Settings, Size, event, mouse};
use iced_test::simulator::{self, Simulator};
use iced_widget::core::{Length, Vector};
use iced_widget::{
    Renderer, Theme, Widget, column, container, float, stack, text,
};

const VIEWPORT: Size = Size::new(800.0, 600.0);

/// Where the floating card sits in the window.
const ORIGIN: Point = Point::new(40.0, 40.0);

/// The size of the floating card.
const CARD: Size = Size::new(200.0, 100.0);

#[derive(Debug, Clone, PartialEq)]
enum Message {
    Menu(MenuEvent),
}

struct App {
    menu: app::State,
    opened_at: Option<Point>,
    selected: Option<usize>,
}

impl App {
    fn new() -> Self {
        let mut menu = app::State::new();
        menu.set_items([
            String::from("One"),
            String::from("Two"),
            String::from("Three"),
        ]);
        menu.set_window_size(VIEWPORT);

        Self {
            menu,
            opened_at: None,
            selected: None,
        }
    }

    fn update(&mut self, message: Message) {
        let Message::Menu(event) = message;

        if let MenuEvent::Open(position) = event {
            self.opened_at = Some(position);
        }

        if let Some(index) = self.menu.update(event, &Theme::Light) {
            self.selected = Some(index);
        }
    }

    fn view(&self) -> Element<'_, Message, Theme, Renderer> {
        // The card is translated into place by a `float`, just like a
        // scrollable translates its contents. The wrapper only sees the cursor
        // in the card's coordinate space, while the menu has to land at the
        // pointer in window coordinates.
        let card = container(text("Right-click me"))
            .padding(20)
            .width(Length::Fixed(CARD.width))
            .height(Length::Fixed(CARD.height));

        let target = self.menu.wrap(card, Message::Menu);

        let content = float(target)
            .translate(|_bounds, _viewport| Vector::new(ORIGIN.x, ORIGIN.y));

        let mut layers: Vec<Element<'_, Message, Theme, Renderer>> =
            vec![content.boxed()];
        layers.extend(self.menu.layers(Message::Menu));

        stack(layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .boxed()
    }
}

/// The center of the floating card, in window coordinates.
fn card_center() -> Point {
    Point::new(ORIGIN.x + CARD.width / 2.0, ORIGIN.y + CARD.height / 2.0)
}

fn right_click(ui: &mut Simulator<'_, Message, Theme, Renderer>, at: Point) {
    ui.point_at(at);

    let statuses = ui.simulate([
        Event::Mouse(mouse::Event::CursorMoved { position: at }),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
    ]);

    assert_eq!(
        statuses[1],
        event::Status::Captured,
        "a right-click should open the context menu"
    );
}

#[test]
fn the_menu_opens_where_the_pointer_is_inside_a_translated_float() {
    let mut app = App::new();

    let messages: Vec<Message> = {
        let mut ui =
            Simulator::with_size(Settings::default(), VIEWPORT, app.view());

        right_click(&mut ui, card_center());

        ui.into_messages().collect()
    };

    for message in messages {
        app.update(message);
    }

    assert_eq!(
        app.opened_at,
        Some(card_center()),
        "the menu should open at the pointer in window coordinates, not at the \
         translated cursor position"
    );

    // The menu is now open. Its entries have to be drawn around the pointer,
    // not at the origin of the card.
    let mut ui =
        Simulator::with_size(Settings::default(), VIEWPORT, app.view());

    let entry = ui
        .find("One")
        .expect("the first entry should be visible")
        .bounds();

    assert!(
        entry.x >= card_center().x - 1.0 && entry.x < card_center().x + 60.0,
        "the entry should be drawn at the click, got {entry:?}"
    );
    assert!(
        entry.y >= card_center().y - 1.0 && entry.y < card_center().y + 60.0,
        "the entry should be drawn at the click, got {entry:?}"
    );

    // Choosing the entry reports its index.
    ui.point_at(entry.center());
    let _ = ui.simulate(simulator::click());

    let messages: Vec<Message> = ui.into_messages().collect();

    for message in messages {
        app.update(message);
    }

    assert_eq!(app.selected, Some(0));
    assert!(!app.menu.is_open());
}

#[test]
fn clicking_the_backdrop_dismisses_the_menu() {
    let mut app = App::new();
    app.menu.open(card_center());

    let mut ui =
        Simulator::with_size(Settings::default(), VIEWPORT, app.view());

    // Click far away from both the card and the menu.
    let elsewhere = Point::new(700.0, 520.0);
    ui.point_at(elsewhere);
    let _ = ui.simulate(simulator::click());

    let messages: Vec<Message> = ui.into_messages().collect();

    for message in messages {
        app.update(message);
    }

    assert!(!app.menu.is_open());
    assert_eq!(app.selected, None);
}

/// Captures the translation of the first scrollable it visits.
#[derive(Default)]
struct CaptureTranslation(Option<Vector>);

impl iced_widget::core::widget::Operation for CaptureTranslation {
    fn traverse(
        &mut self,
        operate: &mut dyn FnMut(&mut dyn iced_widget::core::widget::Operation),
    ) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        _id: Option<&iced_widget::core::widget::Id>,
        _bounds: iced_widget::core::Rectangle,
        _content: iced_widget::core::Size,
        translation: Vector,
        _state: &mut dyn iced_widget::core::widget::operation::Scrollable,
    ) {
        self.0 = Some(translation);
    }
}

struct ListApp {
    menu: app::State,
    opened_at: Option<Point>,
}

impl ListApp {
    fn new() -> Self {
        let mut menu = app::State::new();
        menu.set_items([String::from("One"), String::from("Two")]);
        menu.set_window_size(VIEWPORT);

        Self {
            menu,
            opened_at: None,
        }
    }

    fn update(&mut self, message: Message) {
        let Message::Menu(event) = message;

        if let MenuEvent::Open(position) = event {
            self.opened_at = Some(position);
        }

        let _ = self.menu.update(event, &Theme::Light);
    }

    fn view(&self) -> Element<'_, Message, Theme, Renderer> {
        let rows = (0..40)
            .map(|index| {
                let row = container(text(format!("Row {index}")))
                    .padding(12)
                    .width(Length::Fill)
                    .height(Length::Fixed(48.0));

                self.menu.wrap(row, Message::Menu)
            })
            .collect::<Vec<_>>();

        let list = iced_widget::scrollable(column(rows))
            .width(Length::Fixed(500.0))
            .height(Length::Fixed(200.0))
            .smooth_scroll(false);

        let mut layers: Vec<Element<'_, Message, Theme, Renderer>> =
            vec![list.boxed()];
        layers.extend(self.menu.layers(Message::Menu));

        stack(layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .boxed()
    }
}

#[test]
fn the_menu_opens_where_the_pointer_is_after_scrolling() {
    let mut app = ListApp::new();
    let clicked: Option<Point>;

    let messages: Vec<Message> = {
        let mut ui =
            Simulator::with_size(Settings::default(), VIEWPORT, app.view());

        // Scroll the list down, then let the scrollable settle its translation.
        ui.point_at(Point::new(200.0, 100.0));

        let _ = ui.scroll(mouse::ScrollDelta::Pixels { x: 0.0, y: -350.0 });
        let _ = ui.simulate([Event::Window(
            core_window::Event::RedrawRequested(std::time::Instant::now()),
        )]);

        let mut translation = CaptureTranslation::default();
        ui.operate(&mut translation);

        let translation = translation.0.expect("the list should be scrollable");
        assert!(
            translation.y > 0.0,
            "the list should have scrolled down, got {translation:?}"
        );

        // `find` reports bounds in the content space of the scrollable, which
        // is translated by its scroll offset when drawn in the window.
        let row = ui
            .find("Row 10")
            .expect("the row should be discoverable")
            .bounds();

        let window_position =
            Point::new(row.center().x, row.center().y - translation.y);

        assert!(
            window_position.y >= 0.0 && window_position.y <= 200.0,
            "the row should be visible after scrolling, got {window_position:?}"
        );

        right_click(&mut ui, window_position);
        clicked = Some(window_position);

        ui.into_messages().collect()
    };

    for message in messages {
        app.update(message);
    }

    assert_eq!(app.opened_at, clicked);
}

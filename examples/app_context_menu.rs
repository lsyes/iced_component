//! Demonstrates the application-managed context menu.
//!
//! The menu is described by a list of labels and reports the index of the
//! chosen entry, so a single [`app::State`] can be reused by every row of the
//! list. Right-click a row and notice that the menu always appears underneath
//! the pointer—even after the list has been scrolled.
//!
//! Run with:
//!
//! ```text
//! cargo run --example app_context_menu
//! ```
//!
//! On Wayland, enable the native `xdg_popup` backend with:
//!
//! ```text
//! cargo run --example app_context_menu --features native
//! ```

use iced::widget::{Space, column, container, row, scrollable, stack, text};
use iced::{
    Alignment, Element, Fill, Length, Size, Subscription, Task, Widget,
};
use iced_component::context_menu::app::{self, Event as MenuEvent};

pub fn main() -> iced::Result {
    iced::application(Demo::new, Demo::update, Demo::view)
        .subscription(Demo::subscription)
        .window_size((760.0, 560.0))
        .centered()
        .run()
}

struct Demo {
    menu: app::State,
    selected: Option<usize>,
}

#[derive(Debug, Clone)]
enum Message {
    Menu(MenuEvent),
    Resized(Size),
    #[cfg(all(feature = "native", target_os = "linux"))]
    WindowHandle(Option<(usize, usize)>),
}

impl Demo {
    fn new() -> (Self, Task<Message>) {
        let mut menu = app::State::new();
        menu.set_items((1..=6).map(|index| format!("Action {index}")));

        #[allow(unused_mut)]
        let mut tasks = vec![
            iced::window::latest()
                .and_then(iced::window::size)
                .map(Message::Resized),
        ];

        #[cfg(all(feature = "native", target_os = "linux"))]
        tasks.push(capture_window_handle());

        (
            Self {
                menu,
                selected: None,
            },
            iced::Task::batch(tasks),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Menu(event) => {
                if let Some(index) =
                    self.menu.update(event, &iced::Theme::Light)
                {
                    self.selected = Some(index);
                }
            }
            Message::Resized(size) => {
                // Lets the menu flip upwards near the bottom edge.
                self.menu.set_window_size(size);
            }
            #[cfg(all(feature = "native", target_os = "linux"))]
            Message::WindowHandle(handle) => {
                // Enables the native `xdg_popup`; without a handle the menu
                // falls back to the in-application panel.
                self.menu.set_window_handle(handle);
            }
        }

        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::window::resize_events()
                .map(|(_id, size)| Message::Resized(size)),
            self.menu.subscription().map(Message::Menu),
        ])
    }

    fn view(&self) -> Element<'_, Message> {
        let rows = (0..40)
            .map(|index| {
                let row = container(
                    row![
                        text(format!("Row {index}")).size(15),
                        Space::new().width(Fill),
                        text("right-click me").size(12),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding(12)
                .width(Length::Fill)
                .style(container::rounded_box);

                self.menu.wrap(row, Message::Menu)
            })
            .collect::<Vec<_>>();

        let content = column![
            text("Application-managed context menu").size(24),
            text(
                "Right-click any row. The menu is a layer of the root stack, \
                 so it stays glued to the pointer even while the list is \
                 scrolled."
            )
            .size(14),
            scrollable(column(rows).spacing(6).padding(12))
                .height(Length::Fill),
            text(match self.selected {
                Some(index) => format!("Chose action {}", index + 1),
                None => String::from("Nothing chosen yet"),
            })
            .size(13),
        ]
        .spacing(16)
        .padding(24)
        .width(Fill)
        .height(Fill);

        let mut layers: Vec<Element<'_, Message>> = vec![content.boxed()];
        layers.extend(self.menu.layers(Message::Menu));

        stack(layers).width(Fill).height(Fill).boxed()
    }
}

/// Captures the raw Wayland handles of the main window, so that a native
/// `xdg_popup` can be parented to it.
#[cfg(all(feature = "native", target_os = "linux"))]
fn capture_window_handle() -> Task<Message> {
    iced::window::latest().and_then(|id| {
        iced::window::run(id, |window| app::wayland_handle(&window))
            .map(Message::WindowHandle)
    })
}

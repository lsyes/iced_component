//! Native Wayland context menus for the text widgets.
//!
//! Both the text input and the selectable text own an application-side
//! [`native::Menu`](iced_component::context_menu::native::Menu): the widget uses
//! it for a native `xdg_popup`, while the application pumps it and feeds the
//! window handles.
//!
//! Run with the native backend:
//!
//! ```text
//! cargo run --example native_text_menu --features native
//! ```
//!
//! Without the feature—or on a non-Wayland session—the exact same example
//! falls back to the in-application overlay menu.

use iced::widget::{Space, column, container, text};
use iced::{
    Alignment, Element, Fill, Length, Size, Subscription, Task, Theme, Widget,
};
use iced_component::context_menu::native::{self, Menu as NativeMenu};
use iced_component::selectable_text::selectable_text;
use iced_component::text_input::context_menu_text_input;

pub fn main() -> iced::Result {
    iced::application(Demo::new, Demo::update, Demo::view)
        .subscription(Demo::subscription)
        .theme(|_: &Demo| Theme::Light)
        .window_size((640.0, 460.0))
        .centered()
        .run()
}

struct Demo {
    value: String,
    text_menu: NativeMenu,
    select_menu: NativeMenu,
}

#[derive(Debug, Clone)]
enum Message {
    Changed(String),
    TextMenu(native::Event),
    SelectMenu(native::Event),
    Resized(Size),
    #[cfg(all(feature = "native", target_os = "linux"))]
    WindowHandle(Option<(usize, usize)>),
}

impl Demo {
    fn new() -> (Self, Task<Message>) {
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
                value: String::from("hello native menu"),
                text_menu: NativeMenu::new(),
                select_menu: NativeMenu::new(),
            },
            Task::batch(tasks),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Changed(value) => self.value = value,
            Message::TextMenu(event) => self.text_menu.update(event),
            Message::SelectMenu(event) => self.select_menu.update(event),
            Message::Resized(size) => {
                self.text_menu.set_window_size(size);
                self.select_menu.set_window_size(size);
            }
            #[cfg(all(feature = "native", target_os = "linux"))]
            Message::WindowHandle(handle) => {
                self.text_menu.set_window_handle(handle);
                self.select_menu.set_window_handle(handle);
            }
        }

        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::window::resize_events()
                .map(|(_id, size)| Message::Resized(size)),
            self.text_menu.subscription().map(Message::TextMenu),
            self.select_menu.subscription().map(Message::SelectMenu),
        ])
    }

    fn view(&self) -> Element<'_, Message> {
        let card = container(
            column![
                text("Native text menus").size(24),
                text(
                    "Right-click either widget. The menu is a native xdg_popup \
                     when the native feature is enabled and a Wayland handle is \
                     available; otherwise it falls back to the overlay menu."
                )
                .size(14),
                context_menu_text_input("Type here…", &self.value)
                    .on_input(Message::Changed)
                    .native_menu(&self.text_menu),
                selectable_text(
                    "This paragraph can be selected and copied through its \
                     right-click menu."
                )
                .size(16)
                .padding(12)
                .width(Fill)
                .native_menu(&self.select_menu),
                Space::new().height(Length::Fixed(8.0)),
                text(format!("Value: {}", self.value)).size(13),
            ]
            .spacing(16)
            .align_x(Alignment::Start),
        )
        .padding(24)
        .width(Length::Fixed(520.0))
        .style(container::rounded_box);

        container(card).center(Fill).padding(24).boxed()
    }
}

/// Captures the raw Wayland handles of the main window.
#[cfg(all(feature = "native", target_os = "linux"))]
fn capture_window_handle() -> Task<Message> {
    iced::window::latest().and_then(|id| {
        iced::window::run(id, |window| native::wayland_handle(&window))
            .map(Message::WindowHandle)
    })
}

//! Demonstrates the right-click context menu text input.
//!
//! Right-click inside the field to open the Cut / Copy / Paste / Select All
//! menu. Select some text first with the mouse or `Ctrl+A`.
//!
//! Run with:
//!
//! ```text
//! cargo run --example context_menu_text_input
//! ```

use iced::widget::{Space, column, container, row, text};
use iced::{Alignment, Element, Fill, Length, Task};
use iced_component::text_input::{
    ContextMenuTextInput, context_menu_text_input,
};

pub fn main() -> iced::Result {
    iced::application(Demo::default, Demo::update, Demo::view)
        .window_size((560.0, 360.0))
        .centered()
        .run()
}

#[derive(Default)]
struct Demo {
    content: String,
}

#[derive(Debug, Clone)]
enum Message {
    ContentChanged(String),
}

impl Demo {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ContentChanged(content) => self.content = content,
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let input: ContextMenuTextInput<'_, Message> =
            context_menu_text_input("Right-click me...", &self.content)
                .on_input(Message::ContentChanged)
                .padding(10);

        let value = if self.content.is_empty() {
            text("(empty)").size(14)
        } else {
            text!("{}", self.content).size(14)
        };

        let card = container(
            column![
                text("Context menu text input").size(24),
                text(
                    "Right-click the field below to open Cut, Copy, Paste and \
                     Select All. Everything else works like the built-in text \
                     input."
                )
                .size(14),
                input,
                row![text("Value:").size(14), Space::new().width(8.0), value]
                    .align_y(Alignment::Center),
            ]
            .spacing(16),
        )
        .padding(24)
        .width(Length::Fixed(440.0))
        .style(container::rounded_box);

        container(card).center(Fill).padding(24).into()
    }
}

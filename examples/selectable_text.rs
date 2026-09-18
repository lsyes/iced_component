//! Demonstrates selectable text with a right-click menu.
//!
//! Drag over the text to select it, double-click for a word, triple-click for a
//! line, then press `Ctrl`/`Cmd` + `C` or right-click and pick *Copy*.
//!
//! Run with:
//!
//! ```text
//! cargo run --example selectable_text
//! ```

use iced::widget::{column, container, text};
use iced::{Element, Fill, Task};
use iced_component::selectable_text::selectable_text;

pub fn main() -> iced::Result {
    iced::application(|| (), update, view)
        .window_size((640.0, 460.0))
        .centered()
        .run()
}

fn update(_state: &mut (), _message: ()) -> Task<()> {
    Task::none()
}

fn view(_state: &()) -> Element<'_, ()> {
    let card = container(
        column![
            text("Selectable text").size(24),
            text(
                "This paragraph is plain text, but you can select it with the \
                 mouse exactly like in a browser: click to place a cursor, \
                 drag to select a range, double-click for a word and \
                 triple-click for a line."
            )
            .size(15),
            selectable_text(
                "The quick brown fox jumps over the lazy dog. Pack my box with \
                 five dozen liquor jugs. How vexingly quick daft zebras jump!"
            )
            .size(16)
            .padding(12)
            .width(Fill)
            .wrapping(iced::widget::text::Wrapping::Word),
            text(
                "Press Ctrl/Cmd + C to copy the selection, Ctrl/Cmd + A to \
                 select everything, or right-click for the menu."
            )
            .size(13),
        ]
        .spacing(16),
    )
    .padding(24)
    .width(Fill)
    .style(container::rounded_box);

    container(card).center(Fill).padding(24).into()
}

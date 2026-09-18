//! Minimal smooth-scrolling demo with a large amount of text.
//!
//! It is deliberately tiny: a single [`smooth_scrollable`] call already gives a
//! complete widget, and the only extra calls here are the ones that make it fill
//! the window.
//!
//! Run with:
//!
//! ```text
//! cargo run --example smooth_scroll_text
//! ```

use iced::widget::{column, text};
use iced::{Element, Fill, Task};
use iced_component::smooth_scrollable::smooth_scrollable;

pub fn main() -> iced::Result {
    iced::application(|| (), update, view)
        .window_size((720.0, 640.0))
        .centered()
        .run()
}

fn update(_state: &mut (), _message: ()) -> Task<()> {
    Task::none()
}

fn view(_state: &()) -> Element<'_, ()> {
    // 生成 400 段文本，每段是一句话。
    let paragraphs = (0..400).map(|i| {
        text(format!(
            "[{i:03}] The quick brown fox jumps over the lazy dog. \
Pack my box with five dozen liquor jugs. \
How vexingly quick daft zebras jump!"
        ))
        .size(14)
        .into()
    });

    let content = column(paragraphs).spacing(10).padding(16);

    // 默认就是垂直 + Firefox 式平滑滚动，无需任何额外配置。
    smooth_scrollable(content)
        .id("text-scroll")
        .height(Fill)
        .width(Fill)
        .into()
}

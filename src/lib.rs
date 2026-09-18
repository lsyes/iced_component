//! Extra components for [`iced`].
//!
//! This crate currently provides:
//!
//! - [`smooth_scrollable::SmoothScrollable`]: a drop-in wrapper around
//!   [`iced::widget::scrollable`] that adds Firefox-style *smooth scrolling*
//!   through [`smooth_scrollable::ScrollMode`].
//! - [`text_input::ContextMenuTextInput`]: a text input with a right-click
//!   context menu (Cut / Copy / Paste / Select All).
//! - [`selectable_text::SelectableText`]: text that can be selected with the
//!   mouse or the keyboard and copied through the same context menu.
//! - [`context_menu`]: the reusable right-click menu that powers the two
//!   above—usable on its own with any other widget.
//!
//! All of them are built entirely on top of the public `iced` APIs, so they can
//! be dropped into any `iced` application without forking the framework.
//!
//! # Quick start
//!
//! One call is enough to get a fully working widget; every other call is an
//! optional part of a builder chain:
//!
//! ```no_run
//! use iced_component::smooth_scrollable;
//! use iced_widget::core::{Element, Length};
//! use iced_widget::{Renderer, Theme, text};
//!
//! // The default: vertical, Firefox-style smooth scrolling.
//! fn view<'a>() -> Element<'a, (), Theme, Renderer> {
//!     smooth_scrollable(text("Some very long content")).into()
//! }
//!
//! // Only add the calls you actually need.
//! fn customized<'a>() -> Element<'a, (), Theme, Renderer> {
//!     smooth_scrollable(text("Some very long content"))
//!         .id("my-list")
//!         .height(Length::Fill)
//!         .instant() // or keep the default `.smooth()`
//!         .into()
//! }
//! ```

pub mod context_menu;
pub mod selectable_text;
pub mod smooth_scrollable;
pub mod text_input;

pub use context_menu::{ContextMenu, Entry};
pub use selectable_text::{SelectableText, selectable_text};
pub use smooth_scrollable::{ScrollMode, SmoothScrollable, smooth_scrollable};
pub use text_input::{ContextMenuTextInput, context_menu_text_input};

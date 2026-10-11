//! Extra components for [`iced`](https://docs.rs/iced).
//!
//! This crate currently provides:
//!
//! - [`context_menu`]: a reusable right-click menu that can be attached to any
//!   other widget—usable on its own.
//! - [`text_input::ContextMenuTextInput`]: a text input with a right-click
//!   context menu (Cut / Copy / Paste / Select All).
//! - [`selectable_text::SelectableText`]: text that can be selected with the
//!   mouse or the keyboard and copied through the same context menu.
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
//! use iced_component::context_menu::{self, Entry};
//! use iced_widget::core::{Element, Length};
//! use iced_widget::{Renderer, Theme, text};
//!
//! fn view<'a>() -> Element<'a, (), Theme, Renderer> {
//!     context_menu::context_menu(text("Right-click me"))
//!         .entry(Entry::new("Copy", || ()))
//!         .into()
//! }
//! ```

pub mod context_menu;
pub mod selectable_text;
pub mod text_input;

pub use context_menu::{ContextMenu, Entry};
pub use selectable_text::{SelectableText, selectable_text};
pub use text_input::{ContextMenuTextInput, context_menu_text_input};

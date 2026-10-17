//! Extra components for [`iced`](https://docs.rs/iced).
//!
//! This crate currently provides:
//!
//! - [`context_menu`]: a reusable right-click menu that can be attached to any
//!   other widget—usable on its own. [`context_menu::app`] is an
//!   application-managed variant for menus whose entries are described by
//!   labels.
//! - [`text_input::ContextMenuTextInput`]: a text input with a right-click
//!   context menu (Cut / Copy / Paste / Select All).
//! - [`selectable_text::SelectableText`]: text that can be selected with the
//!   mouse or the keyboard and copied through the same context menu.
//! - [`context_menu::native`]: application-owned handles that show the text
//!   menus as native Wayland `xdg_popup`s.
//!
//! All of them are built on top of the public `iced` APIs. The optional native
//! popup adds one narrowly scoped `winit` patch; see the crate's `README` for
//! the `[patch]` section an application has to apply when enabling it.
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

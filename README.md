# iced-component

Extra components for [`iced`], built on top of its public APIs. The optional
native `xdg_popup` adds one narrowly scoped [`winit`] patch, described below.

1. **`context_menu::ContextMenu`** — a reusable right-click menu that can be
   attached to any widget, or driven directly by a custom one.
2. **`context_menu::app`** — an application-managed right-click menu, ported
   from [`iced_systemmonitor`]. Entries are plain labels and the chosen index is
   reported back, so one state can serve many widgets. Like the overlay menu,
   it lands underneath the pointer inside scrolled or translated content.
3. **`text_input::ContextMenuTextInput`** — a text input with a right-click
   context menu (Cut / Copy / Paste / Select All).
4. **`selectable_text::SelectableText`** — text that can be selected with the
   mouse or the keyboard and copied through the same menu.
5. **`context_menu::native::Menu`** — an application-owned handle that turns
   the context menus of the text input and the selectable text into native
   Wayland popups.

The overlay menu is a soft, rounded, white card that always floats above the
rest of the interface—including other overlays.

## Native Wayland popup

On Linux, enable the `native` feature to draw the context menus as real Wayland
`xdg_popup`s outside of the window. Without the feature—or when no Wayland
window handle is available—they fall back to the in-application panel.

`context_menu::app` uses `State::set_window_handle` directly. The text input
and the selectable text use an application-owned `context_menu::native::Menu`
(one per widget):

```rust
use iced_component::context_menu::native::Menu;

let mut menu = Menu::new();
menu.set_window_handle(handle);
menu.set_window_size(size);

let input = context_menu_text_input("Type…", &value)
    .native_menu(&menu);
```

Keep `menu.subscription()` in the application subscription and forward its
events to `menu.update(event)`. The widget applies the chosen entry on its next
update.

```text
cargo run --example app_context_menu --features native
cargo run --example native_text_menu --features native
```

The popup needs the window's `xdg_surface`, which upstream `winit` does not
expose. This repository vendors a patched `winit` and wires it with a
`[patch]` section. An application that enables the feature has to apply the
same patch to its own `Cargo.toml`:

```toml
[patch."https://github.com/iced-rs/winit.git"]
winit = { path = "path/to/iced_component/third_party/winit" }
dpi = { path = "path/to/iced_component/third_party/winit/dpi" }
```

Then capture the handles once with `app::wayland_handle(&window)` and pass them
to `State::set_window_handle`. The rest of the API is unchanged: keep
`State::subscription` in the application subscription and forward
`Event`s to `State::update(event, &theme)`.

[`iced`]: https://github.com/iced-rs/iced
[`iced_systemmonitor`]: https://github.com/lsyes/iced_systemmonitor
[`winit`]: https://github.com/iced-rs/winit

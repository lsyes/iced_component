# iced-component

Extra components for [`iced`], built entirely on top of its public APIs:

1. **`context_menu::ContextMenu`** — a reusable right-click menu that can be
   attached to any widget, or driven directly by a custom one.
2. **`text_input::ContextMenuTextInput`** — a text input with a right-click
   context menu (Cut / Copy / Paste / Select All).
3. **`selectable_text::SelectableText`** — text that can be selected with the
   mouse or the keyboard and copied through the same menu.

The menu is a soft, rounded, white card that always floats above the rest of the
interface—including other overlays.

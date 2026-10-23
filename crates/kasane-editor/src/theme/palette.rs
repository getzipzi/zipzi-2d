//! Original Kasane palettes and the VS Code Modern variants.
use crate::ui::compact::RADIUS;
use gpui_kit::component::Theme;
use gpui_kit::{Hsla, px, rgb};

pub(crate) struct ToolbarPalette {
    pub surface: Hsla,
    pub hovered: Hsla,
    pub pressed: Hsla,
}

pub(super) fn toolbar(choice: super::ColorTheme, theme: &Theme) -> ToolbarPalette {
    let dark = theme.is_dark();
    let (surface, hovered, pressed) = match (choice.is_modern(), dark) {
        (false, true) => (0x202020, 0x292929, 0x181818),
        (false, false) => (0xffffff, 0xf2f2f2, 0xdedede),
        (true, true) => (0x181818, 0x2b2b2b, 0x313131),
        (true, false) => (0xf8f8f8, 0xf2f2f2, 0xe8e8e8),
    };
    ToolbarPalette {
        surface: rgb(surface).into(),
        hovered: rgb(hovered).into(),
        pressed: rgb(pressed).into(),
    }
}

pub(super) fn apply_original(theme: &mut Theme) {
    let dark = theme.is_dark();
    let color = |dark_color, light_color| rgb(if dark { dark_color } else { light_color }).into();
    theme.background = color(0x292929, 0xf3f3f3);
    theme.foreground = color(0xd8d8d8, 0x292929);
    theme.muted_foreground = color(0xa6a6a6, 0x666666);
    theme.border = color(0x484848, 0xb8b8b8);
    theme.tab_bar = color(0x303030, 0xe6e6e6);
    theme.tab = theme.tab_bar;
    theme.tab_active = theme.background;
    theme.tab_foreground = theme.muted_foreground;
    theme.tab_active_foreground = theme.foreground;
    theme.popover = theme.background;
    theme.popover_foreground = theme.foreground;
    theme.muted = color(0x353535, 0xe7e7e7);
    theme.accent = color(0x424242, 0xdcdcdc);
    theme.accent_foreground = theme.foreground;
    theme.button = color(0x3a3a3a, 0xe9e9e9);
    theme.button_hover = color(0x474747, 0xdcdcdc);
    theme.button_active = color(0x525252, 0xcdcdcd);
    theme.button_foreground = theme.foreground;
    theme.input = theme.border;
    theme.radius = px(RADIUS);
    theme.shadow = false;
    let (primary, hover, active, foreground) = if dark {
        (0x82aaff, 0x96b8ff, 0x7198e6, 0xffffff)
    } else {
        (0x3568ad, 0x2f5e9e, 0x28518a, 0xffffff)
    };
    theme.primary = rgb(primary).into();
    theme.primary_hover = rgb(hover).into();
    theme.primary_active = rgb(active).into();
    theme.primary_foreground = rgb(foreground).into();
    theme.button_primary = theme.primary;
    theme.button_primary_hover = theme.primary_hover;
    theme.button_primary_active = theme.primary_active;
    theme.button_primary_foreground = theme.primary_foreground;
    theme.ring = theme.primary;
    theme.drag_border = theme.primary;
    theme.selection = theme.primary.opacity(0.25);
}

pub(super) fn apply_modern(theme: &mut Theme) {
    let dark = theme.is_dark();
    let color = |dark_color, light_color| rgb(if dark { dark_color } else { light_color }).into();
    // Surfaces follow VS Code Dark Modern / Light Modern's panel palette.
    // https://github.com/microsoft/vscode/tree/main/extensions/theme-defaults/themes
    theme.background = color(0x181818, 0xf8f8f8);
    theme.foreground = color(0xcccccc, 0x3b3b3b);
    theme.muted_foreground = color(0x9d9d9d, 0x616161);
    theme.border = color(0x2b2b2b, 0xe5e5e5);
    theme.tab_bar = color(0x181818, 0xf8f8f8);
    theme.tab = theme.tab_bar;
    theme.tab_active = color(0x1f1f1f, 0xf8f8f8);
    theme.tab_foreground = theme.muted_foreground;
    theme.tab_active_foreground = color(0xffffff, 0x3b3b3b);
    theme.popover = color(0x1f1f1f, 0xffffff);
    theme.popover_foreground = theme.foreground;
    theme.muted = color(0x1f1f1f, 0xf2f2f2);
    theme.accent = color(0x2b2b2b, 0xf2f2f2);
    theme.accent_foreground = theme.foreground;
    theme.button = color(0x181818, 0xf8f8f8);
    theme.button_hover = color(0x2b2b2b, 0xe8e8e8);
    theme.button_active = color(0x313131, 0xcccccc);
    theme.button_foreground = theme.foreground;
    theme.input = color(0x3c3c3c, 0xcecece);
    theme.radius = px(RADIUS);
    theme.shadow = false;
    let (primary, hover, active, foreground) = if dark {
        (0x0078d4, 0x026ec1, 0x005fa3, 0xffffff)
    } else {
        (0x005fb8, 0x0258a8, 0x004c94, 0xffffff)
    };
    theme.primary = rgb(primary).into();
    theme.primary_hover = rgb(hover).into();
    theme.primary_active = rgb(active).into();
    theme.primary_foreground = rgb(foreground).into();
    theme.button_primary = theme.primary;
    theme.button_primary_hover = theme.primary_hover;
    theme.button_primary_active = theme.primary_active;
    theme.button_primary_foreground = theme.primary_foreground;
    theme.ring = theme.primary;
    theme.drag_border = theme.primary;
    theme.selection = theme.primary.opacity(if dark { 0.25 } else { 0.16 });
    theme.colors.list = theme.background;
    theme.list_head = theme.tab_bar;
    theme.list_even = theme.background;
    theme.list_hover = theme.accent;
    theme.list_active = color(0x264778, 0xe8e8e8);
    theme.list_active_border = theme.primary;
    theme.table = theme.background;
    theme.table_head = theme.list_head;
    theme.table_head_foreground = theme.muted_foreground;
    theme.table_even = theme.list_even;
    theme.table_hover = theme.list_hover;
    theme.table_active = theme.list_active;
    theme.table_active_border = theme.list_active_border;
    theme.table_row_border = theme.border;
    theme.secondary = color(0x181818, 0xe5e5e5);
    theme.secondary_hover = color(0x2b2b2b, 0xcccccc);
    theme.secondary_active = color(0x313131, 0xbfbfbf);
    theme.secondary_foreground = theme.foreground;
    theme.button_secondary = theme.secondary;
    theme.button_secondary_hover = theme.secondary_hover;
    theme.button_secondary_active = theme.secondary_active;
    theme.button_secondary_foreground = theme.secondary_foreground;
    theme.scrollbar_thumb = color(0x6e6e6e, 0xbfbfbf).opacity(0.8);
    theme.scrollbar_thumb_hover = color(0x868686, 0xa3a3a3);
}

//! Editor accents: blue for interaction, type colors on icons, amber for unsaved work.
use editor_core::TreeNodeKind;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Hsla, rgb};

pub(crate) fn init(cx: &mut App) {
    change(ThemeMode::Dark, cx);
}

pub(crate) fn change(mode: ThemeMode, cx: &mut App) {
    Theme::change(mode, None, cx);
    // Update Kit's resolved tokens and Base projection together. Keep neutral
    // surfaces and the parameter/keyform status palette unchanged.
    Theme::update(cx, |theme| {
        let (primary, hover, active, foreground) = if theme.is_dark() {
            (0x82aaff, 0x96b8ff, 0x7198e6, 0x182334)
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
    });
}

/// Type identity stays visible even when the row is selected or hovered.
pub(crate) fn object_icon(kind: TreeNodeKind, theme: &Theme) -> Hsla {
    match kind {
        TreeNodeKind::Mesh => color(theme, 0x73b8b1, 0x327d76),
        TreeNodeKind::WarpDeformer => color(theme, 0xb3a0d6, 0x7958a6),
        TreeNodeKind::RotationDeformer => color(theme, 0xb5c76a, 0x667c20),
        TreeNodeKind::Group | TreeNodeKind::Part | TreeNodeKind::Offscreen => {
            theme.muted_foreground
        }
    }
}

pub(crate) fn modified(theme: &Theme) -> Hsla {
    color(theme, 0xd4af72, 0x95671e)
}

fn color(theme: &Theme, dark: u32, light: u32) -> Hsla {
    rgb(if theme.is_dark() { dark } else { light }).into()
}

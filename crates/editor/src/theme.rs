//! Editor accents: blue for interaction, type colors on icons, amber for unsaved work.
use editor_core::TreeNodeKind;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Hsla, rgb};

pub(crate) fn init(cx: &mut App) {
    change(ThemeMode::Dark, cx);
}

pub(crate) fn change(mode: ThemeMode, cx: &mut App) {
    Theme::change(mode, None, cx);
    // Update Kit's resolved tokens and Base projection together.
    let scale = cx
        .try_global::<crate::settings::Settings>()
        .map_or(1., |s| s.ui_scale.factor());
    Theme::update(cx, |theme| {
        crate::ui::compact::apply_theme(theme);
        theme.font_size = gpui_kit::px(16. * scale);
        theme.radius = gpui_kit::px(crate::ui::compact::RADIUS * scale);
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

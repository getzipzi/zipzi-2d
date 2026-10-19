//! Editor accents: blue for interaction, type colors on icons, amber for unsaved work.
use editor_core::TreeNodeKind;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Hsla, rgb};
use gpui_kit::{BorrowAppContext as _, Global, component::ActiveTheme};
use serde::{Deserialize, Serialize};

mod palette;
pub(crate) use palette::ToolbarPalette;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ColorTheme {
    Light,
    #[default]
    Dark,
    VsCodeLightModern,
    VsCodeDarkModern,
}

impl Global for ColorTheme {}

impl ColorTheme {
    pub const ALL: [Self; 4] = [
        Self::Light,
        Self::Dark,
        Self::VsCodeLightModern,
        Self::VsCodeDarkModern,
    ];

    pub fn label(self) -> std::borrow::Cow<'static, str> {
        match self {
            Self::Light => crate::t!("editor.theme_light"),
            Self::Dark => crate::t!("editor.theme_dark"),
            Self::VsCodeLightModern => "VS Code Light Modern".into(),
            Self::VsCodeDarkModern => "VS Code Dark Modern".into(),
        }
    }

    pub fn mode(self) -> ThemeMode {
        match self {
            Self::Light | Self::VsCodeLightModern => ThemeMode::Light,
            Self::Dark | Self::VsCodeDarkModern => ThemeMode::Dark,
        }
    }

    pub fn is_modern(self) -> bool {
        matches!(self, Self::VsCodeLightModern | Self::VsCodeDarkModern)
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
            Self::VsCodeLightModern => Self::VsCodeDarkModern,
            Self::VsCodeDarkModern => Self::VsCodeLightModern,
        }
    }
}

impl From<ThemeMode> for ColorTheme {
    fn from(mode: ThemeMode) -> Self {
        if mode.is_dark() {
            Self::Dark
        } else {
            Self::Light
        }
    }
}

pub(crate) fn current(cx: &App) -> ColorTheme {
    cx.try_global::<ColorTheme>().copied().unwrap_or_default()
}

pub(crate) fn set_preference(choice: ColorTheme, cx: &mut App) -> anyhow::Result<()> {
    cx.update_global::<crate::settings::Settings, _>(|settings, _| {
        settings.set_color_theme(choice)
    })?;
    change(choice, cx);
    Ok(())
}

pub(crate) fn init(cx: &mut App) {
    let choice = cx
        .try_global::<crate::settings::Settings>()
        .map(|settings| settings.color_theme)
        .unwrap_or_default();
    change(choice, cx);
}

pub(crate) fn change(choice: impl Into<ColorTheme>, cx: &mut App) {
    let choice = choice.into();
    cx.set_global(choice);
    Theme::change(choice.mode(), None, cx);
    // Update Kit's resolved tokens and Base projection together.
    let scale = cx
        .try_global::<crate::settings::Settings>()
        .map_or(1., |s| s.ui_scale.factor());
    Theme::update(cx, |theme| {
        if choice.is_modern() {
            palette::apply_modern(theme);
        } else {
            palette::apply_original(theme);
        }
        theme.font_size = gpui_kit::px(16. * scale);
        theme.radius = gpui_kit::px(crate::ui::compact::RADIUS * scale);
    });
}

pub(crate) fn workspace(cx: &App) -> Hsla {
    let value = match (current(cx).is_modern(), cx.theme().is_dark()) {
        (false, true) => 0x1f1f1f,
        (false, false) => 0xd9d9d9,
        (true, true) => 0x181818,
        (true, false) => 0xf8f8f8,
    };
    rgb(value).into()
}

pub(crate) fn toolbar_colors(cx: &App) -> ToolbarPalette {
    palette::toolbar(current(cx), cx.theme())
}

pub(crate) fn viewport_background(cx: &App) -> Hsla {
    match current(cx) {
        ColorTheme::VsCodeLightModern => rgb(0xebebeb).into(),
        ColorTheme::VsCodeDarkModern => rgb(0x2b2b2b).into(),
        ColorTheme::Light | ColorTheme::Dark => cx.theme().muted,
    }
}

pub(crate) fn transparency_colors(cx: &App) -> (Hsla, Hsla) {
    let (base, alternate) = if cx.theme().is_dark() {
        if current(cx).is_modern() {
            (0x1f1f1f, 0x2b2b2b)
        } else {
            (0x303030, 0x3c3c3c)
        }
    } else {
        (0xffffff, 0xe5e5e5)
    };
    (rgb(base).into(), rgb(alternate).into())
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

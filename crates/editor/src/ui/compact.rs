//! Editor density in logical pixels. Keep Kit's 16px rem baseline intact.
use gpui_kit::component::{
    ActiveTheme, Selectable, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    input::Input,
    menu::PopupMenu,
    select::{Select, SelectDelegate},
};
use gpui_kit::{prelude::*, *};

#[allow(dead_code)]
pub const BODY: f32 = 12.;
#[allow(dead_code)]
pub const CAPTION: f32 = 11.;
#[allow(dead_code)]
pub const CONTROL: f32 = 22.;
#[allow(dead_code)]
pub const HEADER_CONTROL: f32 = 20.;
#[allow(dead_code)]
pub const NUMBER_CONTROL: f32 = 18.;
#[allow(dead_code)]
pub const ROW: f32 = 22.;
#[allow(dead_code)]
pub const DOCK_HEADER: f32 = 26.;
#[allow(dead_code)]
pub const STATUS_BAR: f32 = 22.;
#[allow(dead_code)]
pub const RADIUS: f32 = 3.;
#[allow(dead_code)]
pub const PANEL_RADIUS: f32 = 5.;
#[allow(dead_code)]
pub const PANEL_GAP: f32 = 4.;
#[allow(dead_code)]
pub const PADDING: f32 = 6.;
#[allow(dead_code)]
pub const GAP: f32 = 2.;

#[allow(dead_code)]
pub fn rem(value: f32) -> Rems {
    gpui_kit::rems(value / 16.)
}

#[allow(dead_code)]
pub fn scale(cx: &App) -> f32 {
    cx.theme().font_size / px(16.)
}

#[allow(dead_code)]
pub fn scaled(value: f32, cx: &App) -> Pixels {
    px(value * scale(cx))
}

/// Match horizontal header padding to the centered controls' vertical space.
#[allow(dead_code)]
pub fn header_padding(cx: &App) -> Pixels {
    (scaled(DOCK_HEADER - HEADER_CONTROL, cx) - px(2.)) / 2.
}

/// Align chrome content with a panel's outer inset, outline and header padding.
#[allow(dead_code)]
pub fn chrome_inset(cx: &App) -> Pixels {
    scaled(PANEL_GAP, cx) + px(1.) + header_padding(cx)
}

pub fn workspace(cx: &App) -> Hsla {
    rgb(if cx.theme().is_dark() {
        0x1f1f1f
    } else {
        0xd9d9d9
    })
    .into()
}

pub fn apply_theme(theme: &mut Theme) {
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
}

/// Preserve Kit's focus, disabled, pressed and selected behavior.
pub fn button(button: Button) -> Button {
    let selected = button.is_selected();
    button
        .with_variant(ButtonVariant::Default)
        .xsmall()
        .h(rem(CONTROL))
        .min_w(rem(CONTROL))
        .rounded(gpui_kit::component::button::ButtonRounded::Medium)
        .when(selected, |button| button.primary().outline())
}

/// Kit keeps toggle accessibility and selected styling as separate states.
pub fn toggle(button: Button, selected: bool) -> Button {
    self::button(button.selected(selected).toggled(selected))
}

pub fn header_button(button: Button) -> Button {
    self::button(button)
        .h(rem(HEADER_CONTROL))
        .min_w(rem(HEADER_CONTROL))
}

/// Keep menu interaction in Kit, including submenus and custom multiline rows.
#[allow(dead_code)]
pub fn menu(menu: PopupMenu) -> PopupMenu {
    menu.small()
}

/// XSmall supplies 12px text to the trigger, options and search input.
#[allow(dead_code)]
pub fn select<D: SelectDelegate + 'static>(select: Select<D>) -> Select<D> {
    select.xsmall().h(rem(CONTROL))
}

/// Stop a control's press after composed popovers/menus have handled it, so
/// the dock header cannot arm a panel drag. Do not put this on popup triggers:
/// their parent popover also needs the mouse-down event to open.
#[allow(dead_code)]
pub fn header_control(control: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(control)
}

/// Joined controls keep their own action/toggle semantics and keyboard handlers.
#[allow(dead_code)]
pub fn group(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    axis: Axis,
    buttons: Vec<Button>,
) -> gpui_kit::component::toolbar::ToolbarGroup {
    let last = buttons.len().saturating_sub(1);
    let selected: Vec<_> = buttons.iter().map(Button::is_selected).collect();
    let mut group = gpui_kit::component::toolbar::ToolbarGroup::new(id)
        .label(label)
        .gap(px(0.))
        .when(axis == Axis::Vertical, |g| g.flex_col());
    for (index, button) in buttons.into_iter().enumerate() {
        // A selected button owns both adjoining seams. Otherwise the preceding
        // button owns the shared edge, keeping a single line with no spacing.
        let leading = index == 0 || selected[index];
        let trailing = !selected.get(index + 1).copied().unwrap_or(false);
        let button = button
            .when(axis == Axis::Horizontal, |b| {
                b.border_l(px(if leading { 1. } else { 0. }))
                    .border_r(px(if trailing { 1. } else { 0. }))
            })
            .when(index > 0 && axis == Axis::Horizontal, |b| {
                b.rounded_tl(px(0.)).rounded_bl(px(0.))
            })
            .when(index < last && axis == Axis::Horizontal, |b| {
                b.rounded_tr(px(0.)).rounded_br(px(0.))
            })
            .when(axis == Axis::Vertical, |b| {
                b.border_t(px(if leading { 1. } else { 0. }))
                    .border_b(px(if trailing { 1. } else { 0. }))
            })
            .when(index > 0 && axis == Axis::Vertical, |b| {
                b.rounded_tl(px(0.)).rounded_tr(px(0.))
            })
            .when(index < last && axis == Axis::Vertical, |b| {
                b.rounded_bl(px(0.)).rounded_br(px(0.))
            });
        group = group.content(button);
    }
    group
}

pub fn input(input: Input) -> Input {
    input
        .small()
        // Input::h is multiline-only; Styled::h also sizes single-line inputs.
        .map(|input| Styled::h(input, rem(CONTROL)))
        .text_size(rem(BODY))
        .px(rem(4.))
        .py(rem(0.))
        .rounded(rem(RADIUS))
}

#[allow(dead_code)]
pub fn header_input(input: Input) -> Input {
    self::input(input).map(|input| Styled::h(input, rem(HEADER_CONTROL)))
}

/// Long identifiers stay within the value column; the tooltip retains the full value.
pub fn property_value(value: impl Into<SharedString>) -> impl IntoElement {
    let value = value.into();
    let tooltip = value.clone();
    let numeric = value.parse::<f64>().is_ok();
    div()
        .id("property-value")
        .test_support()
        .w_full()
        .min_w_0()
        .truncate()
        .when(numeric, |value| {
            value
                .text_align(TextAlign::Right)
                .font_family(".AppleSystemUIFontMonospaced")
        })
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .child(value)
}

/// Read-only values stay plain; editable controls can use the same aligned row.
/// Wrapping moves the value below its label when the panel becomes narrow.
pub fn property_row(label: impl Into<SharedString>, value: impl IntoElement, cx: &App) -> Div {
    let label = label.into();
    let tooltip = label.clone();
    div()
        .flex()
        .flex_wrap()
        .items_start()
        .min_w_0()
        .min_h(rem(ROW))
        .gap(rem(GAP))
        .text_size(rem(BODY))
        .line_height(rem(16.))
        .child(
            div()
                .id(SharedString::from(format!("property-label-{label}")))
                .test_support()
                .w(rem(132.))
                .min_w_0()
                .py(rem(3.))
                .truncate()
                .text_color(cx.theme().muted_foreground)
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .child(label),
        )
        .child(
            div()
                .flex_grow(1.)
                .flex_basis(rem(70.))
                .min_w_0()
                .min_h(rem(CONTROL))
                .flex()
                .items_center()
                .child(value),
        )
}

use super::setup;
use crate::{
    settings::{Settings, UiScale},
    theme::{self, ColorTheme},
};
use gpui_kit::{AppContext as _, TestAppContext, component::ActiveTheme, rgb, test::TestWindowExt};

#[gpui_kit::test]
fn theme_menu_selects_original_and_modern_palettes_without_leaking_colors(cx: &mut TestAppContext) {
    let harness = setup(cx);
    for (index, choice, background, primary) in [
        (2, ColorTheme::VsCodeLightModern, 0xf8f8f8, 0x005fb8),
        (0, ColorTheme::Light, 0xf3f3f3, 0x3568ad),
        (3, ColorTheme::VsCodeDarkModern, 0x181818, 0x0078d4),
        (1, ColorTheme::Dark, 0x292929, 0x82aaff),
    ] {
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            window.click("workspace-layout-menu", cx);
            window.within("popup-menu").hover(3usize, cx);
            window
                .within("submenu")
                .within("popup-menu")
                .click(index as usize, cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(theme::current(cx), choice);
            assert_eq!(cx.global::<Settings>().color_theme, choice);
            assert_eq!(cx.theme().background, rgb(background).into());
            assert_eq!(cx.theme().primary, rgb(primary).into());
            let restored = Settings::from_store(
                cx.global::<Settings>().store.clone(),
                cx.keyboard_mapper().as_ref(),
            );
            assert_eq!(restored.color_theme, choice);
            assert!(window.try_find("popup-menu").is_none());
            assert_eq!(
                harness
                    .view
                    .read(cx)
                    .editor
                    .document()
                    .unwrap()
                    .history_lengths(),
                (0, 0)
            );
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn scaling_and_native_toggle_preserve_theme_family_and_saved_appearance(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.dispatch_action(
            Box::new(crate::actions::SetColorTheme {
                theme: ColorTheme::VsCodeDarkModern,
            }),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("workspace-layout-menu", cx);
        window.within("popup-menu").hover(4usize, cx);
        window
            .within("submenu")
            .within("popup-menu")
            .click(1usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        assert_eq!(theme::current(cx), ColorTheme::VsCodeDarkModern);
        assert_eq!(cx.global::<Settings>().ui_scale, UiScale::Large);
        window.dispatch_action(Box::new(crate::actions::ToggleTheme), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(theme::current(cx), ColorTheme::VsCodeLightModern);
        let restored = Settings::from_store(
            cx.global::<Settings>().store.clone(),
            cx.keyboard_mapper().as_ref(),
        );
        assert_eq!(restored.color_theme, ColorTheme::VsCodeLightModern);
        assert_eq!(restored.ui_scale, UiScale::Large);
        cx.set_global(restored);
        theme::change(ColorTheme::Dark, cx);
        theme::init(cx);
        assert_eq!(theme::current(cx), ColorTheme::VsCodeLightModern);
        assert_eq!(cx.theme().font_size, gpui_kit::px(17.6));
    });
}

#[gpui_kit::test]
fn failed_theme_save_leaves_current_palette_and_preference_unchanged(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, _, cx| {
        let path = cx.global::<Settings>().store.root().join("appearance.json");
        std::fs::create_dir(path).unwrap();
        assert!(theme::set_preference(ColorTheme::VsCodeLightModern, cx).is_err());
        assert_eq!(theme::current(cx), ColorTheme::Dark);
        assert_eq!(cx.global::<Settings>().color_theme, ColorTheme::Dark);
        assert_eq!(cx.theme().background, rgb(0x292929).into());
    })
    .unwrap();
}

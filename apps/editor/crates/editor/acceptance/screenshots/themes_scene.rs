use super::{Harness, common::save};
use crate::{
    settings::Settings,
    theme::{self, ColorTheme},
};
use gpui_kit::{component::ActiveTheme, rgb, test::TestWindowExt};

pub(super) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    let mut h = Harness::new()?;
    h.capture()?;
    for (index, choice, name, background) in [
        (0usize, ColorTheme::Light, "light", 0xf3f3f3),
        (1, ColorTheme::Dark, "dark", 0x292929),
        (
            2,
            ColorTheme::VsCodeLightModern,
            "vscode-light-modern",
            0xf8f8f8,
        ),
        (
            3,
            ColorTheme::VsCodeDarkModern,
            "vscode-dark-modern",
            0x181818,
        ),
    ] {
        h.cx.update_window(h.window, |_, window, cx| {
            window.click("workspace-layout-menu", cx);
            window.within("popup-menu").hover(3usize, cx);
        })?;
        let menu = h.capture()?;
        save(&menu, directory, &format!("color-theme-menu-{name}.png"))?;
        h.cx.update_window(h.window, |_, window, cx| {
            window
                .within("submenu")
                .within("popup-menu")
                .click(index, cx);
        })?;
        let screenshot = h.capture()?;
        h.cx.update_window(h.window, |_, _, cx| {
            assert_eq!(theme::current(cx), choice);
            assert_eq!(cx.global::<Settings>().color_theme, choice);
            assert_eq!(cx.theme().background, rgb(background).into());
        })?;
        save(&screenshot, directory, &format!("color-theme-{name}.png"))?;
    }
    h.cx.update_window(h.window, |_, window, cx| {
        window.click("workspace-layout-menu", cx);
        window.within("popup-menu").hover(3usize, cx);
    })?;
    save(&h.capture()?, directory, "color-theme-menu.png")?;
    Ok(())
}

use gpui_kit::test::TestWindowExt;
use gpui_kit::{point, px};
use image::RgbaImage;
use std::sync::atomic::{AtomicBool, Ordering};

static FIRST_SAVED: AtomicBool = AtomicBool::new(false);

pub(crate) fn save(
    image: &RgbaImage,
    directory: &std::path::Path,
    name: &str,
) -> anyhow::Result<()> {
    let should_save = std::env::var_os("KASANE_SCREENSHOT_DIR").is_some()
        || std::env::var_os("KASANE_SCREENSHOT_SAVE").is_some()
        || !FIRST_SAVED.swap(true, Ordering::Relaxed);

    if should_save {
        let path = directory.join(name);
        image.save_with_format(&path, image::ImageFormat::Png)?;
        // Verify the actual exported artifact round-trips without loss.
        anyhow::ensure!(
            image::open(&path)?.into_rgba8() == *image,
            "PNG round trip changed pixels"
        );
        println!("Screenshot: {}", path.display());
    }
    Ok(())
}

pub(crate) fn click_expression(
    window: &mut gpui_kit::Window,
    index: usize,
    cx: &mut gpui_kit::App,
) {
    window.render_frame(cx);
    let bounds = window.find(("animation-expression", index)).bounds();
    let viewport = window.find("animation-expression-list").bounds();
    window.scroll(
        "animation-expression-list",
        gpui_kit::ScrollDelta::Pixels(point(px(0.), viewport.center().y - bounds.center().y)),
        cx,
    );
    window.click(("animation-expression", index), cx);
}

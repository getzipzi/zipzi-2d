//! A main-thread native resource smoke test. Pointer ownership, visual quality and
//! OS accessibility preferences still require the interactive acceptance matrix.
#[path = "support/cursor_artwork.rs"]
mod cursor_artwork;
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn run() -> anyhow::Result<()> {
    use gpui_kit::{CursorStyle, CustomCursor, size};
    let platform = gpui_kit::platform::current_platform(true);
    for (name, svg) in cursor_artwork::ARTWORK {
        let artwork = CustomCursor::from_svg(
            svg,
            size(24, 24),
            cursor_artwork::hotspot(name),
            CursorStyle::Crosshair,
        )?;
        let expected = CursorStyle::Custom(artwork.id());
        anyhow::ensure!(
            platform.register_custom_cursor(artwork.clone()) == expected,
            "{name}: native registration fell back to the standard cursor"
        );
        anyhow::ensure!(
            platform.register_custom_cursor(artwork) == expected,
            "{name}: duplicate registration changed the cursor ID"
        );
    }
    println!(
        "native cursor registration and reuse passed on {}",
        std::env::consts::OS
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| run())
}

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    run()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn main() {
    eprintln!("SKIP: native cursors require macOS or Windows");
}

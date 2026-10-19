//! Pixel sensitivity checks for the real editor, including its asynchronous Metal preview.
#![cfg(target_os = "macos")]

use std::path::PathBuf;

use super::harness::Harness;

mod animation_scene;
mod common;
mod dock_and_layout;
mod mesh_generation_scene;
mod modeling_scene;
mod properties_scene;
mod themes_scene;
mod timeline_scene;
mod transfer_scene;
mod viewports_scene;

pub(super) fn run() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var_os("KASANE_DEV_SESSION_DIR").is_none(),
        "Unset KASANE_DEV_SESSION_DIR to keep screenshots isolated from development sessions"
    );
    let directory = std::env::var_os("KASANE_SCREENSHOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/headless-screenshots")
        });
    std::fs::create_dir_all(&directory)?;
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("themes") {
        themes_scene::run(&directory)?;
        println!("Color theme selection screenshots passed.");
        return Ok(());
    }
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("transfer") {
        transfer_scene::run(&directory)?;
        println!("Export settings screenshots passed.");
        return Ok(());
    }
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("properties") {
        properties_scene::run(&directory)?;
        println!("Properties form screenshots passed.");
        return Ok(());
    }
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("compact-popups") {
        dock_and_layout::compact_popup_screenshots(&directory)?;
        println!("Compact popup density and scale screenshots passed.");
        return Ok(());
    }
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("titlebar") {
        dock_and_layout::titlebar_screenshots(&directory)?;
        println!("Title bar project and empty-state screenshots passed.");
        return Ok(());
    }
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("compact") {
        dock_and_layout::compact_screenshots(&directory)?;
        println!("Compact density, scale and Metal corner screenshots passed.");
        return Ok(());
    }
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("dock") {
        dock_and_layout::dock_screenshots(&directory)?;
        println!("Dock Metal screenshots passed.");
        return Ok(());
    }
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("timeline") {
        timeline_scene::timeline_screenshots(&directory)?;
        println!("Timeline Metal screenshots passed.");
        return Ok(());
    }

    viewports_scene::viewport_screenshots(&directory)?;
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("viewports") {
        println!("Dual viewport Metal screenshots passed.");
        return Ok(());
    }
    modeling_scene::empty_titlebar_screenshots(&directory)?;
    animation_scene::animation_scrubbing_presents_frames_before_release()?;
    let mut harness = Harness::new()?;
    modeling_scene::modeling_and_parameters_screenshots(&mut harness, &directory)?;
    mesh_generation_scene::mesh_generation_screenshots(&mut harness, &directory)?;
    animation_scene::animation_playback_screenshots(&mut harness, &directory)?;
    animation_scene::animation_replacement_keeps_canvas(&mut harness)?;
    animation_scene::animation_panel_review_screenshots(&mut harness, &directory)?;
    println!("Headless editor screenshots passed (real Metal, no native window).");
    Ok(())
}

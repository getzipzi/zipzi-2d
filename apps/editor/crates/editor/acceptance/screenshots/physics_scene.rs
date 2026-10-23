use super::super::{fixture, harness::Harness};
use super::common::save;
use editor_core::{CommandId, Editor, PanelKind};
use gpui_kit::test::TestWindowExt;

pub(crate) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    for locale in ["en", "zh-CN"] {
        for compact in [false, true] {
            let mut harness = if compact {
                Harness::with_size(960., 900.)?
            } else {
                Harness::with_size(1160., 1080.)?
            };
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../../models/local/mao/runtime/mao_pro.model3.json");
            let document = if path.is_file() {
                editor_core::Document::import_model3(&path)?.0
            } else {
                fixture::combined_animation_document()?.0
            };
            crate::i18n::activate(Some(locale));
            harness.cx.update_window(harness.window, |_, window, cx| {
                harness.view.update(cx, |view, cx| {
                    view.editor = Editor::from_document(document);
                    view.refresh_document(window, cx);
                    view.run_command(CommandId::AnimationWorkspace, window, cx);
                    view.run_command(CommandId::ResetLayout, window, cx);
                    let id = view.editor.active_id().unwrap();
                    view.editor.set_physics_manual(id, true).unwrap();
                    let asset = view
                        .editor
                        .document()
                        .unwrap()
                        .physics_definition()
                        .unwrap();
                    if let Some(id) = asset
                        .data
                        .settings
                        .first()
                        .and_then(|r| r.inputs.first())
                        .and_then(|input| input.source.parameter_id())
                        .map(str::to_owned)
                    {
                        view.editor
                            .set_physics_input(view.editor.active_id().unwrap(), &id, 10.)
                            .unwrap();
                    }
                    view.editor.advance_animation_clock(0.5);
                    view.editor.pause_animation();
                    view.dock
                        .as_ref()
                        .unwrap()
                        .show(PanelKind::AnimationPhysics, None, window, cx);
                    view.redraw(cx);
                });
                window.render_frame(cx);
                window.click(("physics-group-row", 0usize), cx);
                window.render_frame(cx);
            })?;
            if compact {
                save(
                    &harness.capture()?,
                    directory,
                    &format!("physics-compact-{locale}.png"),
                )?;
                continue;
            }
            save(
                &harness.capture()?,
                directory,
                &format!("physics-panel-{locale}.png"),
            )?;
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.click("physics-fold-physics-inputs", cx);
                window.render_frame(cx);
            })?;
            save(
                &harness.capture()?,
                directory,
                &format!("physics-pendulum-{locale}.png"),
            )?;
        }
    }
    crate::i18n::activate(Some("en"));
    Ok(())
}

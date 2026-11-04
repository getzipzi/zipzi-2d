//! Native review of inline density and shared object frames.
use super::super::harness::Harness;
use super::{common::save, properties_scene::show, warp_authoring_scene::fixture_with_density};
use gpui_kit::{point, px, test::TestWindowExt};
use zipzi_editor_core::{Editor, EditorCommand, ObjectKind, ObjectRef, ViewportPoint};
pub(super) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    for locale in ["en", "zh-CN", "ja"] {
        let mut h = Harness::new()?;
        crate::i18n::activate(Some(locale));
        let (document, _assets, object) =
            fixture_with_density(zipzi_core::WarpBezierEditMode::Local, 5, 2)?;
        h.cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                view.replace_editor(Editor::from_document(document), cx);
                view.editor
                    .execute(EditorCommand::SelectObject(object))
                    .unwrap();
                view.editor.reset_view();
                view.editor.pan_by(ViewportPoint { x: -128., y: 128. });
                view.refresh_document(window, cx);
            });
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("warp-basic-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            show(window, "property-input-bezier_rows", cx);
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("warp-inline-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            for id in [
                "property-warp-transform",
                "property-warp-frame",
                "deformer-frame",
                "deformer-whole",
                "deformer-keep-children",
                "deformer-warp-bezier",
                "deformer-warp-fine",
                "warp-grid-interpolation",
            ] {
                assert!(window.try_find(id).is_none());
            }
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("warp-combined-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            window.activate_window();
            let tangent = h.view.read_with(cx, |view, _| {
                view.editor
                    .overlay()
                    .handles
                    .into_iter()
                    .find(|h| h.tangent)
                    .unwrap()
                    .position
            });
            let start =
                window.find("canvas-body").bounds().origin + point(px(tangent.x), px(tangent.y));
            window.drag(start, start + point(px(15.), px(-24.)), cx);
        })?;
        h.view.read_with(&h.cx, |view, _| {
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
            let overlay = view.editor.overlay();
            assert!(!overlay.warp_frame_lines.is_empty());
            assert_eq!(overlay.handles.iter().filter(|h| !h.tangent).count(), 9);
        });
        save(
            &h.capture()?,
            directory,
            &format!("warp-combined-edited-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            let grid = h.view.read_with(cx, |view, _| {
                view.editor
                    .overlay()
                    .vertices
                    .into_iter()
                    .find(|v| {
                        v.kind == zipzi_editor_core::OverlayVertexKind::WarpGrid && v.id == 14
                    })
                    .unwrap()
                    .position
            });
            window.click_at("canvas-body", point(px(grid.x), px(grid.y)), cx);
        })?;
        h.view.read_with(&h.cx, |view, _| {
            let overlay = view.editor.overlay();
            assert!(!overlay.warp_frame_lines.is_empty());
            assert_eq!(overlay.handles.iter().filter(|h| !h.tangent).count(), 9);
            let selected: Vec<_> = overlay.vertices.iter().filter(|v| v.selected).collect();
            assert_eq!(selected.len(), 1);
            assert_eq!(
                selected[0].kind,
                zipzi_editor_core::OverlayVertexKind::WarpGrid
            );
        });
        save(
            &h.capture()?,
            directory,
            &format!("warp-single-grid-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                view.editor
                    .execute(EditorCommand::SelectObject(ObjectRef::new(
                        ObjectKind::Mesh,
                        uuid::Uuid::from_u128(3).to_string(),
                    )))
                    .unwrap();
                view.refresh_document(window, cx);
            });
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("mesh-frame-{locale}.png"),
        )?;
        h.view.read_with(&h.cx, |view, _| {
            assert_eq!(view.editor.overlay().handles.len(), 9)
        });
    }
    Ok(())
}

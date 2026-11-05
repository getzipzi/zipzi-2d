//! Moderately sized Rotation controls through production native pointer events.
use super::super::harness::Harness;
use super::{common::save, warp_authoring_scene::fixture};
use gpui_kit::{
    InputEvent as _, Modifiers, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, point, px, test::TestWindowExt,
};
use zipzi_editor_core::{Editor, EditorCommand, ObjectRef, SceneObjectType, ViewportPoint};

fn gizmo(h: &Harness, object: &ObjectRef) -> zipzi_editor_core::DeformerGizmo {
    h.view.read_with(&h.cx, |view, _| {
        let frame = &view.modeling_frame().unwrap().canvas;
        let control = frame
            .geometry
            .controls
            .iter()
            .find(|c| c.object == *object)
            .unwrap();
        frame
            .geometry
            .deformer_gizmo(control, frame.transform)
            .unwrap()
    })
}

fn pointer(h: &mut Harness, p: ViewportPoint, phase: &str) -> anyhow::Result<()> {
    pointer_with_modifiers(h, p, phase, Modifiers::default())
}

fn pointer_with_modifiers(
    h: &mut Harness,
    p: ViewportPoint,
    phase: &str,
    modifiers: Modifiers,
) -> anyhow::Result<()> {
    h.cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let position = window.find("canvas-body").bounds().origin + point(px(p.x), px(p.y));
        let event = match phase {
            "down" => MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers,
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            "up" => MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers,
                click_count: 1,
            }
            .to_platform_input(),
            _ => MouseMoveEvent {
                position,
                modifiers,
                pressed_button: (phase == "drag").then_some(MouseButton::Left),
            }
            .to_platform_input(),
        };
        window.dispatch_event(event, cx);
        window.render_frame(cx);
    })?;
    Ok(())
}

pub(super) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    for locale in ["en", "zh-CN", "ja"] {
        let (mut document, _assets, warp) = fixture(zipzi_core::WarpBezierEditMode::Local)?;
        let plan = document.plan_scene_object_creation(
            SceneObjectType::Rotation,
            "Rotation".into(),
            &[warp],
        )?;
        let object = document
            .execute_scene_object_creation(&plan)?
            .suggested_selection[0]
            .clone();
        let mut h = Harness::new()?;
        crate::i18n::activate(Some(locale));
        h.cx.update_window(h.window, |_, window, cx| {
            window.activate_window();
            h.view.update(cx, |view, cx| {
                view.replace_editor(Editor::from_document(document), cx);
                view.editor
                    .execute(EditorCommand::SelectObject(object.clone()))
                    .unwrap();
                view.editor.reset_view();
                view.editor.pan_by(ViewportPoint { x: -128., y: 128. });
                view.refresh_document(window, cx);
            });
        })?;
        h.capture()?;
        h.cx.update_window(h.window, |_, _, cx| {
            h.view.update(cx, |view, cx| {
                view.editor.set_zoom_at(1., None);
                view.redraw(cx);
            });
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("rotation-content-{locale}.png"),
        )?;
        let initial = gizmo(&h, &object);
        let lines = h
            .view
            .read_with(&h.cx, |view, _| view.editor.overlay().rotation_lines);
        let start = ViewportPoint {
            x: (initial.center.x + initial.handles[0].1.x) * 0.5,
            y: (initial.center.y + initial.handles[0].1.y) * 0.5,
        };
        h.cx.update_window(h.window, |_, _, cx| {
            h.view.update(cx, |view, cx| {
                view.editor
                    .execute(EditorCommand::SelectObjects(Vec::new()))
                    .unwrap();
                view.redraw(cx);
            });
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("rotation-unselected-{locale}.png"),
        )?;
        h.view.read_with(&h.cx, |view, _| {
            let overlay = view.editor.overlay();
            assert_eq!(overlay.rotation_bones.len(), 1);
            assert!(!overlay.rotation_bones[0].selected);
            assert!(
                view.editor
                    .pick_candidates(start, Some(view.modeling_frame().unwrap().canvas.version))
                    .iter()
                    .any(|c| c.object == object)
            );
        });
        pointer(&mut h, start, "down")?;
        pointer(&mut h, start, "up")?;
        h.capture()?;
        h.view.read_with(&h.cx, |view, _| {
            assert_eq!(
                view.editor.document().unwrap().context().selections,
                [object.clone()]
            );
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
        });
        pointer(&mut h, start, "hover")?;
        h.capture()?;
        h.view.read_with(&h.cx, |view, _| {
            let overlay = view.editor.overlay();
            assert_eq!(
                overlay.active_handle,
                Some(zipzi_editor_core::DeformerHandle::Rotate)
            );
            assert_eq!(overlay.rotation_lines, lines);
            assert!(overlay.deformers.iter().all(|d| !d.hovered));
        });
        pointer(&mut h, start, "down")?;
        let (sin, cos) = 23.4f32.to_radians().sin_cos();
        let (dx, dy) = (start.x - initial.center.x, start.y - initial.center.y);
        let end = ViewportPoint {
            x: initial.center.x + dx * cos - dy * sin,
            y: initial.center.y + dx * sin + dy * cos,
        };
        pointer(&mut h, end, "drag")?;
        save(
            &h.capture()?,
            directory,
            &format!("rotation-drag-{locale}.png"),
        )?;
        h.view.read_with(&h.cx, |view, _| {
            let overlay = view.editor.overlay();
            assert_eq!(overlay.rotation_lines.len(), lines.len());
            assert!(overlay.guide_lines.is_empty());
            assert!(overlay.gizmo_lines.is_empty());
            assert!(
                overlay
                    .interaction_label
                    .as_ref()
                    .unwrap()
                    .contains("23.4°")
            );
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        });
        for (shift, alt, expected) in [
            (true, false, -30.),
            (true, true, -23.),
            (false, false, -23.4),
        ] {
            h.cx.update_window(h.window, |_, window, cx| {
                window.dispatch_event(
                    ModifiersChangedEvent {
                        modifiers: Modifiers {
                            shift,
                            alt,
                            ..Default::default()
                        },
                        capslock: Default::default(),
                    }
                    .to_platform_input(),
                    cx,
                );
                window.render_frame(cx);
            })?;
            h.capture()?;
            h.view.read_with(&h.cx, |view, _| {
                let angle = view
                    .editor
                    .document()
                    .unwrap()
                    .object_index()
                    .get(&object)
                    .unwrap()
                    .properties["rotation"]["pose"]["angle"]
                    .as_f64()
                    .unwrap();
                assert!(
                    (angle - expected).abs() < 0.002,
                    "shift={shift} alt={alt}: {angle}"
                );
                assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
            });
        }
        pointer(&mut h, end, "up")?;
        h.capture()?;
        let before_scale = gizmo(&h, &object);
        let start = before_scale.handles[0].1;
        let center = before_scale.center;
        let (dx, dy) = (start.x - center.x, start.y - center.y);
        let distance = dx.hypot(dy);
        let desired = distance * 1.5;
        let end = ViewportPoint {
            x: center.x + dx / distance * desired,
            y: center.y + dy / distance * desired,
        };
        pointer(&mut h, start, "down")?;
        pointer(&mut h, end, "drag")?;
        pointer(&mut h, end, "up")?;
        save(
            &h.capture()?,
            directory,
            &format!("rotation-scaled-{locale}.png"),
        )?;
        let scaled = gizmo(&h, &object);
        let radius = |g: &zipzi_editor_core::DeformerGizmo| {
            (g.handles[0].1.x - g.center.x).hypot(g.handles[0].1.y - g.center.y)
        };
        let authored_length = h.view.read_with(&h.cx, |view, _| {
            let key = zipzi_editor_core::property::deformer::BONE_LENGTH.key;
            view.editor
                .document()
                .unwrap()
                .property_values(&object)
                .unwrap()[&key]
                .text()
                .parse::<f32>()
                .unwrap()
        });
        assert!((radius(&initial) - authored_length).abs() < 0.01);
        assert!((radius(&scaled) - radius(&initial) * 1.5).abs() < 0.01);
        h.view.read_with(&h.cx, |view, _| {
            let index = view.editor.document().unwrap().object_index();
            let scale = index.get(&object).unwrap().properties["rotation"]["pose"]["scale"]
                .as_f64()
                .unwrap();
            assert!((scale - 1.5).abs() < 0.001);
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (3, 0));
        });
        let pose = h.view.read_with(&h.cx, |view, _| {
            view.editor
                .document()
                .unwrap()
                .object_index()
                .get(&object)
                .unwrap()
                .properties
                .clone()
        });
        let children = h.view.read_with(&h.cx, |view, _| {
            view.modeling_frame()
                .unwrap()
                .canvas
                .geometry
                .meshes
                .iter()
                .map(|m| m.vertices.clone())
                .collect::<Vec<_>>()
        });
        let start = scaled.handles[0].1;
        let end = ViewportPoint {
            x: scaled.center.x + (start.x - scaled.center.x) * 1.25,
            y: scaled.center.y + (start.y - scaled.center.y) * 1.25,
        };
        pointer(&mut h, start, "hover")?;
        h.cx.update_window(h.window, |_, window, cx| {
            window.dispatch_event(
                ModifiersChangedEvent {
                    modifiers: Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                    capslock: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
        })?;
        h.capture()?;
        h.view.read_with(&h.cx, |view, _| {
            let overlay = view.editor.overlay();
            assert!(overlay.rotation_length_adjusting);
            assert!(!overlay.handles[0].selected);
            assert!(view.editor.canvas_idle_hints().contains(
                &zipzi_editor_core::InteractionHint::Alt(zipzi_editor_core::HintAction::BoneLength)
            ));
        });
        let alt = Modifiers {
            alt: true,
            ..Default::default()
        };
        pointer_with_modifiers(&mut h, start, "down", alt)?;
        pointer_with_modifiers(&mut h, end, "drag", alt)?;
        save(
            &h.capture()?,
            directory,
            &format!("rotation-length-{locale}.png"),
        )?;
        // Native mouse-up reports Alt released; the captured operation stays length-only.
        pointer(&mut h, end, "up")?;
        h.capture()?;
        let lengthened = gizmo(&h, &object);
        assert!((radius(&lengthened) - radius(&scaled) * 1.25).abs() < 0.01);
        h.view.read_with(&h.cx, |view, _| {
            let frame = &view.modeling_frame().unwrap().canvas;
            assert_eq!(
                frame
                    .geometry
                    .meshes
                    .iter()
                    .map(|m| m.vertices.clone())
                    .collect::<Vec<_>>(),
                children
            );
            let index = view.editor.document().unwrap().object_index();
            assert_eq!(index.get(&object).unwrap().properties, pose);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (4, 0));
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        });
    }
    Ok(())
}

//! Fixture selection uses displayed geometry; measured edits use normal GPUI input.
use super::*;
use zipzi_editor_core::{ControlKind, HitTarget, ObjectKind, ObjectRef, OverlayVertexKind};

pub(super) struct Drag {
    pub start: Point<Pixels>,
    pub selected: Vec<(u32, Point<Pixels>)>,
    pub rotation_center: Option<Point<Pixels>>,
    manual_destination: Option<Point<Pixels>>,
}

impl Drag {
    pub fn position(&self, sample: usize) -> Point<Pixels> {
        let phase = sample as f32 * 0.13;
        if let Some(destination) = self.manual_destination {
            return self.start + (destination - self.start) * (0.3 + phase.sin() * 0.1);
        }
        if let Some(center) = self.rotation_center {
            let (sin, cos) = (0.12 + phase.sin() * 0.18).sin_cos();
            let delta = self.start - center;
            center + point(delta.x * cos - delta.y * sin, delta.x * sin + delta.y * cos)
        } else {
            self.start + point(px(phase.sin() * 12.), px((phase * 0.7).cos() * 6.))
        }
    }
}

pub(super) fn frame_ready(view: &EditorView) -> bool {
    let (Some(shown), Some(evaluated), Some(_document)) = (
        view.modeling_frame(),
        view.editor.evaluated_frame(),
        view.editor.document(),
    ) else {
        return false;
    };
    view.editor.has_presented_frame(Some(shown.canvas.version))
        && std::sync::Arc::ptr_eq(&shown.canvas.geometry.source.viewport_frame, &evaluated)
        && Some(shown.canvas.transform)
            == zipzi_editor_core::ViewportTransform::new(
                view.editor.canvas_metrics(),
                view.editor.view(),
            )
}

pub(super) fn prepare(
    view: &mut EditorView,
    mode: u64,
    cx: &mut Context<EditorView>,
) -> anyhow::Result<Drag> {
    let config = CONFIG.get().unwrap();
    let frame = view
        .modeling_frame()
        .context("no displayed canvas")?
        .canvas
        .clone();
    ensure!(
        view.editor.has_presented_frame(Some(frame.version)),
        "stale canvas frame"
    );
    let bounds = view
        .dock
        .as_ref()
        .unwrap()
        .active_viewport()
        .read(cx)
        .benchmark_bounds()
        .context("canvas has no bounds")?;
    let scene = &frame.geometry;
    let required = if mode == 6 { config.vertices } else { 1 };
    let mut candidates: Vec<(ObjectRef, usize)> = if mode <= 6 {
        scene
            .meshes
            .iter()
            .filter(|m| scene.mesh_visible(&m.id))
            .map(|m| (ObjectRef::new(ObjectKind::Mesh, &m.id), m.vertices.len()))
            .collect()
    } else {
        let kind = if mode == 7 {
            ControlKind::Warp
        } else {
            ControlKind::Rotation
        };
        scene
            .controls
            .iter()
            .filter(|c| c.enabled && c.kind == kind)
            .map(|c| {
                (
                    c.object.clone(),
                    scene
                        .member_meshes(&c.object)
                        .iter()
                        .filter(|m| scene.mesh_visible(&m.id))
                        .map(|m| m.vertices.len())
                        .sum(),
                )
            })
            .filter(|(_, count)| *count > 0)
            .collect()
    };
    // Stable sort preserves scene order for equal sizes (import IDs may be random).
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    let requested = match mode {
        5 | 6 => config.mesh.as_deref(),
        7 => config.warp.as_deref(),
        _ => config.rotation.as_deref(),
    };
    for (object, affected_vertices) in candidates {
        if requested.is_some_and(|id| id != object.id) {
            continue;
        }
        view.editor
            .execute(EditorCommand::SelectObjects(Vec::new()))?;
        view.editor
            .execute(EditorCommand::SelectObject(object.clone()))?;
        if config.manual_mesh
            && view
                .editor
                .document()
                .unwrap()
                .manual_mesh_target()
                .is_err()
        {
            continue;
        }
        let overlay = view.editor.overlay();
        if !config.manual_mesh && overlay.unavailable.is_some() {
            continue;
        }
        let manual_vertices: Vec<_> = if config.manual_mesh {
            let document = view.editor.document().unwrap();
            let host = document.runtime_host();
            let runtime = host.read();
            let source = runtime
                .document(document.runtime_session_id())?
                .get_mesh(&object.id)
                .unwrap();
            scene
                .mesh(&object.id)
                .unwrap()
                .vertices
                .iter()
                .enumerate()
                .filter(|(slot, _)| {
                    (0.15..0.85).contains(&source.uvs[*slot].x)
                        && (0.15..0.85).contains(&source.uvs[*slot].y)
                })
                .map(|(_, &(id, p))| zipzi_editor_core::OverlayVertex {
                    id,
                    position: frame.transform.to_viewport(p),
                    kind: OverlayVertexKind::Mesh,
                    selected: false,
                    hovered: false,
                    editable: true,
                })
                .collect()
        } else {
            vec![]
        };
        let mut selected = Vec::new();
        let mut rotation_center = None;
        let mut manual_destination = None;
        for vertex in if config.manual_mesh {
            &manual_vertices
        } else {
            &overlay.vertices
        } {
            let p = vertex.position;
            let absolute = bounds.origin + point(px(p.x), px(p.y));
            // Keep the complete motion inside the canvas, away from floating controls.
            if p.x < 40.
                || p.y < 40.
                || p.x > bounds.size.width.as_f32() - 40.
                || p.y > bounds.size.height.as_f32() - 80.
            {
                continue;
            }
            let hit = scene.hit(
                p,
                frame.transform,
                std::slice::from_ref(&object),
                view.editor.active_tool(),
                view.editor.deformer_visibility(),
            );
            let valid = if config.manual_mesh {
                true
            } else if mode == 8 {
                vertex.kind == OverlayVertexKind::RotationTip
                    && hit == Some(HitTarget::RotationHandle(object.clone()))
            } else {
                matches!(hit, Some(HitTarget::Component(ref c)) if c.object == object && c.id == vertex.id)
            };
            if !valid {
                continue;
            }
            if mode == 8 {
                let origin = overlay
                    .vertices
                    .iter()
                    .find(|v| v.kind == OverlayVertexKind::RotationCenter)
                    .context("rotation has no origin")?
                    .position;
                rotation_center = Some(bounds.origin + point(px(origin.x), px(origin.y)));
                let radius = (p.x - origin.x).hypot(p.y - origin.y);
                if radius < 40. {
                    continue;
                }
            }
            if config.manual_mesh {
                let mesh = scene.mesh(&object.id).unwrap();
                let positions: std::collections::HashMap<_, _> =
                    mesh.vertices.iter().copied().collect();
                let destination = mesh
                    .triangles
                    .iter()
                    .filter(|t| t.contains(&vertex.id))
                    .map(|t| {
                        let p = zipzi_editor_core::ModelPoint {
                            x: t.iter().map(|id| positions[id].x).sum::<f32>() / 3.,
                            y: t.iter().map(|id| positions[id].y).sum::<f32>() / 3.,
                        };
                        let p = frame.transform.to_viewport(p);
                        let distance = (p.x - vertex.position.x).hypot(p.y - vertex.position.y);
                        (distance, bounds.origin + point(px(p.x), px(p.y)))
                    })
                    .max_by(|a, b| a.0.total_cmp(&b.0));
                let Some((_, destination)) = destination.filter(|(d, _)| *d > 24.) else {
                    continue;
                };
                manual_destination = Some(destination);
            }
            selected.push((vertex.id, absolute));
            if selected.len() == required {
                break;
            }
        }
        if selected.len() != required {
            continue;
        }
        let drag = Drag {
            start: selected[0].1,
            selected,
            rotation_center,
            manual_destination,
        };
        if (0..config.samples + config.warmup)
            .any(|sample| !bounds.contains(&drag.position(sample)))
        {
            continue;
        }
        eprintln!(
            "benchmark fixture: {}",
            serde_json::json!({
                "mode": mode, "object": object.id,
                "component_ids": drag.selected.iter().map(|(id, _)| id).collect::<Vec<_>>(),
                "affected_vertices": affected_vertices,
            })
        );
        frame_trace::emit(
            frame_trace::next_id(),
            "benchmark_fixture",
            &[
                ("mode", mode),
                ("components", required as u64),
                ("affected_vertices", affected_vertices as u64),
            ],
        );
        if config.manual_mesh {
            view.editor
                .enter_manual_mesh(view.editor.active_id().unwrap())?;
        }
        view.redraw(cx);
        return Ok(drag);
    }
    anyhow::bail!(
        "mode {mode}: no editable, visible target with {required} reachable controls (requested: {requested:?}); use a model with mesh, Warp and Rotation controls at editable keyforms"
    )
}

pub(super) fn select_components(
    view: &WeakEntity<EditorView>,
    drag: &Drag,
    window: &mut Window,
    cx: &mut App,
) -> anyhow::Result<()> {
    if drag.rotation_center.is_some() {
        return Ok(());
    }
    for (index, (_, position)) in drag.selected.iter().enumerate() {
        dispatch_pointer(
            window,
            MouseDownEvent {
                button: MouseButton::Left,
                position: *position,
                click_count: 1,
                modifiers: Modifiers {
                    shift: index > 0,
                    ..Default::default()
                },
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        dispatch_pointer(
            window,
            MouseUpEvent {
                button: MouseButton::Left,
                position: *position,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
    }
    view.update(cx, |view, _| {
        let selected: Vec<_> = view
            .editor
            .overlay()
            .vertices
            .into_iter()
            .filter(|v| v.selected)
            .map(|v| v.id)
            .collect();
        ensure!(
            selected.len() == drag.selected.len()
                && drag.selected.iter().all(|(id, _)| selected.contains(id)),
            "component selection did not reach the intended controls"
        );
        Ok(())
    })?
}

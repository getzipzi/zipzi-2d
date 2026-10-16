//! CPU interaction smoke test for saved projects passed on the command line.
//! Runs selection and cancelled edits only; source projects are never saved.
use editor_core::*;
use std::{path::Path, time::Instant};

fn main() -> anyhow::Result<()> {
    for path in std::env::args().skip(1) {
        let document = Document::open(&Path::new(&path).canonicalize()?)?;
        let mut editor = Editor::from_document(document);
        editor.set_canvas_metrics(ViewportMetrics {
            width: 1200.,
            height: 800.,
            scale_factor: 2.,
        });
        let frame = editor.preview()?.canvas.unwrap();
        assert!(editor.present_frame(frame.clone()));
        println!(
            "{path}: {} meshes, {} helpers",
            frame.geometry.meshes.len(),
            frame.geometry.controls.len()
        );
        for tool in [ToolId::Select, ToolId::Glue] {
            editor.execute(EditorCommand::SetTool(tool))?;
            let mut times = Vec::new();
            let mut candidate_count = 0;
            for y in 0..8 {
                for x in 0..12 {
                    let p = ViewportPoint {
                        x: 50. + x as f32 * 100.,
                        y: 50. + y as f32 * 100.,
                    };
                    let start = Instant::now();
                    editor.canvas_input(CanvasInput::Move {
                        point: p,
                        left_down: false,
                    })?;
                    let overlay = editor.overlay();
                    assert!(
                        overlay
                            .vertices
                            .iter()
                            .all(|v| v.position.x.is_finite() && v.position.y.is_finite())
                    );
                    assert!(
                        overlay
                            .lines
                            .iter()
                            .chain(&overlay.accent_lines)
                            .chain(&overlay.hover_lines)
                            .all(|(a, b)| [a.x, a.y, b.x, b.y].iter().all(|v| v.is_finite()))
                    );
                    times.push(start.elapsed().as_secs_f64() * 1000.);
                    for c in editor.pick_candidates(p, Some(frame.version)) {
                        assert!(frame.geometry.index.get(&c.object).is_some());
                        candidate_count += 1;
                    }
                    editor.canvas_input(CanvasInput::Down {
                        point: p,
                        shift: false,
                    })?;
                    editor.canvas_input(CanvasInput::Up { point: p })?;
                    assert!(!editor.gesture_active());
                    assert!(!editor.document().unwrap().has_pending_edit());
                    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
                }
            }
            times.sort_by(f64::total_cmp);
            println!(
                "  {tool:?}: 96 hover/click cycles; {candidate_count} valid candidates; debug CPU hover+overlay p95={:.3}ms",
                times[91]
            );
        }
        let mut editable = 0;
        let mut rejected = 0;
        for mesh in frame.geometry.meshes.iter().take(24) {
            editor.execute(EditorCommand::SelectObject(ObjectRef::new(
                ObjectKind::Mesh,
                &mesh.id,
            )))?;
            let overlay = editor.overlay();
            if overlay.unavailable.is_some() || !frame.geometry.mesh_visible(&mesh.id) {
                continue;
            }
            let original = editor.document().unwrap().selected_vertex_positions();
            let Some((_, vertex)) = mesh.vertices.first() else {
                continue;
            };
            let start = frame.transform.to_viewport(*vertex);
            assert!(matches!(frame.geometry.hit(start, frame.transform,
                &editor.document().unwrap().context().selections, ToolId::Select, DeformerVisibility::default()),
                Some(HitTarget::Component(ComponentRef { object, .. })) if object.id == mesh.id));
            editor.canvas_input(CanvasInput::Down {
                point: start,
                shift: false,
            })?;
            match editor.canvas_input(CanvasInput::Move {
                point: ViewportPoint {
                    x: start.x + 8.,
                    y: start.y + 5.,
                },
                left_down: true,
            }) {
                Ok(_) => editable += 1,
                Err(e) => {
                    rejected += 1;
                    println!("  rejected {}: {e}", mesh.id);
                }
            }
            editor.canvas_input(CanvasInput::Cancel)?;
            assert_eq!(
                editor.document().unwrap().selected_vertex_positions(),
                original
            );
            assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
            let next = editor.preview()?.canvas.unwrap();
            assert!(editor.present_frame(next));
        }
        println!(
            "  eligible mesh drag/cancel: {editable} accepted, {rejected} rejected; history unchanged"
        );
        let mut beziers = 0;
        editor.execute(EditorCommand::SetTool(ToolId::Select))?;
        for control in frame
            .geometry
            .controls
            .iter()
            .filter(|c| c.kind == ControlKind::Warp)
            .take(24)
        {
            editor.execute(EditorCommand::SelectObject(control.object.clone()))?;
            let before = editor.preview()?.canvas.unwrap();
            assert!(editor.present_frame(before.clone()));
            let overlay = editor.overlay();
            if overlay.unavailable.is_some() {
                continue;
            }
            let Some(handle) = overlay.handles.iter().find(|h| h.round) else {
                continue;
            };
            let point = handle.position;
            editor.canvas_input(CanvasInput::Down {
                point,
                shift: false,
            })?;
            let changed = editor.canvas_input(CanvasInput::Move {
                point: ViewportPoint {
                    x: point.x + 4.,
                    y: point.y - 4.,
                },
                left_down: true,
            })?;
            assert!(changed, "Bezier tangent must edit its lattice");
            editor.canvas_input(CanvasInput::Cancel)?;
            let after = editor.preview()?.canvas.unwrap();
            for (a, b) in before
                .geometry
                .meshes
                .iter()
                .zip(after.geometry.meshes.iter())
            {
                assert_eq!(a.vertices, b.vertices);
            }
            assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
            assert!(editor.present_frame(after));
            beziers += 1;
        }
        println!(
            "  Bezier tangent drag/cancel: {beziers} accepted; original mesh geometry and history preserved"
        );
    }
    Ok(())
}

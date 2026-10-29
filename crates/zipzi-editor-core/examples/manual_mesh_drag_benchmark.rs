//! CPU drag/preview attribution for the normal manual-mesh gesture path.
//! Run --release MODEL [MESH_ID_OR_RUNTIME_ID] [SAMPLES]; never saves the model.
use anyhow::{Context, Result, ensure};
use std::{path::PathBuf, time::Instant};
use zipzi_editor_core::*;

fn present(editor: &mut Editor) -> Result<CanvasFrame> {
    let frame = editor.preview()?.canvas.context("no canvas")?;
    ensure!(editor.present_frame(frame.clone()), "stale frame");
    Ok(frame)
}
fn summary(mut values: Vec<f64>) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    serde_json::json!({"count": values.len(), "p50_ms":values[values.len()/2], "p95_ms":values[values.len()*95/100]})
}
fn main() -> Result<()> {
    ensure!(!cfg!(debug_assertions), "use --release");
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().context("MODEL [MESH] [SAMPLES]")?).canonicalize()?;
    let requested = args.next();
    let samples: usize = args.next().unwrap_or_else(|| "60".into()).parse()?;
    ensure!(samples >= 2, "need >=2 samples");
    let load = if path.to_string_lossy().ends_with(".model3.json") {
        LoadRequest::Model3(path.clone())
    } else if path.extension().is_some_and(|s| s == "cmo3") {
        LoadRequest::Cubism(path.clone())
    } else {
        LoadRequest::Open(path.clone())
    };
    let (document, _) = load.load()?;
    let mut editor = Editor::from_document(document);
    let viewport = ViewportId::new();
    editor.register_primary_viewport(viewport, ViewportKind::Modeling)?;
    editor.set_canvas_metrics(ViewportMetrics {
        width: 1100.,
        height: 740.,
        scale_factor: 1.,
    });
    let mut layers = editor.document().unwrap().layers();
    layers.sort_by_key(|v| std::cmp::Reverse(v.vertices));
    let mut target = None;
    for layer in layers {
        let d = editor.document().unwrap();
        let host = d.runtime_host();
        let mesh = host
            .read()
            .document(d.runtime_session_id())?
            .get_mesh(&layer.id)
            .unwrap()
            .clone();
        if requested
            .as_ref()
            .is_some_and(|s| s != &mesh.id && s != &mesh.runtime_id)
        {
            continue;
        }
        editor.execute(EditorCommand::SelectObject(ObjectRef::new(
            ObjectKind::Mesh,
            &layer.id,
        )))?;
        if editor.document().unwrap().manual_mesh_target().is_ok() {
            target = Some(mesh);
            break;
        }
    }
    let mesh = target.context("no editable target")?;
    let original = mesh.clone();
    let binding = editor.primary_viewport_binding().unwrap();
    let frame = present(&mut editor)?;
    let [x0, y0, x1, y1] = frame
        .geometry
        .mesh(&mesh.id)
        .unwrap()
        .bounds
        .context("empty mesh")?;
    let zoom = (900. / (x1 - x0)).min(560. / (y1 - y0)).clamp(0.04, 32.);
    editor.set_viewport_zoom(binding, zoom, None)?;
    let frame = present(&mut editor)?;
    let center = frame.transform.to_viewport(ModelPoint {
        x: (x0 + x1) * 0.5,
        y: (y0 + y1) * 0.5,
    });
    editor.pan_viewport(
        binding,
        ViewportPoint {
            x: 550. - center.x,
            y: 370. - center.y,
        },
    )?;
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    let frame = present(&mut editor)?;
    let shown = frame.geometry.mesh(&mesh.id).unwrap();
    let mut chosen = None;
    for t in &mesh.triangles {
        let slots = t.map(|id| mesh.vertex_ids.iter().position(|v| *v == id).unwrap());
        for &slot in &slots {
            let uv = mesh.uvs[slot];
            if !(0.1..0.9).contains(&uv.x) || !(0.1..0.9).contains(&uv.y) {
                continue;
            }
            let start = frame.transform.to_viewport(shown.vertices[slot].1);
            let centroid = ModelPoint {
                x: slots.iter().map(|&i| shown.vertices[i].1.x).sum::<f32>() / 3.,
                y: slots.iter().map(|&i| shown.vertices[i].1.y).sum::<f32>() / 3.,
            };
            let end = frame.transform.to_viewport(centroid);
            if (end.x - start.x).hypot(end.y - start.y) > 24. {
                chosen = Some((mesh.vertex_ids[slot], start, end));
                break;
            }
        }
        if chosen.is_some() {
            break;
        }
    }
    let (vertex, start, end) = chosen.context("no reachable interior drag")?;
    editor.begin_canvas_gesture(binding, start, false, false, false)?;
    let token = editor
        .current_viewport_interaction()
        .context("no gesture")?;
    let mut input = vec![];
    let mut preview = vec![];
    #[allow(unused_mut)]
    let mut stages = std::collections::BTreeMap::<String, u64>::new();
    for sample in 0..samples + 5 {
        let a = 0.3 + 0.1 * (sample as f32 * 0.3).sin();
        let point = ViewportPoint {
            x: start.x + (end.x - start.x) * a,
            y: start.y + (end.y - start.y) * a,
        };
        #[cfg(feature = "bench-instrumentation")]
        let capture = zipzi_core::benchmark::Capture::start();
        let now = Instant::now();
        let update = editor.update_canvas_gesture(
            token,
            CanvasInput::Move {
                point,
                left_down: true,
            },
        )?;
        let input_ms = now.elapsed().as_secs_f64() * 1000.;
        ensure!(
            update.model_changed,
            "input was refused: {:?}",
            editor.canvas_notice()
        );
        let now = Instant::now();
        std::hint::black_box(editor.document().unwrap().object_index());
        present(&mut editor)?;
        std::hint::black_box(editor.overlay());
        let preview_ms = now.elapsed().as_secs_f64() * 1000.;
        #[cfg(feature = "bench-instrumentation")]
        for (name, value) in capture.finish() {
            if sample >= 5 {
                *stages.entry(name.to_string()).or_default() += value.self_ns;
            }
        }
        if sample >= 5 {
            input.push(input_ms);
            preview.push(preview_ms);
        }
    }
    ensure!(
        editor.document().unwrap().has_pending_edit(),
        "missing candidate"
    );
    let history = editor.document().unwrap().history_lengths();
    editor.cancel_gesture();
    ensure!(
        editor.document().unwrap().history_lengths() == history,
        "cancel changed history"
    );
    let d = editor.document().unwrap();
    let host = d.runtime_host();
    ensure!(
        host.read()
            .document(d.runtime_session_id())?
            .get_mesh(&mesh.id)
            == Some(&original),
        "cancel changed source"
    );
    println!(
        "{}",
        serde_json::json!({"model":path,"mesh":mesh.id,"runtime_id":mesh.runtime_id,"vertex":vertex,"vertices":mesh.vertex_ids.len(),"input":summary(input),"preview":summary(preview),"self_ns":stages})
    );
    Ok(())
}

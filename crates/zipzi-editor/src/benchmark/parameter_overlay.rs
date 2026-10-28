//! Opt-in parameter/whole-model overlay diagnosis through the native window harness.
use super::*;
use zipzi_editor_core::{ObjectKind, ObjectRef};

#[derive(Default)]
struct Options {
    selection: String,
    paint: String,
    detail: bool,
    eager_canvas: bool,
    uncached_creation_menus: bool,
    unlayered_markers: bool,
    tessellated_edges: bool,
}
static OPTIONS: OnceLock<Options> = OnceLock::new();
static INITIAL: OnceLock<std::sync::Arc<zipzi_core::DrawableFrame>> = OnceLock::new();

pub(super) fn init(automatic: bool) -> anyhow::Result<()> {
    let selection = std::env::var("ZIPZI_BENCH_SELECTION").unwrap_or("first".into());
    let paint = std::env::var("ZIPZI_BENCH_OVERLAY_PAINT").unwrap_or("all".into());
    let detail = std::env::var("ZIPZI_BENCH_OVERLAY_DETAIL").is_ok_and(|v| v == "1");
    let eager_canvas = std::env::var("ZIPZI_BENCH_EAGER_CANVAS").is_ok_and(|v| v == "1");
    let uncached_creation_menus =
        std::env::var("ZIPZI_BENCH_UNCACHED_CREATION_MENUS").is_ok_and(|v| v == "1");
    let unlayered_markers = std::env::var("ZIPZI_BENCH_UNLAYERED_MARKERS").is_ok_and(|v| v == "1");
    let tessellated_edges = std::env::var("ZIPZI_BENCH_TESSELLATED_EDGES").is_ok_and(|v| v == "1");
    ensure!(
        ["first", "none", "all"].contains(&selection.as_str()),
        "invalid benchmark selection"
    );
    ensure!(
        ["all", "none", "vertices", "edges"].contains(&paint.as_str()),
        "invalid overlay paint mode"
    );
    ensure!(
        !detail || cfg!(feature = "bench-instrumentation"),
        "overlay detail requires bench-instrumentation"
    );
    ensure!(
        selection == "first"
            && paint == "all"
            && !detail
            && !eager_canvas
            && !uncached_creation_menus
            && !unlayered_markers
            && !tessellated_edges
            || automatic && std::env::var("ZIPZI_BENCH_PARAMETERS").is_ok_and(|v| v == "1"),
        "overlay diagnostics require automatic parameter workload"
    );
    let _ = OPTIONS.set(Options {
        selection,
        paint,
        detail,
        eager_canvas,
        uncached_creation_menus,
        unlayered_markers,
        tessellated_edges,
    });
    Ok(())
}

pub(super) fn metadata() -> serde_json::Value {
    OPTIONS.get().map_or(serde_json::Value::Null, |o| {
        serde_json::json!({
            "selection": o.selection, "paint": o.paint, "detail": o.detail,
            "eager_canvas": o.eager_canvas, "uncached_creation_menus": o.uncached_creation_menus,
            "unlayered_markers": o.unlayered_markers,
            "tessellated_edges": o.tessellated_edges,
        })
    })
}

pub(crate) fn eager_canvas() -> bool {
    OPTIONS.get().is_some_and(|o| o.eager_canvas)
}

pub(crate) fn layer_markers() -> bool {
    !OPTIONS.get().is_some_and(|o| o.unlayered_markers)
}

pub(crate) fn tessellated_edges() -> bool {
    OPTIONS.get().is_some_and(|o| o.tessellated_edges)
}
pub(crate) fn uncached_creation_menus() -> bool {
    OPTIONS.get().is_some_and(|o| o.uncached_creation_menus)
}

pub(super) fn prepare(
    view: &mut EditorView,
    window: &mut Window,
    cx: &mut Context<EditorView>,
) -> anyhow::Result<()> {
    let o = OPTIONS.get().unwrap();
    if o.selection != "first" {
        let frame = view.modeling_frame().context("no presented model")?;
        let objects = if o.selection == "all" {
            let locks = view.editor.document().unwrap().object_state_index();
            frame
                .canvas
                .geometry
                .meshes
                .iter()
                .filter(|m| frame.canvas.geometry.mesh_visible(&m.id))
                .map(|m| ObjectRef::new(ObjectKind::Mesh, &m.id))
                .filter(|r| !locks.is_locked(r))
                .collect()
        } else {
            Vec::new()
        };
        view.editor.execute(EditorCommand::SelectObjects(objects))?;
        view.refresh_document(window, cx);
    }
    let document = view.editor.document().unwrap();
    let overlay = view.editor.overlay();
    let source = &view
        .modeling_frame()
        .unwrap()
        .canvas
        .geometry
        .source
        .viewport_frame;
    let _ = INITIAL.set(source.clone());
    let audit = serde_json::json!({
        "options": metadata(), "selection": document.context().selections,
        "vertices": overlay.vertices.len(), "edges": overlay.lines.len(),
        "boundary_edges": overlay.accent_lines.len(),
        "model_meshes": source.drawables.len(), "model_parameters": source.parameters.len(),
        "parameters": source.parameters,
    });
    let trace = PathBuf::from(std::env::var_os("ZIPZI_FRAME_TRACE").unwrap());
    std::fs::write(
        trace.with_file_name("overlay-audit.json"),
        serde_json::to_vec_pretty(&audit)?,
    )?;
    frame_trace::emit(
        frame_trace::next_id(),
        "overlay_fixture",
        &[
            ("objects", document.context().selections.len() as u64),
            ("vertices", overlay.vertices.len() as u64),
            ("edges", overlay.lines.len() as u64),
        ],
    );
    Ok(())
}

pub(super) fn finish_workload(view: &EditorView, cx: &App) -> anyhow::Result<()> {
    frame_trace::emit(frame_trace::next_id(), "parameter_phase_end", &[]);
    let initial = INITIAL.get().context("missing initial frame")?;
    let source = &view
        .modeling_frame()
        .context("missing final frame")?
        .canvas
        .geometry
        .source
        .viewport_frame;
    let changed_meshes = initial
        .drawables
        .iter()
        .zip(&source.drawables)
        .filter(|(a, b)| a.positions != b.positions)
        .count();
    let parameter = view
        .dock
        .as_ref()
        .unwrap()
        .parameters
        .read(cx)
        .benchmark_slider_parameter(cx)
        .context("missing parameter slider")?;
    ensure!(
        changed_meshes > 0,
        "parameter workload did not deform any mesh"
    );
    let audit = serde_json::json!({"changed_meshes": changed_meshes,
        "parameter_id": parameter.id, "parameter_name": parameter.name,
        "parameter_value": parameter.value, "minimum": parameter.minimum, "maximum": parameter.maximum,
        "evaluations": view.editor.evaluation_count(),
        "selection_count": view.editor.document().unwrap().context().selections.len(),
    });
    let trace = PathBuf::from(std::env::var_os("ZIPZI_FRAME_TRACE").unwrap());
    std::fs::write(
        trace.with_file_name("parameter-audit.json"),
        serde_json::to_vec_pretty(&audit)?,
    )?;
    Ok(())
}

pub(crate) fn capture<T>(origin: u64, f: impl FnOnce() -> T) -> T {
    #[cfg(feature = "bench-instrumentation")]
    if OPTIONS.get().is_some_and(|o| o.detail) {
        let id = frame_trace::next_id();
        let start = frame_trace::now_ns();
        let capture = zipzi_core::benchmark::Capture::start();
        let value = f();
        let stages = capture.finish();
        let end = frame_trace::now_ns();
        frame_trace::emit_at(
            id,
            "overlay_build",
            start,
            &[("end_ns", end), ("origin", origin)],
        );
        for (name, m) in stages {
            frame_trace::emit_at(
                id,
                name,
                start,
                &[
                    ("calls", m.calls),
                    ("inclusive_ns", m.inclusive_ns),
                    ("self_ns", m.self_ns),
                ],
            );
        }
        return value;
    }
    let _ = origin;
    f()
}

/// Independent, nestable UI scopes; overlay Capture keeps its own span stack.
pub(crate) fn ui_span(stage: &'static str) -> Option<UiSpan> {
    OPTIONS.get().filter(|o| o.detail).map(|_| UiSpan {
        id: frame_trace::next_id(),
        stage,
        start: frame_trace::now_ns(),
        input: frame_trace::current_input(),
    })
}
pub(crate) struct UiSpan {
    id: u64,
    stage: &'static str,
    start: u64,
    input: u64,
}
impl Drop for UiSpan {
    fn drop(&mut self) {
        frame_trace::emit_at(
            self.id,
            self.stage,
            self.start,
            &[("end_ns", frame_trace::now_ns()), ("input", self.input)],
        );
    }
}

pub(crate) fn paint_edges() -> bool {
    OPTIONS
        .get()
        .is_none_or(|o| o.paint == "all" || o.paint == "edges")
}
pub(crate) fn paint_vertices() -> bool {
    OPTIONS
        .get()
        .is_none_or(|o| o.paint == "all" || o.paint == "vertices")
}
pub(crate) fn paint_any() -> bool {
    OPTIONS.get().is_none_or(|o| o.paint != "none")
}

#[cfg(all(feature = "bench-instrumentation", target_os = "macos"))]
pub(crate) fn observer() -> Option<fn(zipzi_editor_render::OverlayPass, &metal::CommandBufferRef)> {
    OPTIONS.get().filter(|o| o.detail).map(|_| observe_gpu as _)
}

#[cfg(all(feature = "bench-instrumentation", target_os = "macos"))]
fn observe_gpu(pass: zipzi_editor_render::OverlayPass, command: &metal::CommandBufferRef) {
    // A single diagnostics worker waits for retained commands; the UI never waits.
    static GPU: OnceLock<mpsc::Sender<(u64, metal::CommandBuffer)>> = OnceLock::new();
    let sender = GPU.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<(u64, metal::CommandBuffer)>();
        std::thread::spawn(move || {
            while let Ok((id, command)) = receiver.recv() {
                command.wait_until_completed();
                let object = unsafe {
                    &*(command.as_ref() as *const metal::CommandBufferRef
                        as *const objc2::runtime::AnyObject)
                };
                let start: f64 = unsafe { objc2::msg_send![object, GPUStartTime] };
                let end: f64 = unsafe { objc2::msg_send![object, GPUEndTime] };
                frame_trace::emit(
                    id,
                    "overlay_gpu_complete",
                    &[("status", command.status() as u64)],
                );
                if start > 0. && end >= start {
                    frame_trace::emit_at(id, "overlay_gpu_start", (start * 1e9) as u64, &[]);
                    frame_trace::emit_at(id, "overlay_gpu_end", (end * 1e9) as u64, &[]);
                }
            }
        });
        sender
    });
    let id = frame_trace::next_id();
    frame_trace::emit(
        id,
        "overlay_commit",
        &[(
            "pass",
            match pass {
                zipzi_editor_render::OverlayPass::All => 0,
                zipzi_editor_render::OverlayPass::Edges => 1,
                zipzi_editor_render::OverlayPass::Vertices => 2,
            },
        )],
    );
    let _ = sender.send((id, command.to_owned()));
}

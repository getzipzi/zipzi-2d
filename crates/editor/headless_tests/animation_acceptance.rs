//! Local licensed assets supplement the portable deterministic fixtures.
use crate::screenshots::Harness;
use anyhow::{Context as _, ensure};
use editor_core::{
    AnimationEvaluator, CommandId, EditorCommand, LoadRequest, ObjectKind, ObjectRef, WorkspaceKind,
};
use gpui_kit::{AppContext as _, frame_trace, test::TestWindowExt};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn update<T>(
    h: &mut Harness,
    f: impl FnOnce(
        &mut crate::EditorView,
        &mut gpui_kit::Window,
        &mut gpui_kit::Context<crate::EditorView>,
    ) -> T,
) -> anyhow::Result<T> {
    Ok(h.cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| f(view, window, cx))
    })?)
}
fn command(h: &mut Harness, command: CommandId) -> anyhow::Result<()> {
    update(h, |view, window, cx| view.run_command(command, window, cx))
}
fn clock(h: &mut Harness, time: f32) -> anyhow::Result<()> {
    update(h, |view, _, cx| -> anyhow::Result<()> {
        if !view.editor.animation().unwrap().playing {
            view.editor.toggle_animation()?;
        }
        view.editor.advance_animation_clock(time);
        view.editor.pause_animation();
        view.redraw(cx);
        Ok(())
    })?
}
fn rss() -> u64 {
    let result = std::process::Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&result.stdout)
        .trim()
        .parse::<u64>()
        .unwrap_or(0)
        * 1024
}
fn pump(h: &mut Harness) -> anyhow::Result<()> {
    h.cx.run_until_parked();
    h.cx.update_window(h.window, |_, window, cx| window.render_frame(cx))?;
    Ok(())
}
fn mark(model: usize, phase: u64) -> u64 {
    eprintln!("animation acceptance model={model} phase={phase}");
    let id = frame_trace::next_id();
    frame_trace::emit(
        id,
        "animation_workload",
        &[("model", model as u64), ("phase", phase)],
    );
    id
}

fn playback(h: &mut Harness, model: usize, samples: usize) -> anyhow::Result<serde_json::Value> {
    // Wall-clock targets at 60 Hz, real evaluator/Metal mailboxes, real GPUI UI draws.
    // The headless driver replaces only the app timer, not SDK stepping or acceptance.
    mark(model, 1);
    update(h, |view, _, cx| {
        view.editor.toggle_animation().unwrap();
        view.redraw(cx);
    })?;
    let start = Instant::now();
    let mut frames = 0;
    let mut last = None;
    let mut max_lag = 0.;
    let mut memory = Vec::new();
    for tick in 0..samples {
        let deadline = start + Duration::from_secs_f64(tick as f64 / 60.);
        while Instant::now() < deadline {
            h.cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(1));
        }
        let target = (start.elapsed().as_secs_f32() * 60.).floor() / 60.;
        update(h, |view, _, cx| {
            view.editor.advance_animation_clock(target);
            view.request_preview();
            view.notify_animation(cx);
        })?;
        pump(h)?;
        h.view.read_with(&h.cx, |view, _| {
            let a = view.editor.animation().unwrap();
            assert!(a.error.is_none(), "{:?}", a.error);
            if let Some(snapshot) = view.displayed_animation() {
                if last != Some(snapshot.time) {
                    frames += 1;
                    last = Some(snapshot.time);
                }
                max_lag = f32::max(max_lag, a.time - snapshot.time);
            }
        });
        if tick % 60 == 0 {
            memory.push(json!({"tick":tick, "rss_bytes":rss(), "gpu_bytes":h.view.read_with(&h.cx, |view, _| view.gpu_device.as_ref().unwrap().current_allocated_size())}));
        }
    }
    update(h, |view, _, cx| {
        view.editor.pause_animation();
        view.redraw(cx);
    })?;
    h.capture()?; // the final target must drain and be accepted
    mark(model, 0);
    ensure!(frames > 0, "continuous playback starved every frame");
    Ok(
        json!({"requests":samples,"observed_frames":frames,"wall_seconds":start.elapsed().as_secs_f64(),"max_lag_seconds":max_lag,"memory":memory}),
    )
}

fn model(
    path: &Path,
    index: usize,
    output: &Path,
    samples: usize,
) -> anyhow::Result<serde_json::Value> {
    eprintln!("animation acceptance importing {}", path.display());
    let mut h = Harness::new()?;
    // Suppress the app timer because this headless driver owns clock targets.
    // UI commands and actual workers remain unchanged.
    update(&mut h, |view, _, _| view.animation_clock_running = true)?;
    let other = h
        .view
        .read_with(&h.cx, |view, _| view.editor.active_id().unwrap());
    let task = update(&mut h, |view, _, _| {
        view.editor
            .prepare_load(LoadRequest::Model3(path.to_path_buf()))
    })??
    .context("no load task")?;
    let loaded = task.run();
    update(&mut h, |view, window, cx| -> anyhow::Result<()> {
        view.editor.finish_load(loaded)?;
        ensure!(
            view.editor.workspace() == WorkspaceKind::Modeling,
            "import changed workspace"
        );
        view.refresh_document(window, cx);
        Ok(())
    })??;
    let id = h
        .view
        .read_with(&h.cx, |view, _| view.editor.active_id().unwrap());
    let catalog = h.view.read_with(&h.cx, |view, _| {
        view.editor.document().unwrap().animation_catalog()
    });
    // Mao ships only looping clips. A local test edit verifies the same imported
    // curves/geometry in non-loop mode without modifying the licensed source files.
    if catalog.motions.len() > 1 {
        update(&mut h, |view, _, _| {
            view.editor.execute(EditorCommand::SetObjectProperty {
                object: ObjectRef::new(ObjectKind::Motion, &catalog.motions[1].selection.clip_id),
                field: "looping".into(),
                value: json!(false),
            })
        })??;
    }
    let parameter = h
        .view
        .read_with(&h.cx, |view, _| {
            view.editor
                .document()
                .unwrap()
                .parameters()
                .into_iter()
                .find(|p| p.minimum != p.maximum)
        })
        .context("no parameter")?;
    let hidden = update(&mut h, |view, _, cx| -> anyhow::Result<String> {
        let preview = view.editor.preview()?;
        let mesh = preview
            .scene
            .viewport_frame
            .drawables
            .iter()
            .find(|d| d.visible && !d.positions.is_empty())
            .context("no visible mesh")?
            .id
            .clone();
        view.editor.execute(EditorCommand::SetPreviewParameter {
            parameter_id: parameter.id.clone(),
            value: if parameter.maximum != parameter.default_value {
                parameter.maximum
            } else {
                parameter.minimum
            },
        })?;
        view.editor.execute(EditorCommand::SetObjectVisible {
            object: ObjectRef::new(ObjectKind::Mesh, &mesh),
            visible: false,
        })?;
        view.redraw(cx);
        Ok(mesh)
    })??;
    let baseline = h.view.read_with(&h.cx, |view, _| {
        (
            view.editor.document().unwrap().parameters(),
            view.editor.document().unwrap().history_lengths(),
            view.editor.document().unwrap().modified(),
        )
    });
    let before = h.capture()?;
    before.save(output.join(format!("model-{index}-modeling.png")))?;
    let before_canvas = h.canvas_image(&before)?;
    command(&mut h, CommandId::AnimationWorkspace)?;
    // This suite checks the complete SDK pipeline and replay checkpoints.
    update(&mut h, |view, _, cx| {
        view.editor
            .set_animation_preview_mode(id, editor_core::AnimationPreviewMode::Result)
            .unwrap();
        view.redraw(cx);
    })?;
    let zero = h.capture()?;
    zero.save(output.join(format!("model-{index}-animation.png")))?;
    let request = update(&mut h, |view, _, _| {
        view.editor.invalidate_animation();
        view.editor.animation_request().unwrap()
    })?;
    let mut evaluator = AnimationEvaluator::default();
    let runtime = evaluator.evaluate(&request, |_, _| true)?;
    ensure!(
        runtime
            .scene
            .viewport_frame
            .drawables
            .iter()
            .find(|d| d.id == hidden)
            .unwrap()
            .visible,
        "editor hidden leaked into runtime"
    );
    ensure!(
        runtime.snapshot.parameters[&parameter.id] == parameter.default_value,
        "trial pose leaked into runtime"
    );
    let counts = json!({"drawables":runtime.scene.viewport_frame.drawables.len(),"offscreens":runtime.scene.viewport_frame.offscreens.len(),"masks":runtime.scene.viewport_frame.drawables.iter().filter(|d| !d.masks.is_empty()).count(),"physics":catalog.physics,"pose":catalog.pose,"motions":catalog.motions.len(),"expressions":catalog.expressions.len(),"diagnostics":catalog.diagnostics.len()});
    let mut result = json!({"path":path,"counts":counts});
    if !catalog.motions.is_empty() {
        update(&mut h, |view, _, cx| {
            view.editor
                .select_motion(id, catalog.motions[0].selection.clone())
                .unwrap();
            view.redraw(cx);
        })?;
        clock(&mut h, catalog.motions[0].duration + 0.5)?;
        h.capture()?;
        ensure!(
            h.view
                .read_with(&h.cx, |view, _| view.editor.animation().unwrap().time)
                > catalog.motions[0].duration,
            "loop lost absolute clock"
        );
        command(&mut h, CommandId::RewindAnimation)?;
        h.capture()?;
        if samples > 0 {
            result["playback"] = playback(&mut h, index, samples)?;
        }
        // Actual long-running loop CPU state/cache, including fractional targets
        // that force transactional canonical replay. It never mutates UI state.
        mark(index, 2);
        let request = update(&mut h, |view, _, _| {
            view.editor.invalidate_animation();
            view.editor.animation_request().unwrap()
        })?;
        let mut evaluator = AnimationEvaluator::default();
        let mut seeks = Vec::new();
        for target in if samples > 0 {
            vec![20.125, 40.125, 80.125, 120.125]
        } else {
            vec![1.125, 0.25, 1.125]
        } {
            let mut request = editor_core::AnimationRequest {
                identity: request.identity,
                source: request.source.clone(),
                motion: request.motion.clone(),
                expressions: request.expressions.clone(),
            };
            request.identity.time = target;
            let start = Instant::now();
            evaluator.evaluate(&request, |_, _| true)?;
            let stats = evaluator.seek_cache_stats().unwrap();
            ensure!(
                stats.estimated_bytes <= stats.budget_bytes,
                "unbounded seek cache"
            );
            seeks.push(json!({"time":target,"ms":start.elapsed().as_secs_f64()*1000.,"cache_bytes":stats.estimated_bytes,"cache_budget":stats.budget_bytes,"checkpoints":stats.checkpoints}));
        }
        result["seeks"] = json!(seeks);
        // A huge obsolete loop target must yield to a short first-cycle scrub.
        mark(index, 3);
        update(&mut h, |view, _, cx| {
            if !view.editor.animation().unwrap().playing {
                view.editor.toggle_animation().unwrap();
            }
            view.editor.advance_animation_clock(10_000.);
            view.request_preview();
            view.notify_animation(cx);
        })?;
        std::thread::sleep(Duration::from_millis(20));
        h.cx.run_until_parked();
        let start = Instant::now();
        update(&mut h, |view, _, cx| {
            view.editor.seek_animation(id, 0.5).unwrap();
            view.redraw(cx);
        })?;
        h.capture()?;
        result["cancel_to_present_ms"] = json!(start.elapsed().as_secs_f64() * 1000.);
        ensure!(
            start.elapsed() < Duration::from_secs(2),
            "obsolete loop replay blocked latest target"
        );
        if catalog.motions.len() > 1 && catalog.expressions.len() > 1 {
            mark(index, 4);
            update(&mut h, |view, _, cx| {
                view.editor
                    .select_motion(id, catalog.motions[1].selection.clone())
                    .unwrap();
                view.editor
                    .select_expression(id, catalog.expressions[0].id.clone())
                    .unwrap();
                view.redraw(cx);
            })?;
            clock(&mut h, 0.5)?;
            h.capture()?;
            update(&mut h, |view, _, cx| {
                view.editor
                    .select_expression(id, catalog.expressions[1].id.clone())
                    .unwrap();
                view.redraw(cx);
            })?;
            clock(&mut h, 1.5)?;
            let combined = h.capture()?;
            combined.save(output.join(format!("model-{index}-combined.png")))?;
            let combined_canvas = h.canvas_image(&combined)?;
            let combined_state = h.view.read_with(&h.cx, |view, _| {
                view.editor.animation().unwrap().presented.clone().unwrap()
            });
            for time in [0.25, 1.5] {
                update(&mut h, |view, _, cx| {
                    view.editor.seek_animation(id, time).unwrap();
                    view.redraw(cx);
                })?;
                h.capture()?;
            }
            let replay = h.capture()?;
            ensure!(
                h.canvas_image(&replay)? == combined_canvas,
                "actual model replay pixels changed"
            );
            ensure!(
                *h.view.read_with(&h.cx, |view, _| view
                    .editor
                    .animation()
                    .unwrap()
                    .presented
                    .clone()
                    .unwrap())
                    == *combined_state,
                "actual model replay state changed"
            );
            command(&mut h, CommandId::ClearExpression)?;
            let cleared = h.capture()?;
            cleared.save(output.join(format!("model-{index}-cleared.png")))?;
            ensure!(
                h.canvas_image(&cleared)? != combined_canvas,
                "expressions had no visible effect on actual model"
            );
            clock(&mut h, catalog.motions[1].duration + 1.)?;
            h.capture().context("non-loop endpoint")?;
            h.view.read_with(&h.cx, |view, _| {
                let a = view.editor.animation().unwrap();
                assert!(!a.playing);
                assert_eq!(a.time, catalog.motions[1].duration);
            });
        }
    } else {
        ensure!(
            !h.view.read_with(&h.cx, |view, _| view
                .editor
                .command_available(view.editor.command(CommandId::PlayPause))),
            "empty model enabled playback"
        );
    }
    mark(index, 5);
    let pending = update(&mut h, |view, _, _| {
        view.editor.invalidate_animation();
        view.editor.animation_request().unwrap()
    })?;
    update(&mut h, |view, window, cx| {
        view.editor.activate(other).unwrap();
        view.refresh_document(window, cx);
    })?;
    h.capture().context("document switch")?;
    ensure!(
        !h.view.read_with(&h.cx, |view, _| view
            .editor
            .accepts_animation(pending.identity)),
        "old document frame accepted"
    );
    update(&mut h, |view, window, cx| {
        view.editor.activate(id).unwrap();
        view.refresh_document(window, cx);
    })?;
    h.capture()?;
    ensure!(
        !h.view
            .read_with(&h.cx, |view, _| view.editor.animation().unwrap().playing),
        "document resumed itself"
    );
    command(&mut h, CommandId::ModelingWorkspace)?;
    let restored = h.capture()?;
    ensure!(
        h.canvas_image(&restored)? == before_canvas,
        "return to Modeling changed pixels"
    );
    h.view.read_with(&h.cx, |view, _| {
        assert_eq!(
            (
                view.editor.document().unwrap().parameters(),
                view.editor.document().unwrap().history_lengths(),
                view.editor.document().unwrap().modified()
            ),
            baseline
        );
    });
    // Save and reopen imported resources, then verify a clean preview is still clean.
    let saved = tempfile::tempdir()?;
    let save = update(&mut h, |view, _, _| {
        view.editor
            .prepare_save(id, &saved.path().join("accepted.kasane"), false)
    })??;
    let mut completion = save.run();
    update(&mut h, |view, _, _| {
        view.editor.finish_save(&mut completion)
    })??;
    update(&mut h, |view, _, _| view.editor.close_document(id, false))??;
    let project = saved.path().join("accepted.kasane/project.kasane.json");
    let task = update(&mut h, |view, _, _| {
        view.editor.prepare_load(LoadRequest::Open(project))
    })??
    .context("no reopen task")?;
    let loaded = task.run();
    update(&mut h, |view, window, cx| {
        view.editor.finish_load(loaded).unwrap();
        view.refresh_document(window, cx);
    })?;
    command(&mut h, CommandId::AnimationWorkspace)?;
    h.capture()?;
    h.view.read_with(&h.cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.animation_catalog().motions.len(), catalog.motions.len());
        assert_eq!(
            doc.animation_catalog().expressions.len(),
            catalog.expressions.len()
        );
        assert!(!doc.modified());
        assert_eq!(doc.history_lengths(), (0, 0));
        assert!(!view.editor.animation().unwrap().playing);
    });
    result["workflow_passed"] = json!(true);
    Ok(result)
}

pub(super) fn run() -> anyhow::Result<()> {
    ensure!(
        std::env::var_os("KASANE_DEV_SESSION_DIR").is_none(),
        "unset KASANE_DEV_SESSION_DIR"
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::env::var_os("KASANE_ANIMATION_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/animation-acceptance"));
    std::fs::create_dir_all(&output)?;
    let samples = std::env::var("KASANE_ANIMATION_SAMPLES")
        .ok()
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(0);
    ensure!(samples <= 3600, "sample count too large");
    let paths = std::env::args()
        .skip(1)
        .filter(|s| !s.starts_with('-'))
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let paths = if paths.is_empty() {
        vec![
            root.join("models/local/mao/runtime/mao_pro.model3.json"),
            root.join("models/local/Shirousagi/Shirousagi.model3.json"),
            root.join("models/local/heijiao/heijiao.model3.json"),
        ]
    } else {
        paths
    };
    let mut results = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        if !path.exists() {
            eprintln!("SKIP local licensed model: {}", path.display());
            continue;
        }
        results.push(
            model(path, index, &output, samples)
                .with_context(|| format!("acceptance: {}", path.display()))?,
        );
    }
    std::fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(
            &json!({"profile":if cfg!(debug_assertions){"debug"}else{"release"},"samples":samples,"logical_window":[1100,740],"models":results}),
        )?,
    )?;
    println!(
        "Animation acceptance passed for {} local models; results: {}",
        results.len(),
        output.display()
    );
    Ok(())
}

//! Paired A16 probe: two native Metal surfaces, with another session publishing.
use super::harness::{Harness, update};
use anyhow::{Context as _, ensure};
use gpui_kit::{
    InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, point, px,
};
use serde_json::json;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use zipzi_editor_core::{Document, Editor, ViewportId, ViewportKind};

type FinishBatch = Box<dyn FnOnce() -> anyhow::Result<()>>;
mod background;

fn ready(
    h: &mut Harness,
    ids: &[ViewportId],
    mut background: Option<&mut background::Background>,
) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(background) = background.as_deref_mut() {
            background.poll()?;
        }
        h.cx.run_until_parked();
        let current = h.cx.update_window(h.window, |_, window, cx| {
            window.draw(cx).clear(cx);
            h.view.read_with(cx, |view, _| {
                ids.iter().all(|&id| {
                    view.modeling_frame_for(id).is_some_and(|frame| {
                        view.editor.accepts_viewport_preview(
                            frame.canvas.version.binding,
                            frame.canvas.version.request,
                        )
                    })
                })
            })
        })?;
        if current {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "dual viewport preview did not become current"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn stats(values: Vec<f64>) -> serde_json::Value {
    let raw = values.clone();
    let mut values = values;
    values.sort_by(f64::total_cmp);
    json!({"samples":values.len(), "p50_ms":values[(values.len()-1)/2], "p95_ms":values[((values.len()-1) as f64*0.95).ceil() as usize],"raw_ms":raw})
}

pub(super) fn run() -> anyhow::Result<()> {
    let model =
        std::env::var_os("ZIPZI_TIMELINE_MODEL").context("ZIPZI_TIMELINE_MODEL is required")?;
    let model = Path::new(&model);
    let output = std::path::PathBuf::from(
        std::env::var_os("ZIPZI_RUNTIME_INTERACTION_OUTPUT").context("output is required")?,
    );
    std::fs::create_dir_all(&output)?;
    let samples: usize = std::env::var("ZIPZI_RUNTIME_INTERACTION_SAMPLES")
        .unwrap_or_else(|_| "60".into())
        .parse()?;
    ensure!(samples >= 30, "at least 30 measured samples");
    let (document, _) = Document::import_model3(model)?;
    let mut h = Harness::new()?;
    update(&mut h, |view, window, cx| {
        view.replace_editor(Editor::from_document(document), cx);
        view.refresh_document(window, cx);
    })?;
    let ids = h.cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.install_viewport_scene(
                &[ViewportKind::Modeling, ViewportKind::Modeling],
                window,
                cx,
            )
        })
    })?;
    ready(&mut h, &ids, None)?;
    let (imported, _) = crate::test_fixture::import_model3_fixture(model)?;
    let mesh = imported.document.mesh_order()[0].clone();
    let sdk = zipzi_sdk::AuthoringSession::new(zipzi_sdk::Snapshot::new(
        imported.document,
        imported.resources,
    )?);
    let runtime = h
        .view
        .read_with(&h.cx, |view, _| view.editor.runtime_host());
    let client = runtime.write().register_client()?;
    let session = runtime.write().adopt_session(client, sdk)?;
    let batch = |index: usize| -> anyhow::Result<FinishBatch> {
        let mesh = mesh.clone();
        let mut host = runtime.write();
        let version = host.session_read(client, session)?.version();
        let draft = host.begin_draft(client, version)?;
        let job = host.request_batch(client, draft, Default::default(), move |edit, _| {
            edit.rename(
                zipzi_sdk::NameTarget::Mesh(&mesh),
                (format!("batch {index}")).into(),
            )
        })?;
        let work = host.take_work(job)?;
        drop(host);
        let thread = std::thread::spawn(move || work.run());
        let runtime = runtime.clone();
        Ok(Box::new(move || {
            let completion = thread
                .join()
                .map_err(|_| anyhow::anyhow!("batch thread panicked"))?;
            let mut host = runtime.write();
            ensure!(
                host.complete(completion).is_ok(),
                "batch completion rejected"
            );
            let result = host
                .job(client, job)?
                .result
                .clone()
                .context("batch still running")?;
            host.release_job(client, job)?;
            let zipzi_runtime::JobValue::DraftUpdated(draft) = result? else {
                anyhow::bail!("batch did not update the draft");
            };
            host.publish(client, draft, "background batch")?;
            Ok(())
        }))
    };
    let history = h.view.read_with(&h.cx, |view, _| {
        view.editor.document().unwrap().history_lengths()
    });
    let mut report = serde_json::Map::new();
    let mut background_work = serde_json::Map::new();
    let mut cases = vec![
        ("dual_viewport_switch", false, None),
        ("other_session_batch", true, None),
    ];
    if std::env::var_os("ZIPZI_RUNTIME_INTERACTION_READS").is_some() {
        for kind in [
            background::Kind::Cpu,
            background::Kind::Preview,
            background::Kind::Observe,
        ] {
            cases.push((kind.name(), false, Some(kind)));
        }
    }
    for (name, publication, reads) in cases {
        let mut continuous = reads
            .map(|kind| background::Background::start(runtime.clone(), session, kind))
            .transpose()?;
        let mut latency = vec![];
        let mut dispatch = vec![];
        for index in 0..samples + 10 {
            if let Some(background) = &mut continuous {
                background.poll()?;
            }
            let finish = if publication {
                Some(batch(index)?)
            } else {
                None
            };
            let id = ids[index % 2];
            let origin = h.view.read_with(&h.cx, |view, cx| {
                view.viewport_surface(id, cx)
                    .unwrap()
                    .read(cx)
                    .benchmark_bounds()
                    .unwrap()
                    .center()
            });
            let pan_before = h.view.read_with(&h.cx, |view, _| {
                view.editor
                    .viewport_view(view.editor.viewport_binding(id).unwrap())
                    .unwrap()
                    .pan
            });
            let start = Instant::now();
            h.cx.update_window(h.window, |_, window, cx| {
                window.dispatch_event(
                    MouseDownEvent {
                        button: MouseButton::Middle,
                        position: origin,
                        click_count: 1,
                        ..Default::default()
                    }
                    .to_platform_input(),
                    cx,
                );
                window.dispatch_event(
                    MouseMoveEvent {
                        position: origin + point(px(3.), px(2.)),
                        pressed_button: Some(MouseButton::Middle),
                        ..Default::default()
                    }
                    .to_platform_input(),
                    cx,
                );
                window.dispatch_event(
                    MouseUpEvent {
                        button: MouseButton::Middle,
                        position: origin + point(px(3.), px(2.)),
                        click_count: 1,
                        ..Default::default()
                    }
                    .to_platform_input(),
                    cx,
                );
            })?;
            let cpu = start.elapsed().as_secs_f64() * 1000.;
            ready(&mut h, &ids, continuous.as_mut())?;
            let displayed = start.elapsed().as_secs_f64() * 1000.;
            ensure!(
                h.view.read_with(&h.cx, |view, _| view
                    .editor
                    .viewport_view(view.editor.viewport_binding(id).unwrap())
                    .unwrap()
                    .pan)
                    != pan_before,
                "native pointer did not move the selected surface"
            );
            if let Some(finish) = finish {
                finish()?;
            }
            if index >= 10 {
                dispatch.push(cpu);
                latency.push(displayed);
            }
        }
        if let Some(continuous) = continuous {
            let (jobs, quanta, warmup) = continuous.finish()?;
            ensure!(
                jobs > 0 && quanta > warmup,
                "background work did not overlap measured input"
            );
            background_work.insert(name.into(), json!({"completed_jobs":jobs,"worker_steps":quanta,"warmup_steps":warmup,"poses_per_batch":4}));
        }
        report.insert(
            name.into(),
            json!({"dispatch":stats(dispatch),"to_current_metal_draw":stats(latency)}),
        );
    }
    ensure!(
        h.view.read_with(&h.cx, |view, _| view
            .editor
            .document()
            .unwrap()
            .history_lengths())
            == history,
        "background session changed visible history"
    );
    std::fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(
            &json!({"profile":if cfg!(debug_assertions){"debug"}else{"release"},"measurement":"injected GPUI input to both current Metal surfaces; excludes screenshot readback and compositor","samples":samples,"warmup":10,"visible_history_unchanged":true,"measurements":report,"background_work":background_work}),
        )?,
    )?;
    Ok(())
}

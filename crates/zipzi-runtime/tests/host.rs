//! P1 local-host foundations: real SDK state, worker completion and lifetime.
use std::{
    sync::{mpsc, Arc},
    time::Duration,
};
use zipzi_core::{ModelSetup, Vec2};
use zipzi_runtime::modeling::ModelingCommand;
use zipzi_runtime::{
    host::{HostError, LocalHost},
    *,
};
use zipzi_sdk::{ModelingIntent, ResourceEdit};

fn id(n: u32) -> String {
    format!("78000000-0000-4000-8000-{n:012x}")
}
fn host() -> LocalHost {
    LocalHost::new(RuntimeLimits::default(), 16).unwrap()
}
fn session(host: &LocalHost) -> (ClientId, SessionId) {
    host.call(|rt| {
        let client = rt.register_client()?;
        let session = rt.create_session(client, &id(1), ModelSetup::default())?;
        Ok((client, session))
    })
    .unwrap()
}

#[test]
fn local_host_composes_candidate_pixels_plan_history_and_save_reopen() {
    let host = host();
    let (client, session) = session(&host);
    let budget = host
        .call(|rt| Ok(rt.image_memory_budget().clone()))
        .unwrap();
    let edit = host
        .call(move |rt| rt.begin_edit(client, session, "triangle", None))
        .unwrap();
    host.call(move |rt| {
        rt.apply_rgba_image(
            client,
            edit,
            id(2),
            "pixels".into(),
            1,
            1,
            vec![12, 34, 56, 255],
            false,
        )?;
        let mesh = zipzi_sdk::rectangle_mesh(
            &id(3),
            "Face",
            &id(2),
            Vec2::new(0., 0.),
            Vec2::new(1., 1.),
        )?;
        rt.apply_modeling(client, edit, vec![ModelingCommand::CreateMesh { mesh }])?;
        let positions = rt
            .edit_document(client, edit)?
            .get_mesh(&id(3))
            .unwrap()
            .base_positions
            .iter()
            .map(|p| Vec2::new(p.x + 2., p.y + 3.))
            .collect();
        let vertex_ids = rt
            .edit_document(client, edit)?
            .get_mesh(&id(3))
            .unwrap()
            .vertex_ids
            .clone();
        rt.apply_modeling(
            client,
            edit,
            vec![ModelingCommand::UpdatePositions {
                id: id(3),
                vertex_ids,
                positions,
            }],
        )?;
        assert!(rt.document(session)?.get_mesh(&id(3)).is_none());
        assert!(matches!(
            rt.edit_snapshot(client, edit)?.source,
            SourceRef::Edit { sequence: 3, .. }
        ));
        let (plan, source, report) = rt.plan_modeling(
            client,
            session,
            Some(edit),
            ModelingIntent::Resource(ResourceEdit::PatchName {
                id: id(2),
                expected: "pixels".into(),
                name: "texture".into(),
            }),
        )?;
        assert!(matches!(source, SourceRef::Edit { sequence: 3, .. }));
        assert_eq!(report.affected_ids, vec![id(2)]);
        rt.apply_modeling_plan(client, edit, plan)?;
        rt.release_modeling_plan(client, plan)?;
        Ok(())
    })
    .unwrap();
    let job = host
        .call(move |rt| rt.request_commit(client, edit))
        .unwrap();
    let published = host.wait_job(client, job).unwrap();
    assert_eq!(published.state, JobState::Succeeded);
    assert!(matches!(published.result, Some(Ok(JobValue::Published(ref r))) if r.changed));
    host.call(move |rt| {
        rt.release_job(client, job)?;
        assert_eq!(rt.session(session)?.history_lengths(), (1, 0));
        assert_eq!(
            rt.session(session)?.read_texture(&id(2))?.rgba.as_ref(),
            &[12, 34, 56, 255]
        );
        rt.undo(client, session)?;
        assert!(rt.document(session)?.get_mesh(&id(3)).is_none());
        rt.redo(client, session)?;
        assert_eq!(
            rt.document(session)?.get_asset(&id(2)).unwrap().name,
            "texture"
        );
        Ok(())
    })
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("project");
    let job = host
        .call(move |rt| rt.request_save(client, session, &path))
        .unwrap();
    let saved = host.wait_job(client, job).unwrap();
    let Some(Ok(JobValue::Saved(saved))) = saved.result else {
        panic!("save failed: {saved:?}")
    };
    assert!(saved.manifest.is_file());
    host.close().unwrap();
    assert_eq!(budget.usage().total_bytes(), 0);
    assert_eq!(host.call(|rt| Ok(rt.live_counts())), Err(HostError::Closed));
    host.close().unwrap();

    let reopened = self::host();
    let client = reopened.call(|rt| rt.register_client()).unwrap();
    let OpenRequest::Pending(job) = reopened
        .call(move |rt| rt.request_open(client, &saved.manifest))
        .unwrap()
    else {
        panic!("new host must open from disk")
    };
    let result = reopened.wait_job(client, job).unwrap();
    let Some(Ok(JobValue::Opened(session))) = result.result else {
        panic!("open failed: {result:?}")
    };
    reopened
        .call(move |rt| {
            let mesh = rt.document(session)?.get_mesh(&id(3)).unwrap();
            assert_eq!(mesh.name, "Face");
            assert_eq!(mesh.base_positions[0], Vec2::new(2., 3.));
            assert_eq!(
                rt.session(session)?.read_texture(&id(2))?.rgba.as_ref(),
                &[12, 34, 56, 255]
            );
            assert_eq!(rt.session(session)?.history_lengths(), (0, 0));
            Ok(())
        })
        .unwrap();
}

#[test]
fn completion_wait_does_not_pump_runtime_or_require_a_later_getter() {
    let host = Arc::new(host());
    let (client, session) = session(&host);
    let job = host
        .call(move |rt| {
            let edit = rt.begin_edit(client, session, "resize", None)?;
            rt.apply_edit(client, edit, |candidate| {
                candidate.create_parameter(zipzi_core::Parameter {
                    id: id(10),
                    ..Default::default()
                })
            })?;
            rt.request_commit(client, edit)
        })
        .unwrap();
    let (send, receive) = mpsc::channel();
    let waiting = host.clone();
    let waiter = std::thread::spawn(move || send.send(waiting.wait_job(client, job)).unwrap());
    // No host calls, polls or getters between Job acceptance and completion.
    let info = receive
        .recv_timeout(Duration::from_secs(5))
        .expect("host must consume completion while caller is idle")
        .unwrap();
    assert_eq!(info.state, JobState::Succeeded);
    waiter.join().unwrap();
}

#[test]
fn prepared_a_and_b_outlive_unrelated_publication_and_release_at_last_owner() {
    let host = host();
    let (client, session) = session(&host);
    let (a, b, budget) = host
        .call(|rt| {
            let budget = rt.image_memory_budget().clone();
            let a =
                zipzi_sdk::PreparedImageContent::from_rgba_bytes(1, 1, vec![1, 2, 3, 255], &budget)
                    .unwrap();
            let b =
                zipzi_sdk::PreparedImageContent::from_rgba_bytes(1, 1, vec![9, 8, 7, 255], &budget)
                    .unwrap();
            Ok((a, b, budget))
        })
        .unwrap();
    let adopted = a.clone();
    host.call(move |rt| {
        let edit = rt.begin_edit(client, session, "adopt A", None)?;
        rt.apply_edit(client, edit, |candidate| {
            candidate.create_prepared_image(id(2), "A".into(), &adopted)
        })?;
        rt.commit_edit(client, edit)?;
        rt.collect_image_resources();
        Ok(())
    })
    .unwrap();
    assert_eq!(a.texture().rgba.as_ref(), &[1, 2, 3, 255]);
    assert_eq!(b.texture().rgba.as_ref(), &[9, 8, 7, 255]);
    assert_eq!(budget.usage().rgba_bytes, 8);
    drop(a);
    drop(b);
    assert_eq!(budget.usage().rgba_bytes, 4);
    host.close().unwrap();
    assert_eq!(budget.usage().total_bytes(), 0);
}

#[test]
fn shutdown_drops_unsaved_resources_and_consumes_accepted_jobs() {
    let host = host();
    let (client, session) = session(&host);
    let budget = host
        .call(move |rt| {
            let edit = rt.begin_edit(client, session, "unsaved", None)?;
            rt.apply_rgba_image(
                client,
                edit,
                id(2),
                "image".into(),
                1,
                1,
                vec![1, 2, 3, 255],
                false,
            )?;
            rt.request_commit(client, edit)?;
            Ok(rt.image_memory_budget().clone())
        })
        .unwrap();
    host.close().unwrap();
    assert_eq!(budget.usage().total_bytes(), 0);
}

#[test]
fn host_rejects_zero_capacity_and_worker_count_and_fails_closed_on_panic() {
    assert!(LocalHost::new(Default::default(), 0).is_err());
    assert!(LocalHost::new(
        RuntimeLimits {
            running_jobs: 0,
            ..Default::default()
        },
        1
    )
    .is_err());
    let host = host();
    assert_eq!(
        host.call::<()>(|_| panic!("injected adapter panic")),
        Err(HostError::Panicked)
    );
    host.close().unwrap();
    assert_eq!(host.call(|_| Ok(())), Err(HostError::Closed));
}

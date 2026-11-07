//! Optional real-model regression for default native decode admission.
use std::path::PathBuf;
use zipzi_contract::kernel_wire::ImportFormat;
use zipzi_runtime::*;

fn finish(rt: &mut Runtime, client: ClientId, job: JobId) -> JobValue {
    let work = rt.take_work(job).unwrap();
    rt.complete(work.run()).unwrap();
    let value = rt
        .job(client, job)
        .unwrap()
        .result
        .clone()
        .unwrap()
        .unwrap();
    rt.release_job(client, job).unwrap();
    value
}

#[test]
#[ignore = "requires models/local/DeepSeek; imports and round-trips the full model"]
fn deepseek_model3_save_reopens_under_default_shared_budget() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/local/DeepSeek/大肥鱼.model3.json");
    assert!(source.is_file(), "Install the local DeepSeek assets");
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(
            client,
            "00000000-0000-4000-8000-000000000001",
            Default::default(),
        )
        .unwrap();
    let version = rt.session_read(client, session).unwrap().version();
    let job = rt
        .request_import(
            client,
            version,
            &source,
            ImportFormat::Model3,
            Default::default(),
        )
        .unwrap();
    finish(&mut rt, client, job);
    let before = rt.read_snapshot(client, session).unwrap().snapshot;
    assert_eq!(before.document().mesh_order().len(), 715);
    assert_eq!(before.document().parameter_order().len(), 509);
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("deepseek.zipzi");
    let io = rt.project_io().clone();
    let version = rt.session_read(client, session).unwrap().version();
    let job = rt
        .request_save_as(client, version, &io, &path, None, Default::default())
        .unwrap();
    assert!(matches!(finish(&mut rt, client, job), JobValue::Saved(r) if r.published));
    // Keep the imported session/resources resident: replacement must admit both
    // the old snapshot and the decoder, as an ordinary Editor open does.
    let job = rt
        .request_open(client, version, &io, &path, Default::default())
        .unwrap();
    assert!(matches!(finish(&mut rt, client, job), JobValue::Opened(_)));
    let after = rt.read_snapshot(client, session).unwrap().snapshot;
    macro_rules! compare {
        ($order:ident, $record:ident) => {
            assert_eq!(before.document().$order(), after.document().$order());
            for id in before.document().$order() {
                assert_eq!(before.document().$record(id), after.document().$record(id));
            }
        };
    }
    compare!(parameter_order, parameter);
    compare!(part_order, part);
    compare!(transform_order, transform);
    compare!(mesh_order, mesh);
    compare!(binding_order, binding);
    compare!(scene_binding_order, scene_binding);
    compare!(blend_binding_order, blend_binding);
    for id in before.document().asset_order() {
        let original = before
            .resources()
            .read_texture(before.document().asset(id).unwrap())
            .unwrap();
        let reopened = after
            .resources()
            .read_texture(after.document().asset(id).unwrap())
            .unwrap();
        assert_eq!(
            (original.width, original.height),
            (reopened.width, reopened.height)
        );
        assert_eq!(original.rgba, reopened.rgba, "asset {id}");
    }
    let info = rt.session_read(client, session).unwrap();
    assert_eq!(info.history_state().undo_steps, 0);
    assert!(!info.is_dirty());
    assert_eq!(rt.task_bytes(), 0);
}

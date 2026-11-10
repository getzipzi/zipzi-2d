//! Native Glue tool coverage with two independently authored source images.
use super::super::harness::{Harness, update};
use super::{common::save, manual_mesh_scene::click_model};
use gpui_kit::test::TestWindowExt;
use zipzi_core::Vec2;
use zipzi_editor_core::{
    BrushSelectionSettings, Document, Editor, EditorCommand, ModelPoint, ObjectKind, ObjectRef,
};

#[path = "../../../../fixtures/multi-mesh-edit.rs"]
mod fixture;

pub(super) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    let mut h = Harness::with_size(1100., 790.)?;
    let mut session = fixture::create(
        h._assets.path(),
        include_bytes!("../../../zipzi-editor-core/tests/fixtures/alpha-asymmetric.png"),
    )?;
    let a = ObjectRef::new(ObjectKind::Mesh, fixture::id(2));
    let b = ObjectRef::new(ObjectKind::Mesh, fixture::id(4));
    let mesh = session.snapshot().document().mesh(&b.id).unwrap();
    let positions = mesh
        .vertex_ids
        .iter()
        .zip(&mesh.base_positions)
        .map(|(&id, p)| (id, Vec2::new(p.x - 100., p.y - 100.)))
        .collect::<Vec<_>>();
    crate::test_fixture::publish_fixture(&mut session, "Overlap", |edit| {
        edit.set_vertex_positions(&b.id, &positions)
    })?;
    let path = h._assets.path().join("glue.zipzi");
    crate::test_fixture::save_fixture(&mut session, &path)?;
    let mut document = Document::open(&path)?;
    document.select_objects(vec![a, b.clone()])?;
    update(&mut h, |view, window, cx| {
        view.replace_editor(Editor::from_document(document), cx);
        view.refresh_document(window, cx);
    })?;
    h.capture()?;
    let click = |h: &mut Harness, id: &str| {
        h.cx.update_window(h.window, |_, window, cx| window.click(id.to_owned(), cx))
    };
    click(&mut h, "tool_glue")?;
    h.capture()?;
    assert!(
        !h.view
            .read_with(&h.cx, |view, _| view.editor.manual_active())
    );
    let joint = ModelPoint { x: 60., y: 65. };
    click_model(&mut h, joint)?;
    save(&h.capture()?, directory, "glue-01-overlap.png")?;
    click(&mut h, "glue-stitch-action")?;
    save(&h.capture()?, directory, "glue-follow-controls.png")?;
    assert_eq!(
        h.view.read_with(&h.cx, |view, _| view.editor.active_tool()),
        zipzi_editor_core::ToolId::Select
    );

    let (glue, host, session) = h.view.read_with(&h.cx, |view, _| {
        let doc = view.editor.document().unwrap();
        let host = doc.runtime_host();
        let session = doc.runtime_session_id();
        let snapshot = crate::test_fixture::published_snapshot(&host, session).unwrap();
        let model = snapshot.document();
        (
            model.glue(&model.glue_order()[0]).unwrap().clone(),
            host,
            session,
        )
    });
    let glue_label = format!("glue-label-{}", glue.id);
    click(&mut h, &glue_label)?;
    h.capture()?;
    let version = host
        .read()
        .session_read(host.client_id(), session)?
        .version();
    {
        let client = host.client_id();
        let mut runtime = host.write();
        let draft = runtime.begin_draft(client, version)?;
        let (draft, ()) = runtime.batch(client, draft, |edit| {
            edit.set_vertex_positions(
                &b.id,
                &[(
                    glue.pairs[0].vertex_b.unwrap(),
                    Vec2::new(joint.x + 8., joint.y),
                )],
            )
        })?;
        runtime.publish(client, draft, "Move peer endpoint")?;
    }
    update(&mut h, |view, _, cx| -> anyhow::Result<()> {
        view.editor.sync_runtime()?;
        view.editor.execute(EditorCommand::SetGlueBrushSettings(
            BrushSelectionSettings {
                diameter: 96.,
                weight: 1.,
                ..Default::default()
            },
        ))?;
        view.redraw(cx);
        Ok(())
    })??;
    h.capture()?;
    click(&mut h, "brush-selection-more-button")?;
    save(&h.capture()?, directory, "glue-advanced-controls.png")?;
    click(&mut h, "glue-mode-a")?;
    click(&mut h, "brush-selection-more-button")?;
    click_model(
        &mut h,
        ModelPoint {
            x: joint.x + 4.,
            ..joint
        },
    )?;
    save(&h.capture()?, directory, "glue-02-separated.png")?;
    assert_eq!(
        h.view
            .read_with(&h.cx, |view, _| view.editor.overlay().glue_weights[0]
                .weights),
        [0., 0.5]
    );

    click(&mut h, "brush-selection-more-button")?;
    h.capture()?;
    click(&mut h, "glue-mode-reattach")?;
    click(&mut h, "brush-selection-more-button")?;
    click_model(
        &mut h,
        ModelPoint {
            x: joint.x + 2.,
            ..joint
        },
    )?;
    save(&h.capture()?, directory, "glue-03-reattached.png")?;
    assert_eq!(
        h.view
            .read_with(&h.cx, |view, _| view.editor.overlay().glue_weights[0]
                .weights),
        [0., 1.]
    );

    update(&mut h, |view, _, cx| -> anyhow::Result<()> {
        view.editor.execute(EditorCommand::SetObjectsLocked {
            objects: vec![b],
            locked: true,
        })?;
        view.redraw(cx);
        Ok(())
    })??;
    save(&h.capture()?, directory, "glue-04-locked.png")?;
    assert!(
        h.view
            .read_with(&h.cx, |view, _| view.editor.overlay().glue_labels[0].locked)
    );
    Ok(())
}

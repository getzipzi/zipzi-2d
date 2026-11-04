use super::*;
use zipzi_sdk::{DeletionIntent, DeletionReviewState};

fn large_fixture() -> AuthoringSession {
    let mut s = fixture();
    s.edit("large unrelated record", None, |e| {
        let mut mesh = e.candidate_document().get_mesh(&id(12)).unwrap().clone();
        mesh.id = id(70);
        mesh.runtime_id = id(70);
        mesh.name = "x".repeat(12 * 1024 * 1024);
        // Rebuilding an unrelated vertex index must not consume the deletion
        // budget either. These unused vertices are valid authored geometry.
        for vertex in 100..120_100 {
            mesh.vertex_ids.push(vertex);
            mesh.base_positions.push(Vec2::default());
            mesh.uvs.push(Vec2::default());
        }
        e.create_mesh(mesh)?;
        e.create_blend_binding(BlendShapeBinding {
            id: id(60),
            target_id: id(70),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: String::new(),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Mesh(vec![]),
        })
    })
    .unwrap();
    s.set_authoring_limits(AuthoringLimits {
        max_conversion_bytes: 1024 * 1024,
        ..Default::default()
    })
    .unwrap();
    s
}

#[test]
fn small_deletion_shares_unrelated_records_through_every_sdk_entry_and_history() {
    let mut s = large_fixture();
    let source = s.document().fork_candidate();
    assert!(source.estimated_content_bytes() > AuthoringLimits::default().max_conversion_bytes / 6);
    let request = request(&[10]);
    assert!(
        s.analyze_deletion(&request)
            .unwrap()
            .export_analysis_complete
    );
    let snapshot = s.read_snapshot();
    let plan = snapshot
        .plan_deletion_with_control(
            &request,
            &DeletionOptions::default(),
            s.authoring_limits(),
            &Default::default(),
        )
        .unwrap();
    assert!(plan.cost().temporary_bytes < s.authoring_limits().max_conversion_bytes);
    let review = snapshot
        .prepare_deletion_review(
            DeletionIntent {
                request: request.clone(),
                ..Default::default()
            },
            s.authoring_limits(),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(
        review.summary().state,
        DeletionReviewState::NeedsConfirmation
    );
    let mut edit = s.begin_owned_edit("delete", None).unwrap();
    assert!(
        edit.analyze_deletion(&request)
            .unwrap()
            .export_analysis_complete
    );
    let plan = edit
        .plan_deletion(&request, &DeletionOptions::default())
        .unwrap();
    edit.apply_deletion_plan(&plan).unwrap();
    {
        let document = edit.candidate_document();
        assert!(std::ptr::eq(
            source.get_mesh(&id(70)).unwrap(),
            document.get_mesh(&id(70)).unwrap()
        ));
        assert!(std::ptr::eq(
            source.get_binding(&id(22)).unwrap(),
            document.get_binding(&id(22)).unwrap()
        ));
        assert!(std::ptr::eq(
            source.get_scene_binding(&id(5)).unwrap(),
            document.get_scene_binding(&id(5)).unwrap()
        ));
        assert!(std::ptr::eq(
            source.get_blend_binding(&id(60)).unwrap(),
            document.get_blend_binding(&id(60)).unwrap()
        ));
    }
    edit.commit_to(&mut s).unwrap();
    assert!(s.document().get_mesh(&id(10)).is_none());
    assert!(std::ptr::eq(
        source.get_mesh(&id(70)).unwrap(),
        s.document().get_mesh(&id(70)).unwrap()
    ));
    s.undo().unwrap();
    assert!(s.document().same_content(&source));
    s.redo().unwrap();
    assert!(s.document().get_mesh(&id(10)).is_none());
    s.undo().unwrap();
    let review = s
        .read_snapshot()
        .prepare_deletion_review(
            DeletionIntent {
                request,
                ..Default::default()
            },
            s.authoring_limits(),
            &Default::default(),
        )
        .unwrap();
    s.edit("review route", None, |e| review.apply(e)).unwrap();
    assert!(s.document().get_mesh(&id(10)).is_none());
}

#[test]
fn deletion_budgets_charge_rewrites_reports_and_history_without_changing_source_or_redo() {
    let mut s = large_fixture();
    s.edit("redo seed", None, |e| e.rename_mesh(&id(11), "redo"))
        .unwrap();
    s.undo().unwrap();
    let source = s.document().fork_candidate();
    let history = s.history_state();
    let snapshot = s.read_snapshot();
    let intent = || DeletionIntent {
        request: request(&[10]),
        ..Default::default()
    };
    let review = snapshot
        .prepare_deletion_review(intent(), s.authoring_limits(), &Default::default())
        .unwrap();
    for (limits, expected) in [
        (
            AuthoringLimits {
                max_conversion_bytes: review.cost().temporary_bytes - 1,
                ..s.authoring_limits()
            },
            "AUTHORING_MEMORY_LIMIT",
        ),
        (
            AuthoringLimits {
                max_history_bytes: review.cost().history_bytes - 1,
                ..s.authoring_limits()
            },
            "AUTHORING_HISTORY_LIMIT",
        ),
    ] {
        assert_eq!(
            snapshot
                .prepare_deletion_review(intent(), limits, &Default::default())
                .unwrap_err()
                .code
                .as_ref(),
            expected
        );
    }
    // Deleting the texture rewrites the large retained mesh and must fail a
    // small workspace budget even though deleting another mesh succeeds.
    let mut texture = DeletionRequest {
        roots: vec![DeletionTarget::Texture(id(2))],
        ..Default::default()
    };
    texture.texture_replacements.insert(id(2), None);
    assert_eq!(
        s.plan_deletion(&texture, &DeletionOptions::default())
            .unwrap_err()
            .code
            .as_ref(),
        "AUTHORING_MEMORY_LIMIT"
    );
    assert!(s.document().same_content(&source));
    assert_eq!(s.history_state(), history);
    lock(&mut s, &id(11));
    let blocked = s
        .read_snapshot()
        .prepare_deletion_review(intent(), s.authoring_limits(), &Default::default())
        .unwrap();
    assert_eq!(blocked.summary().state, DeletionReviewState::Blocked);
    assert!(blocked.cost().temporary_bytes < s.authoring_limits().max_conversion_bytes);
}

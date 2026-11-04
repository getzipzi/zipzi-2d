use crate::support;
use zipzi_editor_core::{
    Document, ObjectKind as K, ObjectRef, PaletteMove, PalettePosition as P,
    PalettePreservation as S, PaletteProjection as V, TreeNode,
};
use zipzi_sdk::{ConversionControl, ReparentStrategy};
fn object(n: u128) -> ObjectRef {
    ObjectRef::new(K::Deformer, uuid::Uuid::from_u128(n).to_string())
}
fn request(d: &Document, target: Option<ObjectRef>, position: P) -> PaletteMove {
    PaletteMove {
        projection: V::Deformation,
        sources: vec![ObjectRef::new(K::Mesh, &d.layers()[0].id)],
        target,
        position,
    }
}
fn find<'a>(tree: &'a [TreeNode], id: &str) -> Option<&'a TreeNode> {
    tree.iter().find_map(|n| {
        if n.id == id {
            Some(n)
        } else {
            find(&n.children, id)
        }
    })
}

#[test]
fn preservation_review_applies_exact_order_atomically_and_keeps_organization_and_inspection()
-> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_for_reparent(false)?;
    let source = d.layers()[0].id.clone();
    let target = ObjectRef::new(K::Mesh, &d.layers()[1].id);
    let organization = d.part_tree();
    let selected = d.context().selections.clone();
    let inspected = d.context().inspected.clone();
    let history = d.history_lengths();
    let r = request(&d, Some(target.clone()), P::Before);
    let preflight = d.plan_palette_move(r.clone())?;
    assert!(!preflight.is_noop());
    assert_eq!(d.history_lengths(), history);
    let plan = d.capture_palette_reparent()?.plan_with_tolerance(
        r,
        S::ParameterDomain,
        0.001,
        1e-6,
        &ConversionControl::default(),
    )?;
    assert_eq!(
        plan.scene()
            .conversion_report()
            .unwrap()
            .reparent_guarantee
            .as_ref()
            .unwrap()
            .strategy,
        ReparentStrategy::PreserveParameterDomain
    );
    d.execute_palette_reparent(&plan)?;
    assert_eq!(
        find(&d.deformer_tree(), &object(502).id)
            .unwrap()
            .children
            .iter()
            .map(|n| n.id.clone())
            .collect::<Vec<_>>(),
        [source.clone(), target.id.clone()]
    );
    assert_eq!(d.part_tree(), organization);
    assert_eq!(d.context().inspected, inspected);
    assert_eq!(d.context().selections, selected);
    assert_eq!(d.history_lengths(), (history.0 + 1, 0));
    d.undo()?;
    assert_eq!(d.history_lengths(), (history.0, 1));
    let plan = d.capture_palette_reparent()?.plan_with_tolerance(
        request(&d, Some(target.clone()), P::After),
        S::ParameterDomain,
        0.001,
        1e-6,
        &ConversionControl::default(),
    )?;
    d.execute_palette_reparent(&plan)?;
    assert_eq!(
        find(&d.deformer_tree(), &object(502).id)
            .unwrap()
            .children
            .iter()
            .map(|n| n.id.clone())
            .collect::<Vec<_>>(),
        [target.id, source.clone()]
    );
    let plan = d.capture_palette_reparent()?.plan_with_tolerance(
        request(&d, None, P::Inside),
        S::ParameterDomain,
        0.001,
        1e-6,
        &ConversionControl::default(),
    )?;
    d.execute_palette_reparent(&plan)?;
    assert!(find(&d.deformer_tree(), &source).is_some());
    d.undo()?;
    d.redo()?;
    Ok(())
}
#[test]
fn driven_parent_requires_explicit_edit_state_scope() -> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_for_reparent(true)?;
    let r = request(&d, Some(object(502)), P::Inside);
    let history = d.history_lengths();
    assert!(d.plan_palette_move(r.clone()).is_err());
    assert_eq!(d.history_lengths(), history);
    let capture = d.capture_palette_reparent()?;
    let control = ConversionControl::default();
    let error = capture
        .plan_with_tolerance(r.clone(), S::ParameterDomain, 0.001, 1e-6, &control)
        .unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<zipzi_sdk::SdkError>()
            .unwrap()
            .code
            .as_ref(),
        "PARAMETER_DOMAIN_UNPROVEN"
    );
    let plan = capture.plan_with_tolerance(r, S::EditState, 0.001, 1e-6, &control)?;
    let g = plan
        .scene()
        .conversion_report()
        .unwrap()
        .reparent_guarantee
        .as_ref()
        .unwrap();
    assert_eq!(g.strategy, ReparentStrategy::PreserveEditState);
    assert!(g.context.as_ref().unwrap().parameter_domain.is_empty());
    assert_eq!(
        g.context.as_ref().unwrap().parameter_values[&uuid::Uuid::from_u128(701).to_string()],
        0.5
    );
    d.execute_palette_reparent(&plan)?;
    Ok(())
}
#[test]
fn changed_frozen_values_and_stale_document_reject_reviewed_plans_without_consuming_redo()
-> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_for_reparent(false)?;
    let r = request(&d, None, P::Inside);
    let capture = d.capture_palette_reparent()?;
    let plan = capture.plan_with_tolerance(
        r.clone(),
        S::ParameterDomain,
        0.001,
        1e-6,
        &ConversionControl::default(),
    )?;
    let history = d.history_lengths();
    d.set_preview_parameter(&uuid::Uuid::from_u128(701).to_string(), 0.7)?;
    assert!(d.execute_palette_reparent(&plan).is_err());
    assert_eq!(d.history_lengths(), history);
    let plan = d.capture_palette_reparent()?.plan_with_tolerance(
        r.clone(),
        S::ParameterDomain,
        0.001,
        1e-6,
        &ConversionControl::default(),
    )?;
    d.execute_palette_reparent(&plan)?;
    d.undo()?;
    let history = d.history_lengths();
    assert!(d.execute_palette_reparent(&plan).is_err());
    assert_eq!(d.history_lengths(), history);
    let control = ConversionControl::default();
    let plan = d.capture_palette_reparent()?.plan_with_tolerance(
        r,
        S::ParameterDomain,
        0.001,
        1e-6,
        &control,
    )?;
    control.cancel();
    assert!(d.execute_palette_reparent(&plan).is_err());
    assert_eq!(d.history_lengths(), history);
    d.redo()?;
    Ok(())
}
#[test]
fn preservation_converts_rotation_subtrees_and_checks_cycles_and_locks() -> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_for_reparent(false)?;
    let source = ObjectRef::new(K::Mesh, &d.layers()[0].id);
    d.set_objects_locked(std::slice::from_ref(&object(502)), true)?;
    let e = d
        .plan_palette_move(request(&d, Some(object(502)), P::Inside))
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<zipzi_sdk::SdkError>()
            .unwrap()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    d.set_objects_locked(std::slice::from_ref(&object(502)), false)?;
    let plan = d.capture_palette_reparent()?.plan_with_tolerance(
        PaletteMove {
            sources: vec![object(501)],
            ..request(&d, Some(object(502)), P::Inside)
        },
        S::EditState,
        0.01,
        1e-6,
        &ConversionControl::default(),
    )?;
    d.execute_palette_reparent(&plan)?;
    d.undo()?;
    let e = d
        .plan_palette_move(PaletteMove {
            sources: vec![object(501)],
            ..request(&d, Some(object(501)), P::Inside)
        })
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<zipzi_sdk::SdkError>()
            .unwrap()
            .code
            .as_ref(),
        "RELATION_CYCLE"
    );
    let warp =
        d.plan_scene_object_creation(zipzi_editor_core::SceneObjectType::Warp, "Warp".into(), &[])?;
    let warp = d.execute_scene_object_creation(&warp)?.suggested_selection[0].clone();
    let plan = d.capture_palette_reparent()?.plan_with_tolerance(
        PaletteMove {
            sources: vec![source],
            ..request(&d, Some(warp), P::Inside)
        },
        S::EditState,
        0.01,
        1e-6,
        &ConversionControl::default(),
    )?;
    d.execute_palette_reparent(&plan)?;
    Ok(())
}

#[test]
fn whole_rotation_and_mixed_parent_moves_preserve_child_meshes_and_roundtrip() -> anyhow::Result<()>
{
    let (mut d, _assets) = support::sample_document_for_reparent(false)?;
    let source = ObjectRef::new(K::Mesh, &d.layers()[0].id);
    let create =
        d.plan_scene_object_creation(zipzi_editor_core::SceneObjectType::Warp, "Warp".into(), &[])?;
    let warp = d
        .execute_scene_object_creation(&create)?
        .suggested_selection[0]
        .clone();
    let tree = d.deformer_tree();
    let organization = d.part_tree();
    let original = d.snapshot()?.runtime_frame;
    let properties = d.object_index().get(&source).unwrap().properties.clone();
    let history = d.history_lengths();
    for (root, target) in [(object(501), object(502)), (object(502), warp)] {
        let move_request = PaletteMove {
            sources: vec![source.clone(), root.clone()],
            ..request(&d, Some(target.clone()), P::Inside)
        };
        assert_eq!(
            d.palette_deformation_move_support(&move_request)?,
            Some(zipzi_sdk::ReparentSupport::RequiresConversionPlan)
        );
        let plan = d.plan_palette_move(move_request)?;
        d.execute_palette_move(&plan)?;
        assert_eq!(
            d.object_index().get(&root).unwrap().properties["parent_id"],
            target.id
        );
        assert_eq!(d.part_tree(), organization);
        assert_eq!(
            d.object_index().get(&source).unwrap().properties,
            properties
        );
        let after = d.snapshot()?.runtime_frame;
        for before in &original.drawables {
            let after = after.drawables.iter().find(|a| a.id == before.id).unwrap();
            assert_eq!(before.uvs, after.uvs);
            assert_eq!(before.visible, after.visible);
            for (a, b) in before.positions.iter().zip(&after.positions) {
                assert!((a.x - b.x).hypot(a.y - b.y) < 0.001, "{a:?} != {b:?}");
            }
        }
    }
    assert_eq!(d.history_lengths(), (history.0 + 2, 0));
    d.undo()?;
    d.undo()?;
    assert_eq!(d.deformer_tree(), tree);
    d.redo()?;
    d.redo()?;
    let directory = tempfile::tempdir()?;
    let saved = d.save(&directory.path().join("rotation-parent.zipzi"))?;
    let mut reopened = Document::open(&saved.path)?;
    assert_eq!(reopened.deformer_tree(), d.deformer_tree());
    assert_eq!(
        reopened.snapshot()?.runtime_frame.drawables,
        d.snapshot()?.runtime_frame.drawables
    );
    Ok(())
}

#[test]
fn same_parent_deformer_reordering_still_normalizes_selected_descendants() -> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_for_reparent(false)?;
    let source = ObjectRef::new(K::Mesh, &d.layers()[0].id);
    let original = d.snapshot()?.runtime_frame;
    let plan = d.plan_palette_move(PaletteMove {
        sources: vec![source.clone(), object(501)],
        ..request(&d, Some(object(502)), P::After)
    })?;
    d.execute_palette_move(&plan)?;
    assert_eq!(
        find(&d.deformer_tree(), &object(501).id).unwrap().children[0].id,
        source.id
    );
    assert_eq!(d.snapshot()?.runtime_frame.drawables, original.drawables);
    assert_eq!(d.history_lengths(), (1, 0));
    d.undo()?;
    d.redo()?;
    Ok(())
}

#[test]
fn whole_rotation_tree_move_compensates_driven_parent_at_current_pose_and_expires_on_preview_change()
-> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_for_reparent(true)?;
    let source = ObjectRef::new(K::Mesh, &d.layers()[0].id);
    let move_request = PaletteMove {
        sources: vec![object(501)],
        ..request(&d, Some(object(502)), P::Inside)
    };
    assert!(d.plan_palette_move(move_request.clone()).is_err());
    let old = d.capture_palette_reparent()?.plan(
        move_request.clone(),
        S::EditState,
        &ConversionControl::default(),
    )?;
    let history = d.history_lengths();
    d.set_preview_parameter(&uuid::Uuid::from_u128(701).to_string(), 0.7)?;
    assert!(d.execute_palette_reparent(&old).is_err());
    assert_eq!(d.history_lengths(), history);
    let before = d.snapshot()?.runtime_frame;
    let plan = d.capture_palette_reparent()?.plan(
        move_request,
        S::EditState,
        &ConversionControl::default(),
    )?;
    d.execute_palette_reparent(&plan)?;
    let after = d.snapshot()?.runtime_frame;
    for before in &before.drawables {
        let after = after.drawables.iter().find(|a| a.id == before.id).unwrap();
        for (a, b) in before.positions.iter().zip(&after.positions) {
            assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
        }
    }
    let original = after
        .drawables
        .iter()
        .find(|a| a.id == source.id)
        .unwrap()
        .positions
        .clone();
    d.set_preview_parameter(&uuid::Uuid::from_u128(701).to_string(), 0.9)?;
    let next = d.snapshot()?.runtime_frame;
    let next = next.drawables.iter().find(|a| a.id == source.id).unwrap();
    assert!(
        original
            .iter()
            .zip(&next.positions)
            .any(|(a, b)| (a.x - b.x).hypot(a.y - b.y) > 0.1)
    );
    d.undo()?;
    d.redo()?;
    Ok(())
}

#[test]
fn default_mesh_tree_moves_into_and_out_of_warp_keep_visible_geometry_and_undo()
-> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_for_reparent(false)?;
    let sources: Vec<_> = d
        .layers()
        .iter()
        .map(|l| ObjectRef::new(K::Mesh, &l.id))
        .collect();
    let create =
        d.plan_scene_object_creation(zipzi_editor_core::SceneObjectType::Warp, "Warp".into(), &[])?;
    let warp = d
        .execute_scene_object_creation(&create)?
        .suggested_selection[0]
        .clone();
    let original = d.snapshot()?.runtime_frame;
    let organization = d.part_tree();
    let history = d.history_lengths();
    let into = PaletteMove {
        sources: sources.clone(),
        ..request(&d, Some(warp.clone()), P::Inside)
    };
    assert_eq!(
        d.palette_deformation_move_support(&into)?,
        Some(zipzi_sdk::ReparentSupport::RequiresConversionPlan)
    );
    assert_eq!(d.history_lengths(), history);
    let plan = d.plan_palette_move(into)?;
    d.execute_palette_move(&plan)?;
    assert_eq!(d.history_lengths(), (history.0 + 1, 0));
    assert_eq!(d.part_tree(), organization);
    for source in &sources {
        assert_eq!(
            d.object_index().get(source).unwrap().properties["deformer_id"],
            warp.id
        );
    }
    for next in [None, Some(warp)] {
        let plan = d.plan_palette_move(PaletteMove {
            sources: sources.clone(),
            ..request(&d, next, P::Inside)
        })?;
        d.execute_palette_move(&plan)?;
        let after = d.snapshot()?.runtime_frame;
        for before in &original.drawables {
            let after = after.drawables.iter().find(|a| a.id == before.id).unwrap();
            assert_eq!(before.uvs, after.uvs);
            assert_eq!(before.visible, after.visible);
            for (a, b) in before.positions.iter().zip(&after.positions) {
                assert!((a.x - b.x).hypot(a.y - b.y) < 0.001, "{a:?} != {b:?}");
            }
        }
    }
    d.undo()?;
    d.redo()?;
    let directory = tempfile::tempdir()?;
    let saved = d.save(&directory.path().join("warp-parent.zipzi"))?;
    let mut reopened = Document::open(&saved.path)?;
    assert_eq!(reopened.deformer_tree(), d.deformer_tree());
    assert_eq!(
        reopened.snapshot()?.runtime_frame.drawables,
        d.snapshot()?.runtime_frame.drawables
    );
    Ok(())
}

#[test]
fn default_tolerance_matches_deletion_and_explicit_tolerance_is_never_relaxed() -> anyhow::Result<()>
{
    let (d, _assets) = support::sample_document_for_reparent(false)?;
    let r = request(&d, Some(object(502)), P::Inside);
    let capture = d.capture_palette_reparent()?;
    let default = capture.plan(r.clone(), S::EditState, &ConversionControl::default())?;
    assert_eq!(
        default
            .scene()
            .conversion_report()
            .unwrap()
            .reparent_guarantee
            .as_ref()
            .unwrap()
            .tolerance,
        0.001
    );
    let error = capture
        .plan_with_tolerance(
            r.clone(),
            S::EditState,
            1e-15,
            0.,
            &ConversionControl::default(),
        )
        .unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<zipzi_sdk::SdkError>()
            .unwrap()
            .code
            .as_ref(),
        "REPARENT_TOLERANCE_EXCEEDED"
    );
    let plan =
        capture.plan_with_tolerance(r, S::EditState, 0.001, 1e-6, &ConversionControl::default())?;
    assert_eq!(
        plan.scene()
            .conversion_report()
            .unwrap()
            .reparent_guarantee
            .as_ref()
            .unwrap()
            .tolerance,
        0.001
    );
    assert_eq!(d.history_lengths(), (0, 0));
    Ok(())
}

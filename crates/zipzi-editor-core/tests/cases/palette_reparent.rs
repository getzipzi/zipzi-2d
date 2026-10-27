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
    assert!(!d.plan_palette_move(r.clone())?.is_noop());
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
fn preservation_rejects_unproved_subtrees_and_warp_while_moves_check_cycles_and_locks()
-> anyhow::Result<()> {
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
    let e = d
        .capture_palette_reparent()?
        .plan(
            PaletteMove {
                sources: vec![object(501)],
                ..request(&d, Some(object(502)), P::Inside)
            },
            S::EditState,
            &ConversionControl::default(),
        )
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<zipzi_sdk::SdkError>()
            .unwrap()
            .code
            .as_ref(),
        "SUBTREE_CONVERSION_UNPROVEN"
    );
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
    let e = d
        .capture_palette_reparent()?
        .plan(
            PaletteMove {
                sources: vec![source],
                ..request(&d, Some(warp), P::Inside)
            },
            S::EditState,
            &ConversionControl::default(),
        )
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<zipzi_sdk::SdkError>()
            .unwrap()
            .code
            .as_ref(),
        "NONLINEAR_PARENT_UNPROVEN"
    );
    Ok(())
}

#[test]
fn local_moves_accept_warp_and_deformer_subtrees_and_keep_order_and_undo() -> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_for_reparent(true)?;
    let source = ObjectRef::new(K::Mesh, &d.layers()[0].id);
    let sibling = ObjectRef::new(K::Mesh, &d.layers()[1].id);
    let create =
        d.plan_scene_object_creation(zipzi_editor_core::SceneObjectType::Warp, "Warp".into(), &[])?;
    let warp = d
        .execute_scene_object_creation(&create)?
        .suggested_selection[0]
        .clone();
    let original_tree = d.deformer_tree();
    let original_properties = d.object_index().get(&source).unwrap().properties.clone();
    let organization = d.part_tree();
    let selection = d.context().selections.clone();
    let inspected = d.context().inspected.clone();
    let history = d.history_lengths();
    // Selecting a parent and its child moves only the parent subtree.
    let plan = d.plan_palette_move(PaletteMove {
        sources: vec![source.clone(), object(501)],
        ..request(&d, Some(warp.clone()), P::Inside)
    })?;
    d.execute_palette_move(&plan)?;
    assert_eq!(d.history_lengths(), (history.0 + 1, 0));
    let tree = d.deformer_tree();
    assert_eq!(
        find(&tree, &warp.id).unwrap().children[0].id,
        object(501).id
    );
    assert_eq!(
        find(&tree, &object(501).id).unwrap().children[0].id,
        source.id
    );
    assert_eq!(
        d.object_index().get(&source).unwrap().properties,
        original_properties
    );
    assert_eq!(d.part_tree(), organization);
    assert_eq!(d.context().selections, selection);
    assert_eq!(d.context().inspected, inspected);
    // A descendant cannot become the parent of its ancestor.
    assert!(
        d.plan_palette_move(PaletteMove {
            sources: vec![warp.clone()],
            ..request(&d, Some(object(501)), P::Inside)
        })
        .is_err()
    );
    d.undo()?;
    assert_eq!(d.deformer_tree(), original_tree);
    assert!(d.execute_palette_move(&plan).is_err());
    d.redo()?;
    // Move a mesh out of the warp chain and preserve an exact sibling anchor.
    let plan = d.plan_palette_move(request(&d, Some(sibling.clone()), P::Before))?;
    d.execute_palette_move(&plan)?;
    assert_eq!(
        find(&d.deformer_tree(), &object(502).id)
            .unwrap()
            .children
            .iter()
            .map(|n| n.id.clone())
            .collect::<Vec<_>>(),
        [source.id.clone(), sibling.id]
    );
    let plan = d.plan_palette_move(request(&d, Some(warp.clone()), P::Inside))?;
    d.execute_palette_move(&plan)?;
    assert_eq!(
        d.object_index().get(&source).unwrap().properties["deformer_id"],
        warp.id
    );
    let plan = d.plan_palette_move(request(&d, None, P::Inside))?;
    d.execute_palette_move(&plan)?;
    assert_eq!(
        d.object_index().get(&source).unwrap().properties["deformer_id"],
        ""
    );
    Ok(())
}

#[test]
fn default_tolerance_is_never_relaxed_and_explicit_tolerance_is_in_the_review() -> anyhow::Result<()>
{
    let (d, _assets) = support::sample_document_for_reparent(false)?;
    let r = request(&d, Some(object(502)), P::Inside);
    let capture = d.capture_palette_reparent()?;
    let error = capture
        .plan(r.clone(), S::EditState, &ConversionControl::default())
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

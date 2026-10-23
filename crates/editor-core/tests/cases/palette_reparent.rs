use crate::support;
use editor_core::{
    Document, ObjectKind as K, ObjectRef, PaletteMove, PalettePosition as P,
    PalettePreservation as S, PaletteProjection as V, TreeNode,
};
use kasane_sdk::{ConversionControl, ReparentStrategy};
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
    let organization = d.mesh_tree();
    let selected = d.context().selections.clone();
    let inspected = d.context().inspected.clone();
    let history = d.history_lengths();
    let r = request(&d, Some(target.clone()), P::Before);
    let preflight = d.plan_palette_move(r.clone())?;
    assert!(preflight.requires_preservation());
    assert!(!preflight.is_noop());
    assert!(d.execute_palette_move(&preflight).is_err());
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
    assert_eq!(d.mesh_tree(), organization);
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
    assert!(d.plan_palette_move(r.clone())?.requires_preservation());
    let capture = d.capture_palette_reparent()?;
    let control = ConversionControl::default();
    let error = capture
        .plan_with_tolerance(r.clone(), S::ParameterDomain, 0.001, 1e-6, &control)
        .unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<kasane_sdk::SdkError>()
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
fn unsupported_subtrees_warp_cycles_and_locks_are_native_preflight_failures() -> anyhow::Result<()>
{
    let (mut d, _assets) = support::sample_document_for_reparent(false)?;
    let source = ObjectRef::new(K::Mesh, &d.layers()[0].id);
    d.set_objects_locked(std::slice::from_ref(&object(502)), true)?;
    let e = d
        .plan_palette_move(request(&d, Some(object(502)), P::Inside))
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<kasane_sdk::SdkError>()
            .unwrap()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    d.set_objects_locked(std::slice::from_ref(&object(502)), false)?;
    let e = d
        .plan_palette_move(PaletteMove {
            sources: vec![object(501)],
            ..request(&d, Some(object(502)), P::Inside)
        })
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<kasane_sdk::SdkError>()
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
        e.downcast_ref::<kasane_sdk::SdkError>()
            .unwrap()
            .code
            .as_ref(),
        "RELATION_CYCLE"
    );
    let warp =
        d.plan_scene_object_creation(editor_core::SceneObjectType::Warp, "Warp".into(), &[])?;
    let warp = d.execute_scene_object_creation(&warp)?.suggested_selection[0].clone();
    let e = d
        .plan_palette_move(PaletteMove {
            sources: vec![source],
            ..request(&d, Some(warp), P::Inside)
        })
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<kasane_sdk::SdkError>()
            .unwrap()
            .code
            .as_ref(),
        "NONLINEAR_PARENT_UNPROVEN"
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
            .downcast_ref::<kasane_sdk::SdkError>()
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

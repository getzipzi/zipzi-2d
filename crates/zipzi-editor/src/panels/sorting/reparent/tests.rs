use super::{
    AuthoringContext, DropPosition, Duration, ObjectKind, ObjectRef, PaletteMove,
    PaletteProjection, ReparentReview, Scope,
};
use crate::headless_tests::{Harness, document_fixture, setup_with_document};
use gpui_kit::component::WindowExt;
use gpui_kit::{AppContext as _, Entity};
use gpui_kit::{
    MouseButton, TestAppContext,
    test::{ClickOptions, TestWindowExt},
};
fn object(n: u128) -> ObjectRef {
    ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(n).to_string())
}
fn setup(cx: &mut TestAppContext, driven: bool) -> (Harness, Vec<ObjectRef>) {
    let (d, assets) = document_fixture::sample_document_for_reparent(driven).unwrap();
    let sources = d
        .layers()
        .iter()
        .map(|l| ObjectRef::new(ObjectKind::Mesh, &l.id))
        .collect();
    let h = setup_with_document(cx, d, assets);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.dock.as_ref().unwrap().show(
                zipzi_editor_core::PanelKind::Deformers,
                None,
                window,
                cx,
            )
        });
    })
    .unwrap();
    cx.run_until_parked();
    (h, sources)
}
fn open(
    h: &Harness,
    sources: Vec<ObjectRef>,
    fixed: Option<PaletteMove>,
    cx: &mut TestAppContext,
) -> Entity<ReparentReview> {
    let v = cx
        .update_window(h.window, |_, window, cx| {
            let state = h.view.read(cx);
            ReparentReview::open(
                h.view.downgrade(),
                state.editor.active_id().unwrap(),
                AuthoringContext::capture(&state.editor).unwrap(),
                sources,
                fixed,
                window,
                cx,
            )
            .unwrap()
        })
        .unwrap();
    cx.update_window(h.window, |_, window, cx| {
        assert_eq!(v.read(cx).scope, Scope::EditState);
        assert_eq!(v.read(cx).tolerance[0].read(cx).value(), "0.00001");
        window.render_frame(cx);
        window.click("reparent-absolute", cx);
        window.press("cmd-a", cx);
        window.input("0.001", cx);
    })
    .unwrap();
    settle(cx);
    v
}
fn request(source: ObjectRef, target: Option<ObjectRef>, position: DropPosition) -> PaletteMove {
    PaletteMove {
        projection: PaletteProjection::Deformation,
        sources: vec![source],
        target,
        position,
    }
}
fn settle(cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(160));
    cx.run_until_parked();
}
#[gpui_kit::test]
fn fixed_review_reports_native_proof_and_commits_once_with_selection_and_undo(
    cx: &mut TestAppContext,
) {
    let (h, objects) = setup(cx, false);
    let (history, selection, inspected, organization) = h.view.read_with(cx, |v, _| {
        let d = v.editor.document().unwrap();
        (
            d.history_lengths(),
            d.context().selections.clone(),
            d.context().inspected.clone(),
            d.part_tree(),
        )
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click("deformer-tree-search", cx);
        window.input("Source rotation", cx);
    })
    .unwrap();
    settle(cx);
    let v = open(
        &h,
        vec![objects[0].clone()],
        Some(request(
            objects[0].clone(),
            Some(objects[1].clone()),
            DropPosition::Before,
        )),
        cx,
    );
    v.read_with(cx, |v, _| {
        assert!(v.plan.is_some(), "{:?}", v.reason);
        assert_eq!(v.plans_built.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(v.scope, Scope::EditState);
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click("reparent-domain", cx)
    })
    .unwrap();
    settle(cx);
    v.read_with(cx, |v, _| {
        assert_eq!(
            v.plan
                .as_ref()
                .unwrap()
                .scene()
                .conversion_report()
                .unwrap()
                .reparent_guarantee
                .as_ref()
                .unwrap()
                .strategy,
            zipzi_sdk::ReparentStrategy::PreserveParameterDomain
        );
    });
    cx.update_window(h.window, |_, window, cx| {
        assert!(window.find("reparent-report").visible());
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            history
        );
        window.click("reparent-submit", cx);
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |v, _| {
        let d = v.editor.document().unwrap();
        assert_eq!(d.history_lengths(), (history.0 + 1, 0));
        assert_eq!(d.context().selections, selection);
        assert_eq!(d.context().inspected, inspected);
        assert_eq!(d.part_tree(), organization);
        assert_eq!(
            d.deformer_tree()
                .iter()
                .find(|n| n.id == object(502).id)
                .unwrap()
                .children
                .iter()
                .map(|n| n.id.clone())
                .collect::<Vec<_>>(),
            objects.iter().map(|o| o.id.clone()).collect::<Vec<_>>()
        );
    });
    cx.update_window(h.window, |_, window, cx| {
        assert_eq!(
            window.find("deformer-tree-search").value(),
            Some("Source rotation")
        );
        assert_eq!(
            v.read(cx)
                .plans_built
                .load(std::sync::atomic::Ordering::Relaxed),
            2
        );
        assert!(!window.has_active_dialog(cx));
        assert!(
            window
                .find(format!("deformer-item-Mesh:{}", objects[0].id))
                .visible()
        );
        h.view.update(cx, |v, cx| {
            let r = v.editor.execute(zipzi_editor_core::EditorCommand::Undo);
            v.complete_edit(r, cx);
        });
    })
    .unwrap();
    assert_eq!(
        h.view
            .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths()),
        (history.0, 1)
    );
}
#[gpui_kit::test]
fn full_domain_failure_requires_explicit_scope_change_before_submit(cx: &mut TestAppContext) {
    let (h, objects) = setup(cx, true);
    let history = h
        .view
        .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths());
    let v = open(
        &h,
        vec![objects[0].clone()],
        Some(request(
            objects[0].clone(),
            Some(object(502)),
            DropPosition::Inside,
        )),
        cx,
    );
    v.read_with(cx, |v, _| {
        assert!(v.plan.is_some(), "{:?}", v.reason);
        assert_eq!(v.scope, Scope::EditState);
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click("reparent-domain", cx)
    })
    .unwrap();
    settle(cx);
    v.read_with(cx, |v, _| {
        assert!(v.plan.is_none());
        assert!(v.reason.as_ref().unwrap().contains("full parameter range"));
        assert_eq!(v.scope, Scope::ParameterDomain);
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click("reparent-submit", cx);
        assert!(window.has_active_dialog(cx));
        window.click("reparent-state", cx);
    })
    .unwrap();
    settle(cx);
    v.read_with(cx, |v, _| {
        assert!(v.plan.is_some(), "{:?}", v.reason);
        assert_eq!(v.scope, Scope::EditState);
        assert_eq!(v.plans_built.load(std::sync::atomic::Ordering::Relaxed), 3);
        assert_eq!(
            v.plan
                .as_ref()
                .unwrap()
                .scene()
                .conversion_report()
                .unwrap()
                .reparent_guarantee
                .as_ref()
                .unwrap()
                .strategy,
            zipzi_sdk::ReparentStrategy::PreserveEditState
        );
    });
    assert_eq!(
        h.view
            .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths()),
        history
    );
    cx.update_window(h.window, |_, window, cx| {
        window.click("reparent-submit", cx)
    })
    .unwrap();
    settle(cx);
    assert_eq!(
        h.view
            .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths()),
        (history.0 + 1, 0)
    );
}
#[gpui_kit::test]
fn destination_search_and_paint_never_plan_until_chosen_and_stale_preview_cancels(
    cx: &mut TestAppContext,
) {
    let (h, objects) = setup(cx, false);
    let v = open(&h, vec![objects[0].clone()], None, cx);
    assert_eq!(
        v.read_with(cx, |v, _| v
            .plans_built
            .load(std::sync::atomic::Ordering::Relaxed)),
        0
    );
    cx.update_window(h.window, |_, window, cx| {
        window.within("reparent-destination").click("input", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.input("Destination rotation", cx);
    })
    .unwrap();
    settle(cx);
    assert_eq!(
        v.read_with(cx, |v, _| v
            .plans_built
            .load(std::sync::atomic::Ordering::Relaxed)),
        0
    );
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    settle(cx);
    let control = v.read_with(cx, |v, _| {
        assert!(v.plan.is_some(), "{:?}", v.reason);
        assert_eq!(v.plans_built.load(std::sync::atomic::Ordering::Relaxed), 1);
        v.control.clone()
    });
    for _ in 0..5 {
        cx.update_window(h.window, |_, window, cx| window.render_frame(cx))
            .unwrap();
        settle(cx);
    }
    assert_eq!(
        v.read_with(cx, |v, _| v
            .plans_built
            .load(std::sync::atomic::Ordering::Relaxed)),
        1
    );
    let history = h
        .view
        .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths());
    h.view.update(cx, |v, cx| {
        v.editor
            .execute(zipzi_editor_core::EditorCommand::SetPreviewParameter {
                parameter_id: uuid::Uuid::from_u128(701).to_string(),
                value: 0.8,
            })
            .unwrap();
        v.redraw(cx);
    });
    settle(cx);
    assert!(control.is_cancelled());
    v.read_with(cx, |v, _| {
        assert!(v.plan.is_none());
        assert!(!v.working);
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click("reparent-submit", cx);
        assert!(window.has_active_dialog(cx));
        window.click("reparent-cancel", cx);
    })
    .unwrap();
    settle(cx);
    assert_eq!(
        h.view
            .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths()),
        history
    );
}
#[gpui_kit::test]
fn cancelling_pending_conversion_preserves_redo_and_discards_result(cx: &mut TestAppContext) {
    let (h, objects) = setup(cx, false);
    h.view.update(cx, |v, cx| {
        let p = v
            .editor
            .document()
            .unwrap()
            .plan_palette_steps(PaletteProjection::Deformation, &[object(502)], false)
            .unwrap();
        v.editor
            .execute_palette_move(v.editor.active_id().unwrap(), &p)
            .unwrap();
        v.editor
            .execute(zipzi_editor_core::EditorCommand::Undo)
            .unwrap();
        v.redraw(cx);
    });
    let history = h
        .view
        .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths());
    assert_eq!(history.1, 1);
    let v = cx
        .update_window(h.window, |_, window, cx| {
            let state = h.view.read(cx);
            let v = ReparentReview::open(
                h.view.downgrade(),
                state.editor.active_id().unwrap(),
                AuthoringContext::capture(&state.editor).unwrap(),
                vec![objects[0].clone()],
                Some(request(objects[0].clone(), None, DropPosition::Inside)),
                window,
                cx,
            )
            .unwrap();
            assert!(v.read(cx).working);
            window.render_frame(cx);
            window.click("reparent-cancel", cx);
            v
        })
        .unwrap();
    settle(cx);
    v.read_with(cx, |v, _| {
        assert!(v.control.is_cancelled());
        assert!(v.dialog.closed());
        assert!(v.plan.is_none());
    });
    assert_eq!(
        h.view
            .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths()),
        history
    );
}
#[gpui_kit::test]
fn deformation_root_drag_opens_review_then_moves_on_confirmation(cx: &mut TestAppContext) {
    let (h, objects) = setup(cx, false);
    let history = h
        .view
        .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths());
    cx.update_window(h.window, |_, window, cx| {
        let from = window
            .find(format!("deformer-item-Mesh:{}", objects[0].id))
            .bounds()
            .center();
        let to = window.find("deformer-tree-root-drop").bounds().center();
        window.drag(from, to, cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("reparent-absolute", cx);
        window.press("cmd-a", cx);
        window.input("0.001", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(window.has_active_dialog(cx));
        assert!(window.find("reparent-report").visible());
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            history
        );
        window.click("reparent-submit", cx);
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |v, _| {
        let d = v.editor.document().unwrap();
        assert_eq!(d.history_lengths(), (history.0 + 1, 0));
        assert!(
            d.deformer_tree()
                .iter()
                .flat_map(|n| n.children.iter())
                .all(|n| n.id != objects[0].id)
                || d.deformer_tree().iter().any(|n| n
                    .children
                    .iter()
                    .any(|n| n.id == objects[0].id)
                    && n.kind == zipzi_editor_core::TreeNodeKind::Group)
        );
    });
}
#[gpui_kit::test]
fn deformation_move_to_menu_opens_searchable_preservation_review(cx: &mut TestAppContext) {
    let (h, objects) = setup(cx, false);
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            format!("deformer-item-Mesh:{}", objects[0].id),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        )
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, cx| {
        let i = (0..24usize)
            .find(|i| {
                window
                    .within("popup-menu")
                    .try_find(*i)
                    .is_some_and(|e| e.label() == Some("Move to…"))
            })
            .expect("Move to menu");
        window.within("popup-menu").click(i, cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(window.has_active_dialog(cx));
        assert!(window.find("reparent-review").visible());
        window.click("reparent-cancel", cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn cross_parent_after_drag_reviews_and_keeps_the_requested_anchor(cx: &mut TestAppContext) {
    let (h, objects) = setup(cx, false);
    let history = h
        .view
        .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths());
    cx.update_window(h.window, |_, window, cx| {
        let from = window
            .find(format!("deformer-item-Mesh:{}", objects[0].id))
            .bounds()
            .center();
        let bounds = window
            .find(format!("deformer-item-Mesh:{}", objects[1].id))
            .bounds();
        window.drag(
            from,
            gpui_kit::point(bounds.center().x, bounds.bottom() - gpui_kit::px(2.)),
            cx,
        );
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(window.has_active_dialog(cx));
        window.click("reparent-absolute", cx);
        window.press("cmd-a", cx);
        window.input("0.001", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, cx| {
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            history
        );
        window.click("reparent-submit", cx);
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |v, _| {
        let d = v.editor.document().unwrap();
        assert_eq!(d.history_lengths(), (history.0 + 1, 0));
        assert_eq!(
            d.deformer_tree()
                .iter()
                .find(|n| n.id == object(502).id)
                .unwrap()
                .children
                .iter()
                .map(|n| n.id.clone())
                .collect::<Vec<_>>(),
            [objects[1].id.clone(), objects[0].id.clone()]
        );
    });
}

#[gpui_kit::test]
fn invalid_tolerance_cancels_the_old_worker_and_noop_move_preserves_redo(cx: &mut TestAppContext) {
    let (h, objects) = setup(cx, false);
    h.view.update(cx, |v, cx| {
        let p = v
            .editor
            .document()
            .unwrap()
            .plan_palette_steps(PaletteProjection::Deformation, &[object(502)], false)
            .unwrap();
        v.editor
            .execute_palette_move(v.editor.active_id().unwrap(), &p)
            .unwrap();
        v.editor
            .execute(zipzi_editor_core::EditorCommand::Undo)
            .unwrap();
        v.redraw(cx);
    });
    let history = h
        .view
        .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths());
    let v = open(
        &h,
        vec![objects[0].clone()],
        Some(request(
            objects[0].clone(),
            Some(object(501)),
            DropPosition::Inside,
        )),
        cx,
    );
    v.read_with(cx, |v, _| {
        assert!(v.plan.is_none());
        assert!(v.reason.as_ref().unwrap().contains("Already"));
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click("reparent-submit", cx);
        assert!(window.has_active_dialog(cx));
        window.click("reparent-cancel", cx);
    })
    .unwrap();
    settle(cx);
    assert_eq!(
        h.view
            .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths()),
        history
    );
    let v = open(
        &h,
        vec![objects[0].clone()],
        Some(request(
            objects[0].clone(),
            Some(object(502)),
            DropPosition::Inside,
        )),
        cx,
    );
    let control = v.read_with(cx, |v, _| v.control.clone());
    cx.update_window(h.window, |_, window, cx| {
        window.click("reparent-absolute", cx);
        window.press("cmd-a", cx);
        window.input("NaN", cx);
    })
    .unwrap();
    settle(cx);
    assert!(control.is_cancelled());
    v.read_with(cx, |v, _| {
        assert!(v.plan.is_none());
        assert!(!v.working);
        assert!(v.reason.as_ref().unwrap().contains("finite"));
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click("reparent-submit", cx);
        assert!(window.has_active_dialog(cx));
        window.press("escape", cx);
    })
    .unwrap();
    settle(cx);
    assert_eq!(
        h.view
            .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths()),
        history
    );
}

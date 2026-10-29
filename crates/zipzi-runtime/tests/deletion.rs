use zipzi_core::{Canvas, Vec2};
use zipzi_runtime::*;
use zipzi_sdk::{
    AuthoringSession, DeletionIntent, DeletionRequest, DeletionReviewState, DeletionRow,
    DeletionSection as S, DeletionTarget, ModelingIntent,
};
fn id(n: u32) -> String {
    format!("40000000-0000-4000-8000-{n:012x}")
}
fn fixture(limits: RuntimeLimits) -> (Runtime, ClientId, SessionId) {
    let mut sdk = AuthoringSession::new(&id(1), Canvas::default()).unwrap();
    sdk.edit("fixture", None, |e| {
        for n in 2..=3 {
            e.create_mesh(zipzi_sdk::rectangle_mesh(
                &id(n),
                "Mesh",
                "",
                Vec2::default(),
                Vec2::new(1., 1.),
            )?)?;
        }
        let mut mesh = e.candidate_document().get_mesh(&id(3)).unwrap().clone();
        mesh.masks = vec![id(2)];
        e.replace_mesh(mesh)
    })
    .unwrap();
    let mut rt = Runtime::new(limits);
    let c = rt.register_client().unwrap();
    let s = rt.adopt_session(c, sdk).unwrap();
    (rt, c, s)
}
fn intent() -> ModelingIntent {
    ModelingIntent::Deletion(DeletionIntent {
        request: DeletionRequest {
            roots: vec![DeletionTarget::Mesh(id(2))],
            ..Default::default()
        },
        ..Default::default()
    })
}
#[test]
fn pages_have_client_ownership_and_survive_staleness_until_release() {
    let (mut rt, c, s) = fixture(Default::default());
    let other = rt.register_client().unwrap();
    let before = rt.document(s).unwrap().clone();
    let (plan, source, report) = rt.plan_modeling(c, s, None, intent()).unwrap();
    assert_eq!(
        report.deletion.unwrap().state,
        DeletionReviewState::NeedsConfirmation
    );
    assert!(report.affected_ids.is_empty());
    assert_eq!(source.version(), rt.session(s).unwrap().version());
    let page = rt
        .deletion_report_page(c, plan, S::Rewritten, 0, 1)
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert!(
        matches!(&page.rows[0], DeletionRow::Relation { relation } if relation.owner.id == id(3))
    );
    assert!(matches!(
        rt.deletion_report_page(other, plan, S::Removed, 0, 1),
        Err(RuntimeError::WrongOwner)
    ));
    let edit = rt.begin_edit(c, s, "rename", None).unwrap();
    rt.apply_edit(c, edit, |e| e.rename_mesh(&id(3), "New"))
        .unwrap();
    rt.commit_edit(c, edit).unwrap();
    assert_eq!(
        serde_json::to_value(&page).unwrap(),
        serde_json::to_value(
            rt.deletion_report_page(c, plan, S::Rewritten, 0, 1)
                .unwrap()
        )
        .unwrap()
    );
    let edit = rt.begin_edit(c, s, "stale", None).unwrap();
    assert!(rt.apply_modeling_plan(c, edit, plan).is_err());
    rt.release_edit(c, edit).unwrap();
    assert!(rt.document(s).unwrap().get_mesh(&id(2)).is_some());
    rt.release_modeling_plan(c, plan).unwrap();
    assert!(rt.deletion_report_page(c, plan, S::Removed, 0, 1).is_err());
    assert!(!rt.document(s).unwrap().same_content(&before));
}
#[test]
fn frozen_job_reports_blockers_and_valid_plans_publish_one_undo_step() {
    let (mut rt, c, s) = fixture(Default::default());
    let history = rt.session(s).unwrap().history_lengths();
    let job = rt.request_modeling_plan(c, s, None, intent()).unwrap();
    let work = rt.take_work(job).unwrap();
    rt.complete(work.run());
    let JobValue::Planned(plan) = rt.job_info(c, job).unwrap().result.unwrap().unwrap() else {
        panic!("plan")
    };
    rt.release_job(c, job).unwrap();
    let edit = rt.begin_edit(c, s, "delete", None).unwrap();
    rt.apply_modeling_plan(c, edit, plan.plan).unwrap();
    rt.commit_edit(c, edit).unwrap();
    assert!(rt.document(s).unwrap().get_mesh(&id(2)).is_none());
    assert!(rt
        .document(s)
        .unwrap()
        .get_mesh(&id(3))
        .unwrap()
        .masks
        .is_empty());
    assert_eq!(rt.session(s).unwrap().history_lengths(), (history.0 + 1, 0));
    rt.undo(c, s).unwrap();
    assert_eq!(
        rt.document(s).unwrap().get_mesh(&id(3)).unwrap().masks,
        vec![id(2)]
    );
    let edit = rt.begin_edit(c, s, "lock", None).unwrap();
    rt.apply_edit(c, edit, |e| {
        let mut state = e.candidate_document().editor_state().clone();
        state.objects.entry(id(3)).or_default().locked = true;
        e.replace_editor_state(state)
    })
    .unwrap();
    rt.commit_edit(c, edit).unwrap();
    let (blocked, _, report) = rt.plan_modeling(c, s, None, intent()).unwrap();
    assert_eq!(report.deletion.unwrap().state, DeletionReviewState::Blocked);
    assert!(rt
        .deletion_report_page(c, blocked, S::Locks, 0, 1)
        .unwrap()
        .rows
        .iter()
        .any(|r| matches!(r, DeletionRow::Lock { blocker } if blocker.source_id == id(3))));
    let edit = rt.begin_edit(c, s, "blocked", None).unwrap();
    assert!(
        matches!(rt.apply_modeling_plan(c,edit,blocked),Err(RuntimeError::Sdk(e)) if e.code.as_ref()=="DELETION_NOT_READY")
    );
    assert_eq!(rt.commit_edit(c, edit), Err(RuntimeError::Aborted));
}
#[test]
fn cancelled_and_over_budget_deletion_jobs_publish_nothing() {
    let (mut rt, c, s) = fixture(Default::default());
    let before = rt.document(s).unwrap().clone();
    let count = rt.modeling_plan_count();
    let job = rt.request_modeling_plan(c, s, None, intent()).unwrap();
    let work = rt.take_work(job).unwrap();
    rt.cancel_job(c, job).unwrap();
    rt.complete(work.run());
    assert_eq!(rt.job_info(c, job).unwrap().state, JobState::Cancelled);
    assert_eq!(rt.modeling_plan_count(), count);
    rt.release_job(c, job).unwrap();
    assert!(rt.document(s).unwrap().same_content(&before));
    let (mut rt, c, s) = fixture(RuntimeLimits {
        plan_bytes: 1,
        ..Default::default()
    });
    assert!(matches!(
        rt.request_modeling_plan(c, s, None, intent()),
        Err(RuntimeError::Budget("plan bytes"))
    ));
    assert_eq!(rt.modeling_plan_count(), 0);
    assert!(rt.document(s).unwrap().get_mesh(&id(2)).is_some());
}

#[test]
fn direct_intents_cannot_bypass_the_review_plan() {
    let (mut rt, c, s) = fixture(Default::default());
    let before = rt.session(s).unwrap().history_lengths();
    let ModelingIntent::Deletion(intent) = intent() else {
        unreachable!()
    };
    let edit = rt.begin_edit(c, s, "unreviewed", None).unwrap();
    assert!(
        matches!(rt.apply_modeling(c,edit,vec![zipzi_runtime::modeling::ModelingCommand::Deletion { intent }]),
        Err(RuntimeError::Sdk(e)) if e.code.as_ref() == "DELETION_PLAN_REQUIRED")
    );
    assert_eq!(rt.commit_edit(c, edit), Err(RuntimeError::Aborted));
    assert_eq!(rt.session(s).unwrap().history_lengths(), before);
    assert!(rt.document(s).unwrap().get_mesh(&id(2)).is_some());
}

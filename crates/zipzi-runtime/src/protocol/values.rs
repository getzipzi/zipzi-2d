use super::*;
pub(super) fn info(s: &SessionInfo) -> Value {
    json!({"id": s.id, "state": match s.state { SessionState::Ready => "ready", SessionState::Busy(_) => "busy", SessionState::Faulted(_) => "faulted", SessionState::Closing => "closing", SessionState::Closed => "closed" },
        "job": match s.state { SessionState::Busy(job) | SessionState::Faulted(job) => Some(job), _ => None }, "version": s.version,
        "modified": s.modified, "history_lengths": s.history_lengths, "path": s.path,
        "fault": s.fault.as_ref().map(|e| error(e.clone()))})
}
pub(super) fn error(e: RuntimeError) -> Value {
    if let RuntimeError::Export { message, report } = e {
        return json!({"error":{"code":"EXPORT_LOSS","message":message,"operation":"project.export","project_report":report}});
    }
    if let RuntimeError::Sdk(e) = e {
        return json!({"error": {"category": "sdk", "code": e.code, "message": e.message,
            "operation": e.operation, "object_ids": e.object_ids, "field_path": e.field_path,
            "expected_version": e.expected_version, "actual_version": e.actual_version, "referrers": e.referrers}});
    }
    let code = match &e {
        RuntimeError::Busy => "BUSY",
        RuntimeError::Budget(_) => "BUDGET_EXCEEDED",
        RuntimeError::Aborted => "EDIT_ABORTED",
        RuntimeError::InvalidReference => "INVALID_REFERENCE",
        RuntimeError::WrongOwner => "WRONG_OWNER",
        RuntimeError::Faulted => "FAULTED",
        RuntimeError::ResyncRequired => "RESYNC_REQUIRED",
        RuntimeError::Cancelled => "CANCELLED",
        RuntimeError::Modified => "MODIFIED",
        RuntimeError::ExecutorFailure(_) => "EXECUTOR_FAILURE",
        RuntimeError::WorkerPanic(_) => "WORKER_PANIC",
        RuntimeError::Path(_) => "INVALID_PATH",
        RuntimeError::Sdk(_) | RuntimeError::Export { .. } => unreachable!(),
    };
    let mut result = failure(code, &e.to_string());
    if let RuntimeError::Budget(name) = &e {
        result["error"]["limit_name"] = json!(name);
    }
    result
}
pub(super) fn receipt(r: &zipzi_sdk::EditReceipt) -> Value {
    json!({"before": r.before, "after": r.after, "changed": r.changed,
        "label": r.label, "object_ids": r.object_ids, "changes": r.changes})
}
pub(super) fn saved(r: &zipzi_sdk::SaveReceipt) -> Value {
    json!({"before": r.before, "after": r.after, "manifest": r.manifest, "durable": r.durable,
        "warnings": r.warnings, "history_warnings": r.history_warnings})
}
pub(super) fn job_info(j: &JobInfo) -> Value {
    json!({"id": j.id, "session": j.session, "state": match j.state { JobState::Queued=>"queued", JobState::Running=>"running", JobState::Cancelling=>"cancelling", JobState::Succeeded=>"succeeded", JobState::Failed=>"failed", JobState::Cancelled=>"cancelled" },
    "result": j.result.as_ref().map(|result| match result {
        Err(e) => error(e.clone()),
        Ok(JobValue::PreparedPlan(_)) => failure("INTERNAL_RESULT", "Unpublished plan result"),
        Ok(value) => success(match value {
            JobValue::PreparedImage(_)=>unreachable!("unregistered image"),
            JobValue::ImagePrepared(value)=>json!(value),
            JobValue::Project{session,report}=>json!({"session":session,"report":report}),
            JobValue::Sampled(value)=>json!(value),
            JobValue::Previewed(value)=>json!(value),
            JobValue::Spatial(value)=>json!(value),
            JobValue::Planned(value)=>json!(value),
            JobValue::PreparedPlan(_)=>unreachable!("unpublished plan"),
            JobValue::Opened(s) => json!({"session": s}),
            JobValue::Saved(r) => saved(r),
            JobValue::Published(r) => receipt(r),
            JobValue::Imported { session, notices, name } => json!({"session": session, "notices": notices, "name": name}),
        }),
    })})
}
pub(super) fn event(e: &RuntimeEvent) -> Value {
    let (kind, value) = match &e.kind {
        EventKind::Published(r) => ("published", receipt(r)),
        EventKind::Saved(r) => ("saved", saved(r)),
        EventKind::StateChanged(s) => ("state_changed", info(s)),
        EventKind::Created(s) => ("created", info(s)),
        EventKind::Closed(s) => ("closed", info(s)),
    };
    json!({"cursor": e.cursor, "client": e.client, "job": e.job, "operation": e.operation,
        "kind": kind, "value": value})
}
pub(super) fn query(
    doc: &Document,
    version: Version,
    sequence: u64,
    meshes: Vec<String>,
    parameters: Vec<String>,
) -> Value {
    json!({"version": version, "sequence": sequence, "document_id": doc.id(),
        "mesh_ids": doc.mesh_order(),
        "parameter_ids": doc.parameter_order(),
        "meshes": meshes.iter().map(|id| doc.get_mesh(id)).collect::<Vec<_>>(),
        "parameters": parameters.iter().map(|id| doc.get_parameter(id)).collect::<Vec<_>>()})
}

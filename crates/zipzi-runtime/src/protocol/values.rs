use super::*;
pub(super) fn history(v: zipzi_sdk::HistoryState) -> Value {
    json!({"undo_steps":v.undo_steps,"redo_steps":v.redo_steps,"estimated_bytes":v.estimated_bytes,"over_target":v.over_target,
        "limits":{"max_steps":v.limits.max_steps,"target_bytes":v.limits.target_bytes}})
}
pub(super) fn publication(v: &zipzi_sdk::Publication) -> Value {
    json!({"version":v.version,"content":v.content,"kind":match v.kind {
        zipzi_sdk::PublicationKind::Commit=>"commit",zipzi_sdk::PublicationKind::Undo=>"undo",
        zipzi_sdk::PublicationKind::Redo=>"redo",zipzi_sdk::PublicationKind::Replace=>"replace",zipzi_sdk::PublicationKind::Unchanged=>"unchanged"},
        "description":v.description,"history":history(v.history),"writes":v.writes})
}
pub(super) fn session(rt: &Runtime, client: ClientId, key: SessionKey) -> Result<Value> {
    let s = rt.session_read(client, key)?;
    Ok(
        json!({"session":key,"version":s.version(),"content":s.snapshot().content_id(),
        "document_id":s.snapshot().document().id(),"path":s.project().map(|p| p.path()),
        "fingerprint":s.project().map(|p| p.fingerprint()),"dirty":s.is_dirty(),"history":history(s.history_state())}),
    )
}
pub(super) fn job(info: &JobInfo) -> Value {
    let result = info.result.as_ref().map(|r| match r {
        Err(e) => status(e.clone()),
        Ok(value) => success(match value {
            JobValue::DraftUpdated(source) => json!({"kind":"draft_updated","source":source}),
            JobValue::Published(p) => json!({"kind":"published","receipt":publication(p)}),
            JobValue::Opened(p) => json!({"kind":"opened","receipt":publication(&p.publication),"diagnostics":p.diagnostics.iter().map(|d| json!({"asset_id":d.asset_id,"code":d.code,"message":d.message})).collect::<Vec<_>>()}),
            JobValue::Saved(p) => json!({"kind":"saved","content":p.content,"path":p.path,"fingerprint":p.fingerprint,
                "published":p.published,"durable":p.durable,"warnings":p.warnings}),
            JobValue::Read(p) => json!({"kind":"read","source":p.source,"bytes":p.bytes()}),
        })
    });
    json!({"id":info.id,"source":info.source,"state":info.state,"result":result})
}

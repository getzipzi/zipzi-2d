use super::*;
pub(super) fn summarize_resources(before: &Document, after: &Document, out: &mut ChangeSet) {
    let entity = |id: &str| EntityRef::new(after.id(), ObjectKind::Asset, id);
    if before.asset_order() != after.asset_order() {
        out.collections.push(CollectionRef::Resources {
            document_id: after.id().into(),
        });
        out.categories.insert(ChangeCategory::Order);
    }
    for id in before
        .asset_order()
        .iter()
        .chain(after.asset_order())
        .collect::<BTreeSet<_>>()
    {
        let a = before.get_asset(id);
        let b = after.get_asset(id);
        if a == b {
            continue;
        }
        if a.is_some() && b.is_some() {
            out.modified.push(entity(id));
        }
        if a.map(|a| &a.name) != b.map(|b| &b.name) {
            out.categories.insert(ChangeCategory::Labels);
        }
        let content_changed =
            a.and_then(|a| a.content.as_ref()) != b.and_then(|b| b.content.as_ref());
        let source_changed = a.and_then(|a| a.unresolved_source.as_ref())
            != b.and_then(|b| b.unresolved_source.as_ref());
        if content_changed || source_changed {
            out.categories.insert(ChangeCategory::Content);
            if content_changed {
                out.categories.insert(ChangeCategory::EvaluationInputs);
            }
            out.resource_content.push(entity(id));
        }
    }
}

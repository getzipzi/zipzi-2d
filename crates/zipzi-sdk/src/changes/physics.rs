use super::*;
use std::collections::BTreeMap;
pub use zipzi_contract::changes::PhysicsReferenceChange;
pub use zipzi_contract::changes::PhysicsReferenceValue;
use zipzi_core::document::PhysicsAsset;
fn references(asset: Option<&PhysicsAsset>) -> BTreeMap<(String, String), PhysicsReferenceValue> {
    let mut out = BTreeMap::new();
    if let Some(a) = asset {
        for g in &a.data.settings {
            for v in &g.inputs {
                out.insert(
                    (v.id.clone(), "source".into()),
                    PhysicsReferenceValue::Parameter {
                        target: v.source.clone(),
                    },
                );
            }
            for v in &g.outputs {
                out.insert(
                    (v.id.clone(), "destination".into()),
                    PhysicsReferenceValue::Parameter {
                        target: v.destination.clone(),
                    },
                );
                out.insert(
                    (v.id.clone(), "node".into()),
                    PhysicsReferenceValue::Node {
                        id: v.node_id.clone(),
                    },
                );
            }
        }
    }
    out
}
pub(super) fn summarize_physics(before: &Document, after: &Document, out: &mut ChangeSet) {
    let a = before.physics();
    let b = after.physics();
    if a.zip(b).is_some_and(|(a, b)| std::ptr::eq(a, b)) || a.is_none() && b.is_none() {
        return;
    }
    let entity = |kind, id: &str| EntityRef::new(after.id(), kind, id);
    let collection = |owner: Option<String>, role: &str| CollectionRef::Physics {
        document_id: after.id().into(),
        owner,
        role: role.into(),
    };
    let ag = a.map(|a| &a.data.settings[..]).unwrap_or_default();
    let bg = b.map(|a| &a.data.settings[..]).unwrap_or_default();
    if ag.iter().map(|g| &g.id).ne(bg.iter().map(|g| &g.id)) {
        out.collections
            .push(collection(b.or(a).map(|a| a.id.clone()), "groups"));
        out.categories
            .extend([ChangeCategory::Order, ChangeCategory::EvaluationInputs]);
    }
    if a.map(|a| (&a.data.meta.dictionary, &a.data.extensions))
        != b.map(|a| (&a.data.meta.dictionary, &a.data.extensions))
    {
        out.categories.insert(ChangeCategory::Labels);
    }
    if a.map(|a| (&a.data.meta.forces, a.data.meta.fps))
        != b.map(|a| (&a.data.meta.forces, a.data.meta.fps))
    {
        out.categories.insert(ChangeCategory::EvaluationInputs);
    }
    if a != b {
        if let Some(asset) = b {
            out.modified.push(entity(ObjectKind::Physics, &asset.id));
        }
    }
    let ids: BTreeSet<_> = ag.iter().chain(bg).map(|g| &g.id).collect();
    for id in ids {
        let old = ag.iter().find(|g| &g.id == id);
        let new = bg.iter().find(|g| &g.id == id);
        if a.map(|a| a.group_name(id)) != b.map(|b| b.group_name(id)) {
            if new.is_some() {
                out.modified.push(entity(ObjectKind::PhysicsGroup, id));
            }
        }
        if old
            .zip(new)
            .is_some_and(|(a, b)| std::sync::Arc::ptr_eq(a, b))
        {
            continue;
        }
        if old != new {
            if let Some(g) = new {
                out.modified.push(entity(ObjectKind::PhysicsGroup, &g.id));
            }
            out.categories.insert(ChangeCategory::EvaluationInputs);
        }
        macro_rules! items {
            ($field:ident,$kind:ident,$role:expr) => {{
                let left = old.map(|g| &g.$field[..]).unwrap_or_default();
                let right = new.map(|g| &g.$field[..]).unwrap_or_default();
                if left.iter().map(|v| &v.id).ne(right.iter().map(|v| &v.id)) {
                    out.collections.push(collection(Some(id.clone()), $role));
                    out.categories.insert(ChangeCategory::Order);
                }
                let old_by_id = left
                    .iter()
                    .map(|v| &v.id)
                    .ne(right.iter().map(|v| &v.id))
                    .then(|| {
                        left.iter()
                            .map(|v| (v.id.as_str(), v))
                            .collect::<std::collections::HashMap<_, _>>()
                    });
                for (index, v) in right.iter().enumerate() {
                    let old = old_by_id
                        .as_ref()
                        .map_or_else(|| left.get(index), |rows| rows.get(v.id.as_str()).copied());
                    if old != Some(v) {
                        out.modified.push(entity(ObjectKind::$kind, &v.id));
                    }
                }
            }};
        }
        items!(inputs, PhysicsInput, "inputs");
        items!(outputs, PhysicsOutput, "outputs");
        items!(vertices, PhysicsNode, "nodes");
        if old.map(|g| g.vertices.iter().map(|v| &v.id).collect::<Vec<_>>())
            != new.map(|g| g.vertices.iter().map(|v| &v.id).collect::<Vec<_>>())
        {
            out.categories.insert(ChangeCategory::Topology);
        }
    }
    let left = references(a);
    let right = references(b);
    for ((id, field), a, b) in paired_records(&left, &right) {
        if a != b {
            out.categories.insert(ChangeCategory::References);
            out.physics_references.push(PhysicsReferenceChange {
                entity: entity(
                    if after.get_physics_input(id).is_some()
                        || before.get_physics_input(id).is_some()
                    {
                        ObjectKind::PhysicsInput
                    } else {
                        ObjectKind::PhysicsOutput
                    },
                    id,
                ),
                field: field.clone(),
                before: a.cloned(),
                after: b.cloned(),
            });
        }
    }
    // Created/deleted are reported separately, never also as modified.
    out.retain_existing_modified();
}

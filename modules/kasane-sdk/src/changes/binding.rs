use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BindingReferenceChange {
    pub entity: EntityRef,
    pub field: String,
    pub before: Option<String>,
    pub after: Option<String>,
}
pub(super) fn summarize_bindings(
    before: &Document,
    after: &Document,
    ids: &[String],
    out: &mut ChangeSet,
) {
    let entity = |kind, id: &str| EntityRef::new(after.id(), kind, id);
    let mut edge = |kind, id: &str, field: &str, old: Option<&str>, new: Option<&str>| {
        if old != new {
            out.binding_references.push(BindingReferenceChange {
                entity: entity(kind, id),
                field: field.into(),
                before: old.map(str::to_owned),
                after: new.map(str::to_owned),
            });
            out.categories.insert(ChangeCategory::References);
        }
    };
    for id in ids {
        let a = before.get_mesh(id);
        let b = after.get_mesh(id);
        if a.is_some() || b.is_some() {
            edge(
                ObjectKind::Mesh,
                id,
                "deformer_id",
                a.map(|m| m.deformer_id.as_str()),
                b.map(|m| m.deformer_id.as_str()),
            );
        }
        let a = before.get_transform(id);
        let b = after.get_transform(id);
        if a.is_some() || b.is_some() {
            edge(
                ObjectKind::Transform,
                id,
                "parent_id",
                a.map(|t| t.parent()),
                b.map(|t| t.parent()),
            );
        }
        let a = before.get_binding(id);
        let b = after.get_binding(id);
        if a.is_some() || b.is_some() {
            edge(
                ObjectKind::MeshBinding,
                id,
                "mesh_id",
                a.map(|b| b.mesh_id.as_str()),
                b.map(|b| b.mesh_id.as_str()),
            );
        }
        let a = before.get_scene_binding(id);
        let b = after.get_scene_binding(id);
        if a.is_some() || b.is_some() {
            edge(
                ObjectKind::SceneBinding,
                id,
                "target_id",
                a.map(|b| b.target_id()),
                b.map(|b| b.target_id()),
            );
        }
    }
    for id in ids {
        let a = before.get_binding(id);
        let b = after.get_binding(id);
        if (a.is_some() || b.is_some())
            && (a.map(|b| &b.axes) != b.map(|b| &b.axes)
                || a.map(|b| b.keyforms.len()) != b.map(|b| b.keyforms.len()))
        {
            out.collections.push(CollectionRef::BindingGrid {
                document_id: after.id().into(),
                binding_id: id.clone(),
            });
            out.categories.insert(ChangeCategory::Order);
        }
        let a = before.get_scene_binding(id);
        let b = after.get_scene_binding(id);
        if (a.is_some() || b.is_some())
            && (a.map(|b| &b.axes) != b.map(|b| &b.axes)
                || a.map(|b| b.track.len()) != b.map(|b| b.track.len()))
        {
            out.collections.push(CollectionRef::BindingGrid {
                document_id: after.id().into(),
                binding_id: id.clone(),
            });
            out.categories.insert(ChangeCategory::Order);
        }
    }
    let parents: BTreeSet<_> = out
        .binding_references
        .iter()
        .filter(|r| matches!(r.entity.kind, ObjectKind::Mesh | ObjectKind::Transform))
        .flat_map(|r| r.before.iter().chain(r.after.iter()))
        .cloned()
        .collect();
    if !parents.is_empty() {
        out.categories.insert(ChangeCategory::Order);
    }
    out.collections.extend(parents.into_iter().map(|parent_id| {
        CollectionRef::DeformationChildren {
            document_id: after.id().into(),
            parent_id,
        }
    }));
}

pub(super) fn summarize_binding_children(
    before: &Document,
    after: &Document,
    ids: &[String],
    out: &mut ChangeSet,
) {
    #[derive(PartialEq)]
    enum Value {
        Axis(kasane_core::BindingAxis),
        Sample(kasane_core::BindingSample, bool),
        Constraint(kasane_core::ConstraintSample),
    }
    fn values(
        d: &Document,
        ids: &[String],
    ) -> std::collections::HashMap<String, (ObjectKind, Value)> {
        let mut result = std::collections::HashMap::new();
        for owner in ids {
            let axes = d
                .get_binding(owner)
                .map(|b| b.axes.as_slice())
                .or_else(|| d.get_scene_binding(owner).map(|b| b.axes.as_slice()))
                .or_else(|| {
                    d.get_glue(owner)
                        .and_then(|g| g.binding.as_ref())
                        .map(|b| b.axes.as_slice())
                });
            if let Some(axes) = axes {
                for a in axes {
                    result.insert(
                        a.id.clone(),
                        (ObjectKind::BindingAxis, Value::Axis(a.clone())),
                    );
                    for s in &a.samples {
                        result.insert(
                            s.id.clone(),
                            (ObjectKind::BindingSample, Value::Sample(s.clone(), false)),
                        );
                    }
                }
            } else if let Some(t) = d.get_blend_key_table(owner) {
                for s in &t.samples {
                    result.insert(
                        s.id.clone(),
                        (
                            ObjectKind::BindingSample,
                            Value::Sample(s.clone(), t.base_sample_id == s.id),
                        ),
                    );
                }
            } else if let Some(c) = d.get_blend_constraint(owner) {
                for s in &c.samples {
                    result.insert(
                        s.id.clone(),
                        (ObjectKind::BindingSample, Value::Constraint(s.clone())),
                    );
                }
            }
        }
        result
    }
    let old = values(before, ids);
    let new = values(after, ids);
    for (id, (kind, value)) in &new {
        if old.get(id).is_some_and(|v| v.1 != *value) {
            out.modified.push(EntityRef::new(after.id(), *kind, id));
        }
    }
    let mut references = |kind, id: &str, field: &str, a: Option<&str>, b: Option<&str>| {
        if a != b {
            out.categories.insert(ChangeCategory::References);
            out.binding_references.push(BindingReferenceChange {
                entity: EntityRef::new(after.id(), kind, id),
                field: field.into(),
                before: a.map(str::to_owned),
                after: b.map(str::to_owned),
            });
            out.categories.insert(ChangeCategory::References);
        }
    };
    let axis_ids: std::collections::BTreeSet<_> = old
        .iter()
        .chain(&new)
        .filter_map(|(id, (_, v))| matches!(v, Value::Axis(_)).then_some(id))
        .collect();
    for id in axis_ids {
        let parameter = |values: &std::collections::HashMap<String, (ObjectKind, Value)>| {
            values.get(id).and_then(|(_, v)| {
                if let Value::Axis(a) = v {
                    Some(a.parameter_id.clone())
                } else {
                    None
                }
            })
        };
        references(
            ObjectKind::BindingAxis,
            id,
            "parameter_id",
            parameter(&old).as_deref(),
            parameter(&new).as_deref(),
        );
    }
    for id in ids {
        references(
            ObjectKind::BlendKeyTable,
            id,
            "parameter_id",
            before
                .get_blend_key_table(id)
                .map(|t| t.parameter_id.as_str()),
            after
                .get_blend_key_table(id)
                .map(|t| t.parameter_id.as_str()),
        );
        references(
            ObjectKind::BlendConstraint,
            id,
            "parameter_id",
            before
                .get_blend_constraint(id)
                .map(|t| t.parameter_id.as_str()),
            after
                .get_blend_constraint(id)
                .map(|t| t.parameter_id.as_str()),
        );
        references(
            ObjectKind::BlendBinding,
            id,
            "key_table_id",
            before
                .get_blend_binding(id)
                .map(|b| b.key_table_id.as_str()),
            after.get_blend_binding(id).map(|b| b.key_table_id.as_str()),
        );
    }
    for id in ids {
        let a = before.get_blend_key_table(id).map(|t| &t.base_sample_id);
        let b = after.get_blend_key_table(id).map(|t| &t.base_sample_id);
        if a != b {
            out.categories.insert(ChangeCategory::References);
            out.binding_references.push(BindingReferenceChange {
                entity: EntityRef::new(after.id(), ObjectKind::BlendKeyTable, id),
                field: "base_sample_id".into(),
                before: a.cloned(),
                after: b.cloned(),
            });
        }
        let changed = before.get_blend_key_table(id).map(|t| &t.samples)
            != after.get_blend_key_table(id).map(|t| &t.samples)
            || before.get_blend_constraint(id).map(|t| &t.samples)
                != after.get_blend_constraint(id).map(|t| &t.samples)
            || before
                .get_glue(id)
                .and_then(|g| g.binding.as_ref())
                .map(|b| &b.axes)
                != after
                    .get_glue(id)
                    .and_then(|g| g.binding.as_ref())
                    .map(|b| &b.axes);
        if changed {
            out.collections.push(CollectionRef::BindingGrid {
                document_id: after.id().into(),
                binding_id: id.clone(),
            });
            out.categories.insert(ChangeCategory::Order);
        }
    }
    out.binding_references
        .sort_by(|a, b| (&a.entity.id, &a.field).cmp(&(&b.entity.id, &b.field)));
}

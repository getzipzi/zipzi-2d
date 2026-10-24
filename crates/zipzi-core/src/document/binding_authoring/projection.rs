use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BindingOmission {
    pub object_id: String,
    pub code: String,
    pub dependency: Option<String>,
}
impl Document {
    pub fn binding_export_omissions(&self) -> Vec<BindingOmission> {
        let mut out = Vec::new();
        let mut grid = |id: &str, axes: &[BindingAxis]| {
            if axes.iter().all(|a| a.parameter_id.is_empty()) {
                out.push(BindingOmission {
                    object_id: id.into(),
                    code: "ZERO_AXIS_BINDING".into(),
                    dependency: None,
                });
            } else {
                out.extend(axes.iter().filter(|a| a.parameter_id.is_empty()).map(|a| {
                    BindingOmission {
                        object_id: a.id.clone(),
                        code: "NEEDS_CONFIGURATION".into(),
                        dependency: Some(id.into()),
                    }
                }));
            }
        };
        for b in self.content.bindings.values() {
            grid(&b.id, &b.axes);
        }
        for b in self.content.scene_bindings.values() {
            grid(&b.id, &b.axes);
        }
        for g in self.content.glues.values() {
            if let Some(b) = &g.binding {
                grid(&g.id, &b.axes);
            }
        }
        for t in self
            .content
            .blend_key_tables
            .values()
            .filter(|t| t.parameter_id.is_empty())
        {
            out.push(BindingOmission {
                object_id: t.id.clone(),
                code: "NEEDS_CONFIGURATION".into(),
                dependency: None,
            });
        }
        for c in self
            .content
            .blend_constraints
            .values()
            .filter(|c| c.parameter_id.is_empty())
        {
            out.push(BindingOmission {
                object_id: c.id.clone(),
                code: "NEEDS_CONFIGURATION".into(),
                dependency: None,
            });
        }
        for b in self
            .content
            .blend_bindings
            .values()
            .filter(|b| !self.blend_binding_active(b))
        {
            let dependency = self
                .get_blend_key_table(&b.key_table_id)
                .filter(|t| t.parameter_id.is_empty())
                .map(|t| t.id.clone())
                .or_else(|| {
                    b.constraint_ids
                        .iter()
                        .find(|id| {
                            self.get_blend_constraint(id)
                                .is_some_and(|c| c.parameter_id.is_empty())
                        })
                        .cloned()
                });
            out.push(BindingOmission {
                object_id: b.id.clone(),
                code: "NEEDS_CONFIGURATION".into(),
                dependency,
            });
        }
        out.sort_by(|a, b| a.object_id.cmp(&b.object_id));
        out
    }
    /// The explicit export projection shares the runtime availability rules.
    /// It is built once per accepted export, never in the frame loop.
    pub fn available_binding_export(&self) -> Document {
        let mut out = self.fork_candidate();
        out.content.bindings.retain(|_, b| {
            b.axes.retain(|a| !a.parameter_id.is_empty());
            !b.axes.is_empty()
        });
        out.content
            .binding_order
            .retain(|id| out.content.bindings.contains_key(id));
        out.content.scene_bindings.retain(|_, b| {
            b.axes.retain(|a| !a.parameter_id.is_empty());
            !b.axes.is_empty()
        });
        out.content
            .scene_binding_order
            .retain(|id| out.content.scene_bindings.contains_key(id));
        for g in out.content.glues.values_mut() {
            if let Some(b) = &mut g.binding {
                b.axes.retain(|a| !a.parameter_id.is_empty());
                if b.axes.is_empty() {
                    g.binding = None;
                }
            }
        }
        out.content
            .blend_bindings
            .retain(|_, b| self.blend_binding_active(b));
        out.content
            .blend_binding_order
            .retain(|id| out.content.blend_bindings.contains_key(id));
        out.content
            .blend_key_tables
            .retain(|_, t| !t.parameter_id.is_empty());
        out.content
            .blend_key_table_order
            .retain(|id| out.content.blend_key_tables.contains_key(id));
        out.content
            .blend_constraints
            .retain(|_, c| !c.parameter_id.is_empty());
        out.content
            .blend_constraint_order
            .retain(|id| out.content.blend_constraints.contains_key(id));
        out.binding_lookup.take();
        out.lookup.take();
        out.prepared.take();
        let mut state = std::mem::take(&mut out.content.editor_state);
        state.objects.retain(|id, _| out.contains_id(id));
        out.content.editor_state = state;
        out.snapshot_identity = Arc::new(());
        out.modified_cache.take();
        out
    }
    pub fn authoring_rest_projection(&self) -> Document {
        let mut out = self.fork_candidate();
        out.content.blend_bindings.clear();
        out.content.blend_binding_order.clear();
        out.content.glues.clear();
        out.content.glue_order.clear();
        out.lookup.take();
        out.binding_lookup.take();
        out.prepared.take();
        let mut flags = std::mem::take(&mut out.content.editor_state);
        flags.objects.retain(|id, _| out.contains_id(id));
        out.content.editor_state = flags;
        out.snapshot_identity = Arc::new(());
        out.modified_cache.take();
        out
    }
    pub fn binding_editability(&self, id: &str) -> Status {
        self.authoring_unlocked(id)
            .map_or_else(|s| s, |()| Status::ok())
    }
}

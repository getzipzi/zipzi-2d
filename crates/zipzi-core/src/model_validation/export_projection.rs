use super::*;
impl Model<'_> {
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
}

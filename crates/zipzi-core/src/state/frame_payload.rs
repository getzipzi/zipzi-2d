use super::*;

#[cfg(test)]
#[path = "frame_payload_tests.rs"]
mod tests;

impl DocumentBuilder {
    /// Labels and parameter defaults cannot change graph references or sample
    /// grids. Avoid rebuilding/validating the complete scene for these edits.
    pub(super) fn validate_metadata_payload(&self) -> Result<bool, Status> {
        if self.setup_touched
            || !self.roots_touched.is_empty()
            || !self.scene_initializations.is_empty()
        {
            return Ok(false);
        }
        if !self.content.assets.shares_order(&self.base.content.assets)
            || !self
                .content
                .parameters
                .shares_order(&self.base.content.parameters)
        {
            return Ok(false);
        }
        for id in &self.touched_assets {
            let Some((old, new)) = self.base.asset(id).zip(self.asset(id)) else {
                return Ok(false);
            };
            let mut original = new.clone();
            original.name.clone_from(&old.name);
            if original != *old {
                return Ok(false);
            }
        }
        for id in &self.touched_parameters {
            let Some((old, new)) = self.base.parameter(id).zip(self.parameter(id)) else {
                return Ok(false);
            };
            let mut original = new.clone();
            original.name.clone_from(&old.name);
            original.default_value = old.default_value;
            if original != *old {
                return Ok(false);
            }
            if crate::document::validate_parameter_range(
                new.minimum,
                new.maximum,
                new.default_value,
            )
            .is_err()
            {
                return Err(Status::error(
                    "INVALID_PARAMETER",
                    format!(
                        "{}: require finite minimum < maximum and default in range",
                        new.id
                    ),
                ));
            }
        }
        for (domain, ids) in &self.touched {
            macro_rules! labels {
                ($records:ident, $get:ident) => {{
                    if !self
                        .content
                        .$records
                        .shares_order(&self.base.content.$records)
                    {
                        return Ok(false);
                    }
                    for id in ids {
                        let Some((old, new)) = self.base.$get(id).zip(self.$get(id)) else {
                            return Ok(false);
                        };
                        let mut original = new.clone();
                        original.name.clone_from(&old.name);
                        if original != *old {
                            return Ok(false);
                        }
                    }
                }};
            }
            match domain {
                RecordDomain::Mesh => labels!(meshes, mesh),
                RecordDomain::Transform => labels!(transforms, transform),
                RecordDomain::Part => labels!(parts, part),
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    /// Check only touched records when all changes are to frame payloads. These
    /// fields leave identities, dependencies, sample grids, static geometry and
    /// drawing order bounds unchanged. Other edits use whole-graph validation.
    pub(super) fn validate_frame_payload(&self) -> Result<bool, Status> {
        if self.setup_touched
            || !self.touched_assets.is_empty()
            || !self.touched_parameters.is_empty()
            || !self.scene_initializations.is_empty()
        {
            return Ok(false);
        }
        if !self.roots_touched.is_empty() {
            if self.roots_touched.len() != 1
                || !self.roots_touched.contains(&RootDomain::Physics)
                || !self.touched.is_empty()
            {
                return Ok(false);
            }
            let Some((old, new)) = self.base.physics().as_ref().zip(self.physics().as_ref()) else {
                return Ok(false);
            };
            // Identities remain admitted in the base graph. Validate changed
            // rigs against those same parameter references and retain unchanged
            // rig Arcs without revalidating the scene's geometry and bindings.
            if old.id != new.id || old.identity_roles() != new.identity_roles() {
                return Ok(false);
            }
            let status = self.base.content.model().validate_physics(new, true);
            return if status.is_ok() {
                Ok(true)
            } else {
                Err(status)
            };
        }
        let model = self.content.model();
        for (domain, ids) in &self.touched {
            let unchanged_order = match domain {
                RecordDomain::Mesh => self.content.meshes.shares_order(&self.base.content.meshes),
                RecordDomain::Transform => self
                    .content
                    .transforms
                    .shares_order(&self.base.content.transforms),
                RecordDomain::Binding => self
                    .content
                    .bindings
                    .shares_order(&self.base.content.bindings),
                RecordDomain::SceneBinding => self
                    .content
                    .scene_bindings
                    .shares_order(&self.base.content.scene_bindings),
                _ => false,
            };
            if !unchanged_order {
                return Ok(false);
            }
            for id in ids {
                let status = match domain {
                    RecordDomain::Mesh => {
                        let Some((old, new)) = self.base.mesh(id).zip(self.mesh(id)) else {
                            return Ok(false);
                        };
                        let mut layout = new.clone();
                        layout.base_positions.clone_from(&old.base_positions);
                        layout.appearance = old.appearance;
                        if layout != *old {
                            return Ok(false);
                        }
                        model.validate_mesh_snapshot(new, false)
                    }
                    RecordDomain::Transform => {
                        let Some((old, new)) = self.base.transform(id).zip(self.transform(id))
                        else {
                            return Ok(false);
                        };
                        let shape = |t: &Transform| t.warp().map(|w| (w.rows, w.columns, w.quad));
                        if shape(old) != shape(new) {
                            return Ok(false);
                        }
                        let mut layout = new.clone();
                        layout.data.clone_from(&old.data);
                        layout.appearance = old.appearance;
                        if layout != *old {
                            return Ok(false);
                        }
                        model.validate_transform(new)
                    }
                    RecordDomain::Binding => {
                        let Some((old, new)) = self.base.binding(id).zip(self.binding(id)) else {
                            return Ok(false);
                        };
                        if old.id != new.id
                            || old.mesh_id != new.mesh_id
                            || old.axes != new.axes
                            || old.keyforms.len() != new.keyforms.len()
                            || !old.keyforms.iter().zip(&new.keyforms).all(|(a, b)| {
                                a.keys == b.keys
                                    && a.samples == b.samples
                                    && a.draw_order == b.draw_order
                            })
                        {
                            return Ok(false);
                        }
                        model.validate_admitted_binding(new)
                    }
                    RecordDomain::SceneBinding => {
                        let Some((old, new)) =
                            self.base.scene_binding(id).zip(self.scene_binding(id))
                        else {
                            return Ok(false);
                        };
                        if old.id != new.id
                            || old.axes != new.axes
                            || !same_scene_track_layout(&old.track, &new.track)
                        {
                            return Ok(false);
                        }
                        model.validate_admitted_scene_binding(new)
                    }
                    _ => return Ok(false),
                };
                if !status.is_ok() {
                    return Err(status);
                }
            }
        }
        Ok(true)
    }
}

fn same_scene_track_layout(old: &SceneTrack, new: &SceneTrack) -> bool {
    if old.target_id() != new.target_id() || old.len() != new.len() {
        return false;
    }
    match (old, new) {
        (SceneTrack::Warp { keyforms: a, .. }, SceneTrack::Warp { keyforms: b, .. }) => a
            .iter()
            .zip(b)
            .all(|(a, b)| a.keys == b.keys && a.samples == b.samples),
        (SceneTrack::Rotation { keyforms: a, .. }, SceneTrack::Rotation { keyforms: b, .. }) => a
            .iter()
            .zip(b)
            .all(|(a, b)| a.keys == b.keys && a.samples == b.samples),
        _ => false,
    }
}

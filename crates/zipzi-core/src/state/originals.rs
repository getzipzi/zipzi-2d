use super::*;
impl DocumentBuilder {
    /// Run after the complete candidate graph is valid, so forward references in
    /// a batch never expose an incomplete model to geometry evaluation.
    pub(super) fn finish_scene_initializations(&mut self) -> Result<(), Status> {
        let ids = std::mem::take(&mut self.scene_initializations)
            .into_iter()
            .filter(|id| self.transform(id).is_some())
            .collect::<Vec<_>>();
        for id in &ids {
            let mut t = self.transform(id).unwrap().clone();
            if t.original_shape.is_none() {
                let mut record = crate::DeformerOriginal::default();
                match self.content.model().capture_deformer_original(
                    &t,
                    &t.data,
                    crate::DeformerOriginalSource::Created,
                    &Default::default(),
                ) {
                    Ok(snapshot) => record.snapshot = Some(snapshot),
                    Err(status) => record.capture_error = Some(status.code),
                }
                t.original_shape = Some(record);
                self.replace_transform(t)?;
            }
        }
        if !ids.is_empty() {
            let mut editor = self.editor_state().clone();
            let model = self.content.model();
            editor
                .rotation_bone_reference_scales
                .extend(model.initial_rotation_bone_scales(&ids, &editor));
            editor
                .rotation_bone_lengths
                .extend(model.initial_rotation_bone_lengths(&ids, &editor));
            self.set_editor_state(editor)?;
        }
        let mut targets = BTreeSet::new();
        if let Some(ids) = self.touched.get(&RecordDomain::Transform) {
            targets.extend(ids.iter().cloned());
        }
        if let Some(ids) = self.touched.get(&RecordDomain::SceneBinding) {
            targets.extend(
                ids.iter()
                    .filter_map(|id| self.scene_binding(id).map(|b| b.target_id().to_owned())),
            );
        }
        for id in targets {
            // Raw decoding preserves supplied original-shape metadata. Automatic
            // first-key capture belongs to edits of existing or authored scene objects.
            if self.base.transform(&id).is_none() && !ids.contains(&id) {
                continue;
            }
            let model = self.content.model();
            if !model.binding_for_scene(&id).is_some_and(|b| {
                !b.track.is_empty() && b.axes.iter().any(|a| !a.parameter_id.is_empty())
            }) {
                continue;
            }
            let Some(mut t) = self
                .transform(&id)
                .filter(|t| t.original_shape.as_ref().is_some_and(|r| !r.first_key_seen))
                .cloned()
            else {
                continue;
            };
            let mut record = t.original_shape.clone().unwrap();
            record.first_key_seen = true;
            if record.auto_update {
                match model.capture_deformer_original(
                    &t,
                    &t.data,
                    crate::DeformerOriginalSource::FirstKey,
                    &Default::default(),
                ) {
                    Ok(snapshot) => {
                        record.snapshot = Some(snapshot);
                        record.capture_error = None;
                    }
                    Err(status) => record.capture_error = Some(status.code),
                }
            }
            t.original_shape = Some(record);
            self.replace_transform(t)?;
        }
        Ok(())
    }
}

impl DocumentBuilder {
    pub fn initialize_rotation_bone_reference_scales(&mut self) -> Result<(), Status> {
        self.write(|this| {
            this.normalize_touched()?;
            this.content.validate()?;
            let mut editor = this.editor_state().clone();
            let ids = this.transform_order().to_vec();
            let scales = this
                .content
                .model()
                .initial_rotation_bone_scales(&ids, &editor);
            editor.rotation_bone_reference_scales.extend(scales);
            for id in &ids {
                if this.transform(id).is_some_and(|t| t.rotation().is_some()) {
                    editor
                        .rotation_bone_angle_offsets
                        .entry(id.clone())
                        .or_insert(90.);
                }
            }
            editor.rotation_bone_lengths.extend(
                this.content
                    .model()
                    .initial_rotation_bone_lengths(&ids, &editor),
            );
            this.set_editor_state(editor)
        })
    }
}

impl DocumentState {
    pub fn rotation_bone_reference_scale(&self, id: &str) -> Option<f32> {
        self.content.model().rotation_bone_reference_scale(id)
    }
}

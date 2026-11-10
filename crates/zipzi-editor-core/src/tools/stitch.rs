//! Independent seam authoring. Mesh topology is an implementation detail, never
//! the active Mesh Edit mode. Only a completed click can modify the document.
use super::*;
use anyhow::Result;

pub(super) struct StitchSession {
    pub viewport: crate::ViewportId,
    pub press: Option<(FrameVersion, ViewportPoint)>,
    pub parts: Vec<ObjectRef>,
    pub engine: Option<manual_mesh::ManualMeshSession>,
    pub diagnostic: Option<String>,
    pub remove: bool,
}
impl StitchSession {
    pub fn new(document: &mut Document, viewport: crate::ViewportId) -> Result<Self> {
        let selected = document.context().selections.clone();
        let existing = selected
            .first()
            .filter(|o| selected.len() == 1 && o.kind == ObjectKind::Glue)
            .and_then(|o| document.model().glue(&o.id).cloned());
        let parts = if let Some(glue) = &existing {
            vec![
                ObjectRef::new(ObjectKind::Mesh, glue.mesh_a_id.as_str()),
                ObjectRef::new(ObjectKind::Mesh, glue.mesh_b_id.as_str()),
            ]
        } else if selected.len() <= 2 && selected.iter().all(|o| o.kind == ObjectKind::Mesh) {
            selected
        } else {
            Vec::new()
        };
        let mut state = Self {
            viewport,
            press: None,
            parts,
            engine: None,
            diagnostic: None,
            remove: false,
        };
        document.select_objects(state.parts.clone())?;
        state.prepare(document)?;
        if let Some(glue) = existing {
            state
                .engine
                .as_mut()
                .unwrap()
                .set_glue_target(Some(glue.id));
        }
        Ok(state)
    }
    fn prepare(&mut self, document: &Document) -> Result<()> {
        if self.parts.len() == 2 {
            let targets = document.manual_mesh_targets()?;
            let mut engine = manual_mesh::ManualMeshSession::new(targets, document);
            engine.bind_viewport(self.viewport);
            engine.start_stitch(document);
            self.engine = Some(engine);
        }
        Ok(())
    }
    pub fn input(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
        event: CanvasInput,
    ) -> Result<bool> {
        if frame.version.binding.viewport != self.viewport {
            return Ok(false);
        }
        match event {
            CanvasInput::Down { point, .. } => {
                anyhow::ensure!(
                    point.x.is_finite() && point.y.is_finite(),
                    "Invalid pointer position"
                );
                self.press = Some((frame.version, point));
            }
            CanvasInput::Cancel | CanvasInput::Leave => self.press = None,
            CanvasInput::Move { point, left_down } => {
                if !left_down
                    || self
                        .press
                        .is_some_and(|(_, start)| crate::picking::distance2(start, point) >= 9.)
                {
                    self.press = None;
                }
            }
            CanvasInput::Up { point } => {
                let Some((_, start)) = self.press.take() else {
                    return Ok(false);
                };
                if crate::picking::distance2(start, point) >= 9. {
                    return Ok(false);
                }
                let result = self.click(document, frame, point);
                return match result {
                    Ok(changed) => {
                        self.diagnostic = None;
                        Ok(changed)
                    }
                    Err(error) => {
                        self.diagnostic = Some(error.to_string());
                        Ok(false)
                    }
                };
            }
            _ => {}
        }
        Ok(false)
    }
    fn click(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
        point: ViewportPoint,
    ) -> Result<bool> {
        if let Some(engine) = &mut self.engine {
            if self.remove {
                let controls = engine.glue_controls(document, Some(frame)).unwrap();
                let Some(id) = controls.target else {
                    return Ok(false);
                };
                let model = document.model();
                let glue = model.glue(&id).unwrap();
                let pair = glue
                    .pairs
                    .iter()
                    .filter_map(|pair| {
                        let distances = [
                            (glue.mesh_a_id.as_str(), pair.vertex_a),
                            (glue.mesh_b_id.as_str(), pair.vertex_b),
                        ]
                        .into_iter()
                        .filter_map(|(mesh, vertex)| {
                            frame
                                .geometry
                                .mesh(mesh)?
                                .vertices
                                .iter()
                                .find(|(id, _)| Some(*id) == vertex)
                                .map(|(_, p)| {
                                    crate::picking::distance2(
                                        point,
                                        frame.transform.to_viewport(*p),
                                    )
                                })
                        });
                        let distance = distances.min_by(f32::total_cmp)?;
                        (distance <= crate::canvas_style::POINT_HIT_RADIUS.powi(2))
                            .then_some((distance, pair.id.clone()))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, id)| id);
                drop(model);
                let Some(pair) = pair else {
                    return Ok(false);
                };
                document.ensure_editable(&[ObjectRef::new(ObjectKind::Glue, &id)])?;
                anyhow::ensure!(
                    frame
                        .geometry
                        .in_isolation_scope(&ObjectRef::new(ObjectKind::Glue, &id))
                        && self.parts.iter().all(|p| frame.geometry.mesh_visible(&p.id)
                            && frame.geometry.in_isolation_scope(p)),
                    "Show both parts before editing the seam"
                );
                anyhow::ensure!(
                    document
                        .object_state(&ObjectRef::new(ObjectKind::Glue, &id))
                        .visible(),
                    "Show the seam before editing it"
                );
                for target in engine.targets() {
                    document.validate_manual_target(&target)?;
                }
                let changed =
                    document.unbind_glue_vertices(&id, &[pair], document.session.version())?;
                engine.refresh_source(document)?;
                return Ok(changed);
            }
            return engine.insert_overlap(document, frame, point);
        }
        let candidates = frame.geometry.candidates_with_locks(
            point,
            frame.transform,
            &[],
            ToolId::Select,
            DeformerVisibility::default(),
            &document.object_state_index(),
        );
        if let Some(candidate) = candidates
            .into_iter()
            .find(|c| c.object.kind == ObjectKind::Mesh && !self.parts.contains(&c.object))
        {
            document.manual_mesh_target_for(&candidate.object)?;
            let mut parts = self.parts.clone();
            parts.push(candidate.object);
            document.select_objects(parts.clone())?;
            self.parts = parts;
            self.prepare(document)?;
        }
        Ok(false)
    }
    pub fn overlay(&self, frame: &CanvasFrame) -> OverlaySnapshot {
        if frame.version.binding.viewport != self.viewport {
            return OverlaySnapshot::default();
        }
        let mut out = self
            .engine
            .as_ref()
            .map_or_else(OverlaySnapshot::default, |m| m.overlay(frame, false, false));
        if self.engine.is_none() {
            for part in &self.parts {
                if let Some(mesh) = frame.geometry.mesh(&part.id) {
                    out.accent_lines.extend(mesh.boundary.iter().map(|(a, b)| {
                        (
                            frame.transform.to_viewport(*a),
                            frame.transform.to_viewport(*b),
                        )
                    }));
                }
            }
        }
        out.vertices.clear();
        out.lines.clear();
        out.automatic_lines.clear();
        out.reference_lines.clear();
        out.image_bounds.clear();
        out.manual_mesh_feedback = None;
        out.manual_mesh_delete_unavailable = false;
        out.unavailable = self.diagnostic.clone();
        out
    }
}

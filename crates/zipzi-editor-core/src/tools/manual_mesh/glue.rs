//! Mesh Edit uses the displayed pose for overlap, and each mesh's own UV domain
//! for topology edits. Matching is independent of the viewport transform.
use super::*;
use crate::ModelPoint;
use anyhow::{anyhow, ensure};

const OVERLAP_TOLERANCE: f32 = 0.001;

fn coincident(a: ModelPoint, b: ModelPoint) -> bool {
    (a.x - b.x).hypot(a.y - b.y) <= OVERLAP_TOLERANCE
}

fn match_vertices(
    mut a: Vec<(VertexId, ModelPoint)>,
    mut b: Vec<(VertexId, ModelPoint)>,
) -> Result<Vec<(VertexId, VertexId)>> {
    ensure!(
        !a.is_empty() && a.len() == b.len(),
        "Select matching vertices on both meshes"
    );
    a.sort_unstable_by_key(|vertex| vertex.0);
    b.sort_unstable_by(|a, b| a.1.x.total_cmp(&b.1.x));
    let mut used = std::collections::HashSet::new();
    a.into_iter()
        .map(|(id, point)| {
            let start = b.partition_point(|(_, p)| p.x < point.x - OVERLAP_TOLERANCE);
            let mut matches = b[start..]
                .iter()
                .take_while(|(_, p)| p.x <= point.x + OVERLAP_TOLERANCE)
                .filter(|(_, p)| coincident(point, *p));
            let &(other, _) = matches
                .next()
                .ok_or_else(|| anyhow!("Selected vertices do not overlap"))?;
            ensure!(
                matches.next().is_none() && used.insert(other),
                "Selected vertices overlap ambiguously"
            );
            Ok((id, other))
        })
        .collect()
}

pub struct ManualGlueControls {
    pub sync_insert: bool,
    pub stitch: bool,
    pub pair_count: usize,
    pub target: Option<String>,
    pub meshes: [String; 2],
    pub targets: Vec<(String, String)>,
    pub bind_count: usize,
    pub unbind_count: usize,
    pub diagnostic: Option<String>,
}

impl ManualMeshSession {
    pub fn start_stitch(&mut self, document: &Document) {
        self.stitch = true;
        self.sync_insert = true;
        for mode in &mut self.modes {
            mode.tool = ManualTool::Select;
        }
        if self.glue_target.is_none() {
            let controls = self.glue_controls(document, None);
            if let Some(controls) = controls.filter(|c| c.targets.len() == 1) {
                self.glue_target = Some(controls.targets[0].0.clone());
            }
        }
    }

    pub fn toggle_sync_insert(&mut self) {
        self.stitch = false;
        self.sync_insert = !self.sync_insert;
    }

    pub fn set_glue_target(&mut self, target: Option<String>) {
        self.glue_target = target;
        self.retired_glue_target = None;
        self.diagnostic = None;
    }

    fn ordered_meshes(&self, document: &Document) -> [&ManualMeshMode; 2] {
        let a = &self.modes[0];
        let b = &self.modes[1];
        let model = document.model();
        if self
            .glue_target
            .as_deref()
            .and_then(|id| model.glue(id))
            .is_some_and(|glue| glue.mesh_a_id.as_str() == b.target.object.id)
        {
            [b, a]
        } else {
            [a, b]
        }
    }

    pub fn glue_controls(
        &self,
        document: &Document,
        frame: Option<&CanvasFrame>,
    ) -> Option<ManualGlueControls> {
        if self.modes.len() != 2 {
            return None;
        }
        let [a, b] = self.ordered_meshes(document);
        let (a, b) = (&a.target.object.id, &b.target.object.id);
        let model = document.model();
        let targets = model
            .glue_order()
            .iter()
            .filter_map(|id| {
                let glue = model.glue(id)?;
                let sides = [glue.mesh_a_id.as_str(), glue.mesh_b_id.as_str()];
                (sides == [a, b] || sides == [b, a]).then(|| (id.clone(), glue.name.clone()))
            })
            .collect();
        let binding = frame.map(|frame| self.binding_pairs(document, frame));
        let release = frame
            .filter(|frame| {
                (0..2).all(|index| self.shown(index, frame))
                    && self.glue_target.as_deref().is_some_and(|id| {
                        self.ensure_glue_target_visible(document, frame, id).is_ok()
                    })
            })
            .map_or_else(Vec::new, |_| self.release_pairs(document));
        let diagnostic = binding
            .as_ref()
            .and_then(|result| result.as_ref().err())
            .filter(|_| self.modes.iter().any(|m| !m.selected.is_empty()))
            .map(ToString::to_string);
        Some(ManualGlueControls {
            sync_insert: self.sync_insert,
            stitch: self.stitch,
            pair_count: self
                .glue_target
                .as_deref()
                .and_then(|id| model.glue(id))
                .map_or(0, |g| g.pairs.len()),
            target: self.glue_target.clone(),
            meshes: [a, b].map(|id| {
                model
                    .mesh(id)
                    .map_or_else(|| id.clone(), |m| m.name.clone())
            }),
            targets,
            bind_count: binding.and_then(Result::ok).map_or(0, |pairs| pairs.len()),
            unbind_count: release.len(),
            diagnostic,
        })
    }

    fn binding_pairs(
        &self,
        document: &Document,
        frame: &CanvasFrame,
    ) -> Result<Vec<(VertexId, VertexId)>> {
        ensure!(
            (0..2).all(|index| self.shown(index, frame)),
            "Both meshes must be visible in this viewport"
        );
        let positions = |mode: &ManualMeshMode| -> Result<Vec<_>> {
            document.validate_manual_target(&mode.target)?;
            let mesh = frame
                .geometry
                .mesh(&mode.target.object.id)
                .ok_or_else(|| anyhow!("Mesh is unavailable"))?;
            let topology = &frame.geometry.topology.meshes[&mode.target.object.id];
            mode.selected
                .iter()
                .map(|id| {
                    topology
                        .slots
                        .get(id)
                        .and_then(|&slot| mesh.vertices.get(slot))
                        .copied()
                        .ok_or_else(|| anyhow!("A selected vertex no longer exists"))
                })
                .collect()
        };
        let [a, b] = self.ordered_meshes(document);
        let pairs = match_vertices(positions(a)?, positions(b)?)?;
        let target = self.glue_target.as_deref();
        if let Some(id) = target {
            self.ensure_glue_target_visible(document, frame, id)?;
        }
        zipzi_sdk::glue_binding_additions(
            &document.model(),
            target,
            &a.target.object.id,
            &b.target.object.id,
            &pairs,
        )?;
        Ok(pairs)
    }

    fn ensure_glue_target_visible(
        &self,
        document: &Document,
        frame: &CanvasFrame,
        id: &str,
    ) -> Result<()> {
        let object = crate::ObjectRef::new(crate::ObjectKind::Glue, id);
        document.ensure_editable(std::slice::from_ref(&object))?;
        ensure!(
            document.object_state(&object).visible() && frame.geometry.in_isolation_scope(&object),
            "Glue must be visible in this viewport"
        );
        Ok(())
    }

    fn release_pairs(&self, document: &Document) -> Vec<String> {
        let [a, b] = self.ordered_meshes(document);
        let model = document.model();
        let Some(glue) = self.glue_target.as_deref().and_then(|id| model.glue(id)) else {
            return Vec::new();
        };
        if glue.mesh_a_id.as_str() != a.target.object.id
            || glue.mesh_b_id.as_str() != b.target.object.id
        {
            return Vec::new();
        }
        glue.pairs
            .iter()
            .filter(|pair| {
                pair.vertex_a.is_some_and(|id| a.selected.contains(&id))
                    || pair.vertex_b.is_some_and(|id| b.selected.contains(&id))
            })
            .map(|pair| pair.id.clone())
            .collect()
    }

    pub fn edit_glue(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
        bind: bool,
    ) -> Result<bool> {
        ensure!(
            self.modes.len() == 2 && self.editable() && !self.source_view,
            "Select two editable meshes"
        );
        ensure!(
            (0..2).all(|index| self.shown(index, frame)),
            "Both meshes must be visible in this viewport"
        );
        for mode in &self.modes {
            document.validate_manual_target(&mode.target)?;
        }
        let target = self.glue_target.as_deref();
        let zipzi_sdk::QuerySource::Published(version) = self.modes[0].target.source else {
            anyhow::bail!("Mesh Edit requires a published source");
        };
        let changed = if bind {
            let pairs = self.binding_pairs(document, frame)?;
            let [a, b] = self.ordered_meshes(document);
            let before = document.session.version();
            let id = document.bind_glue_vertices(
                target,
                &a.target.object.id,
                &b.target.object.id,
                &pairs,
                version,
            )?;
            self.glue_target = Some(id);
            self.retired_glue_target = None;
            document.session.version() != before
        } else {
            let id = target.ok_or_else(|| anyhow!("Choose a Glue to unbind"))?;
            self.ensure_glue_target_visible(document, frame, id)?;
            let pairs = self.release_pairs(document);
            document.unbind_glue_vertices(id, &pairs, version)?
        };
        if changed {
            self.refresh_source(document)?;
        }
        self.diagnostic = None;
        Ok(changed)
    }

    pub(in crate::tools) fn insert_overlap(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
        point: ViewportPoint,
    ) -> Result<bool> {
        ensure!(
            self.modes.len() == 2 && (0..2).all(|index| self.shown(index, frame)),
            "Both meshes must be visible in this viewport"
        );
        let mut hits = [None; 2];
        for (index, mode) in self.modes.iter().enumerate() {
            document.validate_manual_target(&mode.target)?;
            let mesh = frame
                .geometry
                .mesh(&mode.target.object.id)
                .ok_or_else(|| anyhow!("Mesh is unavailable"))?;
            let mut nearby = mesh.vertices.iter().copied().filter(|(_, p)| {
                crate::picking::distance2(point, frame.transform.to_viewport(*p))
                    <= crate::canvas_style::POINT_HIT_RADIUS.powi(2)
            });
            hits[index] = nearby.next();
            ensure!(
                nearby.next().is_none(),
                "Several vertices are under the cursor. Zoom in to choose one"
            );
        }
        if let [Some((_, a)), Some((_, b))] = hits {
            ensure!(
                coincident(a, b),
                "Nearby vertices do not overlap. Zoom in to choose one"
            );
        }
        let position = hits
            .iter()
            .flatten()
            .next()
            .map_or_else(|| frame.transform.to_model(point), |&(_, p)| p);
        let mut selected = [0; 2];
        let mut operations = Vec::new();
        for (index, mode) in self.modes.iter().enumerate() {
            let mesh_id = &mode.target.object.id;
            let shown = frame.geometry.mesh(mesh_id).unwrap();
            let mut nearby = shown
                .vertices
                .iter()
                .filter(|(_, p)| coincident(position, *p));
            if let Some(&(id, _)) = nearby.next() {
                ensure!(
                    nearby.next().is_none(),
                    "Several vertices share this position"
                );
                selected[index] = id;
                continue;
            }
            let initial = PositionUvMap::displayed(document, frame, mesh_id)?
                .uv(Vec2::new(position.x, position.y))?;
            let uv =
                PoseMapping::capture(document, frame, mesh_id)?.insert_uv(position, initial)?;
            let model = document.model();
            let mesh = model.mesh(mesh_id).unwrap();
            let geometry = MeshGeometry {
                vertex_ids: mesh.vertex_ids.clone(),
                positions: mesh.base_positions.clone(),
                uvs: mesh.uvs.clone(),
                triangles: mesh.triangles.clone(),
                meshing: mesh.meshing.clone(),
            };
            let id = mode.allocator.lock().unwrap().allocate(&geometry)?;
            selected[index] = id;
            operations.push((
                mode.target.clone(),
                ManualMeshOperation::InsertVertex { id, uv },
            ));
        }
        let changed = !operations.is_empty();
        if self.stitch {
            if let Some(id) = self.glue_target.as_deref() {
                self.ensure_glue_target_visible(document, frame, id)?;
            }
            let [a, b] = self.ordered_meshes(document);
            let pair = if a.target.object.id == self.modes[0].target.object.id {
                (selected[0], selected[1])
            } else {
                (selected[1], selected[0])
            };
            let before = document.session.version();
            let id = document.stitch_glue_vertices(
                self.glue_target.as_deref(),
                [&a.target, &b.target],
                pair,
                operations,
            )?;
            self.glue_target = Some(id);
            self.retired_glue_target = None;
            self.refresh_source(document)?;
            for (mode, id) in self.modes.iter_mut().zip(selected) {
                mode.selected = vec![id];
                mode.selected_edge = None;
            }
            return Ok(document.session.version() != before);
        }
        if changed {
            document.preview_manual_mesh_batch(operations)?;
            document.finish_manual_mesh_gesture()?;
            self.refresh_source(document)?;
        }
        for (mode, id) in self.modes.iter_mut().zip(selected) {
            mode.selected = vec![id];
            mode.selected_edge = None;
        }
        Ok(changed)
    }
}

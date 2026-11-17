//! One explicitly selected endpoint per side can bind across a gap. Batch
//! selection requires unique coincident matches in the displayed pose.
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
    if a.len() == 1 {
        return Ok(vec![(a[0].0, b[0].0)]);
    }
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
                .ok_or_else(|| anyhow!("For separated vertices, select one vertex on each mesh and bind each pair separately"))?;
            ensure!(
                matches.next().is_none() && used.insert(other),
                "Selected vertices overlap ambiguously"
            );
            Ok((id, other))
        })
        .collect()
}

pub struct ManualGlueControls {
    pub add_glue_point: bool,
    pub pair_count: usize,
    pub target: Option<String>,
    pub meshes: [String; 2],
    pub targets: Vec<(String, String)>,
    pub bind_count: usize,
    pub unbind_count: usize,
    pub diagnostic: Option<String>,
}

pub(super) struct BindingHandle {
    pub(super) key: (String, String),
    ends: [(ViewportPoint, Option<(usize, VertexId)>); 2],
    position: ViewportPoint,
    selected: bool,
    editable: bool,
}

impl ManualMeshSession {
    fn binding_handles(&self, document: &Document, frame: &CanvasFrame) -> Vec<BindingHandle> {
        if self.source_view || self.edit_viewport != Some(frame.version.binding.viewport) {
            return Vec::new();
        }
        let model = document.model();
        let mut handles = Vec::new();
        for topology in frame.geometry.topology.glues.iter() {
            let Some(glue) = model.glue(&topology.object.id) else {
                continue;
            };
            let sides = [glue.mesh_a_id.as_str(), glue.mesh_b_id.as_str()];
            if !sides
                .iter()
                .any(|id| self.modes.iter().any(|m| m.target.object.id == *id))
                || !frame.geometry.in_isolation_scope(&topology.object)
                || sides.iter().any(|id| {
                    !frame.geometry.mesh_visible(id)
                        || !frame.geometry.in_isolation_scope(&crate::ObjectRef::new(
                            crate::ObjectKind::Mesh,
                            *id,
                        ))
                })
                || !frame
                    .geometry
                    .control(&glue.id)
                    .is_some_and(|c| c.visible && c.enabled)
            {
                continue;
            }
            for pair in &glue.pairs {
                let endpoints = sides
                    .into_iter()
                    .zip([pair.vertex_a, pair.vertex_b])
                    .map(|(mesh, vertex)| {
                        let vertex = vertex?;
                        let p = frame
                            .geometry
                            .mesh(mesh)?
                            .vertices
                            .get(
                                *frame
                                    .geometry
                                    .topology
                                    .meshes
                                    .get(mesh)?
                                    .slots
                                    .get(&vertex)?,
                            )?
                            .1;
                        let index = self.modes.iter().position(|m| m.target.object.id == mesh);
                        Some((frame.transform.to_viewport(p), index.map(|i| (i, vertex))))
                    })
                    .collect::<Option<Vec<_>>>();
                let Some(ends) = endpoints.and_then(|v| <[_; 2]>::try_from(v).ok()) else {
                    continue;
                };
                let selected = ends.iter().all(|(_, side)| {
                    side.is_some_and(|(i, id)| self.modes[i].selected.contains(&id))
                });
                let editable = self.editable()
                    && ends.iter().all(|(_, side)| side.is_some())
                    && document
                        .ensure_editable(std::slice::from_ref(&topology.object))
                        .is_ok();
                handles.push(BindingHandle {
                    key: (glue.id.clone(), pair.id.clone()),
                    position: ViewportPoint {
                        x: (ends[0].0.x + ends[1].0.x) * 0.5,
                        y: (ends[0].0.y + ends[1].0.y) * 0.5,
                    },
                    ends,
                    selected,
                    editable,
                });
            }
        }
        handles
    }

    pub(super) fn pick_binding(
        &self,
        document: &Document,
        frame: &CanvasFrame,
        point: ViewportPoint,
    ) -> Option<BindingHandle> {
        self.binding_handles(document, frame)
            .into_iter()
            .filter(|h| h.ends.iter().all(|(_, side)| side.is_some()))
            .filter(|h| {
                crate::picking::distance2(h.position, point)
                    <= crate::canvas_style::POINT_HIT_RADIUS.powi(2)
            })
            .min_by(|a, b| {
                crate::picking::distance2(a.position, point)
                    .total_cmp(&crate::picking::distance2(b.position, point))
                    .then_with(|| {
                        (Some(b.key.0.as_str()) == self.glue_target.as_deref())
                            .cmp(&(Some(a.key.0.as_str()) == self.glue_target.as_deref()))
                    })
            })
    }

    pub(super) fn capture_shared_drag(
        &self,
        document: &Document,
        frame: &CanvasFrame,
        press: Gesture,
    ) -> Result<SharedGesture> {
        let mut meshes = Vec::new();
        for (index, mode) in self.modes.iter().enumerate() {
            if mode.selected.is_empty() || !self.shown(index, frame) {
                continue;
            }
            meshes.push((
                index,
                mode.selected_starts(document),
                PoseMapping::capture(document, frame, &mode.target.object.id)?,
            ));
        }
        Ok(SharedGesture::Drag { press, meshes })
    }

    pub(super) fn binding_region_vertices(
        &self,
        document: &Document,
        frame: &CanvasFrame,
        region: &SelectionRegion<'_>,
    ) -> Vec<(usize, VertexId)> {
        self.binding_handles(document, frame)
            .into_iter()
            .filter(|h| h.editable && region.contains(h.position))
            .flat_map(|h| h.ends.into_iter().filter_map(|(_, side)| side))
            .collect()
    }

    pub(super) fn begin_binding_drag(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
        handle: BindingHandle,
        point: ViewportPoint,
        shift: bool,
        alt: bool,
    ) -> Result<bool> {
        self.hovered_binding = Some(handle.key.clone());
        if !handle.editable {
            self.diagnostic =
                Some("Unlock the Glue and both meshes before moving its binding point".into());
            return Ok(false);
        }
        if !shift && !alt && !handle.selected {
            for mode in &mut self.modes {
                mode.selected.clear();
                mode.selected_edge = None;
            }
        }
        for (_, side) in &handle.ends {
            let (i, id) = side.unwrap();
            let mode = &mut self.modes[i];
            mode.selected_edge = None;
            if alt {
                mode.selected.retain(|v| *v != id);
            } else if !mode.selected.contains(&id) {
                mode.selected.push(id);
            }
        }
        self.glue_target = Some(handle.key.0);
        self.retired_glue_target = None;
        self.diagnostic = None;
        self.retain_context(document);
        if alt {
            return Ok(false);
        }
        let press = Gesture {
            version: frame.version,
            transform: frame.transform,
            start: point,
            current: point,
            kind: GestureKind::Drag(Vec::new()),
            moved: false,
            pose: None,
        };
        self.shared = Some(self.capture_shared_drag(document, frame, press)?);
        Ok(false)
    }

    pub(super) fn binding_overlay(
        &self,
        document: &Document,
        frame: &CanvasFrame,
        alt: bool,
        out: &mut OverlaySnapshot,
    ) {
        for handle in self.binding_handles(document, frame) {
            let hovered = self.hovered_binding.as_ref() == Some(&handle.key);
            if hovered {
                out.manual_mesh_feedback = Some(ManualMeshFeedback::Vertex);
                out.manual_mesh_delete_unavailable = true;
                out.selection_operation = alt.then_some(crate::CanvasSelectionOperation::Remove);
                if !handle.editable {
                    out.point_unavailable = Some(
                        "Unlock the Glue and both meshes before moving its binding point".into(),
                    );
                }
            }
            out.glues.push(crate::OverlayGlue {
                pairs: vec![],
                points: vec![crate::OverlayGluePoint {
                    position: handle.position,
                    state: crate::GluePointState::Binding,
                    selected: handle.selected,
                    hovered,
                }],
                selected: Some(handle.key.0.as_str()) == self.glue_target.as_deref(),
                hovered,
            });
        }
    }
}

impl ManualMeshSession {
    pub fn toggle_glue_insert(&mut self) {
        let enabled = !self.add_glue_point;
        self.set_tool(ManualTool::Select);
        self.add_glue_point = enabled;
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
            add_glue_point: self.add_glue_point,
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

    pub(super) fn insert_overlap(
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
        Ok(document.session.version() != before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_glue_pair_accepts_separation_but_batches_need_unambiguous_matches() {
        let p = |x| ModelPoint { x, y: 0. };
        assert_eq!(
            match_vertices(vec![(7, p(0.))], vec![(3, p(100.))]).unwrap(),
            vec![(7, 3)]
        );
        assert_eq!(
            match_vertices(vec![(7, p(0.)), (8, p(10.))], vec![(3, p(10.)), (4, p(0.))]).unwrap(),
            vec![(7, 4), (8, 3)]
        );
        assert!(
            match_vertices(
                vec![(7, p(0.)), (8, p(10.))],
                vec![(3, p(100.)), (4, p(110.))]
            )
            .is_err()
        );
        assert!(
            match_vertices(vec![(7, p(0.)), (8, p(0.))], vec![(3, p(0.)), (4, p(0.))]).is_err()
        );
        assert!(match_vertices(vec![(7, p(0.))], vec![]).is_err());
    }
}

//! Shared selection and atomic gestures across independently owned mesh topologies.
use super::*;
use crate::ModelVector;
use std::ops::{Deref, DerefMut};

#[path = "glue.rs"]
mod glue;
pub use glue::ManualGlueControls;

enum SharedGesture {
    Region(Gesture),
    Drag {
        press: Gesture,
        meshes: Vec<(usize, Vec<ManualVertexTarget>, PoseMapping)>,
    },
}
impl SharedGesture {
    fn press(&self) -> &Gesture {
        match self {
            Self::Region(g) | Self::Drag { press: g, .. } => g,
        }
    }
}

pub(in crate::tools) struct ManualMeshSession {
    modes: Vec<ManualMeshMode>,
    active: usize,
    hovered: Option<usize>,
    shared: Option<SharedGesture>,
    sync_insert: bool,
    stitch: bool,
    sync_click: bool,
    glue_target: Option<String>,
    retired_glue_target: Option<String>,
}
impl Deref for ManualMeshSession {
    type Target = ManualMeshMode;
    fn deref(&self) -> &Self::Target {
        &self.modes[self.active]
    }
}
impl DerefMut for ManualMeshSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.modes[self.active]
    }
}

impl ManualMeshSession {
    pub fn new(targets: Vec<ManualMeshTarget>, document: &Document) -> Self {
        Self {
            modes: targets
                .into_iter()
                .map(|t| ManualMeshMode::new(t, document))
                .collect(),
            active: 0,
            hovered: None,
            shared: None,
            sync_insert: false,
            stitch: false,
            sync_click: false,
            glue_target: None,
            retired_glue_target: None,
        }
    }
    pub fn targets(&self) -> Vec<ManualMeshTarget> {
        self.modes.iter().map(|m| m.target.clone()).collect()
    }
    pub fn editable(&self) -> bool {
        self.modes.iter().all(|m| m.blocked.is_none())
    }
    pub fn bind_viewport(&mut self, viewport: crate::ViewportId) {
        for mode in &mut self.modes {
            mode.edit_viewport.get_or_insert(viewport);
        }
    }
    pub fn set_tool(&mut self, tool: ManualTool) {
        for mode in &mut self.modes {
            mode.tool = tool;
        }
    }
    pub fn dragging(&self) -> bool {
        self.shared.is_some() || self.modes.iter().any(ManualMeshMode::dragging)
    }
    pub fn viewport(&self) -> Option<crate::ViewportId> {
        self.shared
            .as_ref()
            .map(|g| g.press().version.binding.viewport)
            .or_else(|| self.modes[self.active].viewport())
    }
    pub fn accepts_frame(&self, version: FrameVersion) -> bool {
        self.shared.as_ref().map_or_else(
            || self.modes[self.active].accepts_frame(version),
            |g| {
                version.binding.document == g.press().version.binding.document
                    && version.scene >= g.press().version.scene
            },
        )
    }
    fn retain_context(&self, document: &mut Document) {
        let objects: Vec<_> = self.modes.iter().map(|m| m.target.object.clone()).collect();
        document.retain_manual_mesh_context(&objects, &self.target.object);
    }
    pub fn reconcile(&mut self, document: &mut Document) {
        self.retain_context(document);
        if self
            .glue_target
            .as_deref()
            .is_some_and(|id| document.model().glue(id).is_none())
        {
            self.retired_glue_target = self.glue_target.take();
        }
        if self.stitch
            && self.glue_target.is_none()
            && self
                .retired_glue_target
                .as_deref()
                .is_some_and(|id| document.model().glue(id).is_some())
        {
            self.glue_target = self.retired_glue_target.take();
        }
        if self.shared.is_some()
            && self
                .modes
                .iter()
                .any(|mode| document.validate_manual_target(&mode.target).is_err())
        {
            self.cancel(document);
        }
        for mode in &mut self.modes {
            mode.reconcile(document);
        }
        if !self.editable() {
            self.cancel(document);
            self.active = self
                .modes
                .iter()
                .position(|mode| mode.blocked.is_some())
                .unwrap();
        }
    }
    pub fn refresh_source(&mut self, document: &Document) -> Result<()> {
        for mode in &mut self.modes {
            mode.refresh_source(document)?;
        }
        Ok(())
    }
    pub fn cancel(&mut self, document: &mut Document) -> bool {
        self.sync_click = false;
        let mut changed = self.shared.take().is_some();
        for mode in &mut self.modes {
            changed |= mode.cancel(document);
        }
        self.hovered = None;
        changed
    }
    pub fn settle(&mut self, document: &mut Document) -> Result<bool> {
        if self.shared.take().is_some() {
            let changed = document.has_manual_mesh_gesture();
            document.finish_manual_mesh_gesture()?;
            if changed {
                self.refresh_source(document)?;
            }
            return Ok(true);
        }
        let changed = self.modes[self.active].settle(document)?;
        if changed {
            self.refresh_source(document)?;
        }
        Ok(changed)
    }
    pub fn select_all(&mut self, document: &Document) {
        for mode in &mut self.modes {
            mode.select_all(document);
        }
    }
    pub fn delete_selection(&mut self, document: &mut Document) -> Result<bool> {
        if !self.editable() {
            return Ok(false);
        }
        if self.modes.len() == 1 {
            return self.modes[0].delete_selection(document);
        }
        let operations = self
            .modes
            .iter()
            .filter_map(|mode| {
                let op = if !mode.selected.is_empty() {
                    Some(ManualMeshOperation::DeleteVertices {
                        ids: mode.selected.clone(),
                    })
                } else {
                    mode.selected_edge
                        .map(|[a, b]| ManualMeshOperation::DeleteEdge { a, b })
                }?;
                Some((mode.target.clone(), op))
            })
            .collect::<Vec<_>>();
        if operations.is_empty() {
            return Ok(false);
        }
        document.preview_manual_mesh_batch(operations)?;
        document.finish_manual_mesh_gesture()?;
        self.refresh_source(document)?;
        for mode in &mut self.modes {
            mode.selected.clear();
            mode.selected_edge = None;
        }
        Ok(true)
    }
    fn shown(&self, index: usize, frame: &CanvasFrame) -> bool {
        let object = &self.modes[index].target.object;
        frame.geometry.mesh_visible(&object.id) && frame.geometry.in_isolation_scope(object)
    }
    fn pick(&self, frame: &CanvasFrame, point: ViewportPoint) -> Option<usize> {
        let mut best: Option<(u8, f32, usize, usize)> = None;
        for (index, mode) in self.modes.iter().enumerate() {
            if !self.shown(index, frame) {
                continue;
            }
            let Some(mesh) = frame.geometry.mesh(&mode.target.object.id) else {
                continue;
            };
            let rank = match mode.hit(frame, point) {
                Hit::Vertex(id) => (
                    0,
                    mesh.vertices
                        .iter()
                        .find(|v| v.0 == id)
                        .map_or(f32::INFINITY, |v| {
                            crate::picking::distance2(point, frame.transform.to_viewport(v.1))
                        }),
                ),
                Hit::Edge([a, b], _) => {
                    let at = |id| {
                        frame
                            .transform
                            .to_viewport(mesh.vertices.iter().find(|v| v.0 == id).unwrap().1)
                    };
                    (1, crate::picking::segment_distance2(point, at(a), at(b)))
                }
                Hit::Empty => {
                    let p = frame.transform.to_model(point);
                    let topology = &frame.geometry.topology.meshes[&mode.target.object.id];
                    let contains = mesh.triangles.iter().any(|ids| {
                        let vertices = ids.map(|id| mesh.vertices[topology.slots[&id]].1);
                        crate::picking::barycentric(p, vertices).is_some()
                    });
                    if !contains {
                        continue;
                    }
                    (2, 0.)
                }
            };
            let order = frame
                .geometry
                .meshes
                .iter()
                .position(|m| m.id == mode.target.object.id)
                .unwrap_or(0);
            let candidate = (rank.0, rank.1, usize::MAX - order, index);
            if best.is_none_or(|b| candidate < b) {
                best = Some(candidate);
            }
        }
        best.map(|v| v.3)
    }
    pub fn target_at(&self, frame: &CanvasFrame, point: ViewportPoint) -> &ManualMeshTarget {
        &self.modes[self.pick(frame, point).unwrap_or(self.active)].target
    }
    pub fn activate_at(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
        point: ViewportPoint,
    ) -> bool {
        if self.dragging() {
            return false;
        }
        let next = self.pick(frame, point).unwrap_or(self.active);
        let changed = next != self.active;
        self.active = next;
        self.retain_context(document);
        changed
    }
    pub fn edge_at(&self, frame: &CanvasFrame, point: ViewportPoint) -> Option<ManualMeshEdge> {
        if self.dragging() {
            return None;
        }
        self.modes[self.pick(frame, point).unwrap_or(self.active)].edge_at(frame, point)
    }
    pub fn input(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
        input: CanvasInput,
    ) -> Result<bool> {
        if matches!(input, CanvasInput::Cancel) {
            return Ok(self.cancel(document));
        }
        if !self.editable()
            || self
                .edit_viewport
                .is_some_and(|v| v != frame.version.binding.viewport)
        {
            return Ok(false);
        }
        if self.modes.len() == 1 {
            match input {
                CanvasInput::Move { point, .. } if !self.dragging() => {
                    self.hovered = self.pick(frame, point);
                }
                CanvasInput::Leave => self.hovered = None,
                _ => {}
            }
            return self.modes[0].input(document, frame, input);
        }
        match input {
            CanvasInput::Down {
                point,
                shift,
                alt,
                primary,
            } => {
                self.cancel(document);
                self.sync_click = self.sync_insert
                    && !self.source_view
                    && !primary
                    && !shift
                    && !alt
                    && self.tool != ManualTool::Delete;
                self.active = self.pick(frame, point).unwrap_or(self.active);
                self.hovered = Some(self.active);
                let hit = self.modes[self.active].hit(frame, point);
                if !primary
                    && matches!(self.tool, ManualTool::Select | ManualTool::Lasso)
                    && !shift
                    && !alt
                    && matches!(hit, Hit::Vertex(id) if !self.selected.contains(&id))
                {
                    for (index, mode) in self.modes.iter_mut().enumerate() {
                        if index != self.active {
                            mode.selected.clear();
                            mode.selected_edge = None;
                        }
                    }
                }
                let selection: Vec<_> = self
                    .modes
                    .iter()
                    .map(|m| (m.selected.clone(), m.selected_edge))
                    .collect();
                self.retain_context(document);
                self.modes[self.active].input(document, frame, input)?;
                let Some(gesture) = self.modes[self.active].gesture.take() else {
                    return Ok(false);
                };
                match gesture.kind {
                    GestureKind::Region { .. } => {
                        for (mode, (selected, edge)) in self.modes.iter_mut().zip(selection) {
                            mode.selected = selected;
                            mode.selected_edge = edge;
                        }
                        self.shared = Some(SharedGesture::Region(gesture));
                    }
                    GestureKind::Drag(_)
                        if self.modes.iter().filter(|m| !m.selected.is_empty()).count() > 1 =>
                    {
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
                        self.shared = Some(SharedGesture::Drag {
                            press: gesture,
                            meshes,
                        });
                    }
                    _ => self.modes[self.active].gesture = Some(gesture),
                }
                Ok(false)
            }
            CanvasInput::Move { point, left_down } if self.shared.is_some() => {
                if !left_down {
                    return Ok(self.cancel(document));
                }
                self.update_shared(document, point)
            }
            CanvasInput::Up { point }
                if self.sync_click
                    && self
                        .shared
                        .as_ref()
                        .map(SharedGesture::press)
                        .or(self.gesture.as_ref())
                        .is_some_and(|g| {
                            !g.moved && crate::picking::distance2(g.start, point) < 9.
                        }) =>
            {
                let start = self
                    .shared
                    .as_ref()
                    .map(SharedGesture::press)
                    .or(self.gesture.as_ref())
                    .unwrap()
                    .start;
                self.cancel(document);
                match self.insert_overlap(document, frame, start) {
                    Ok(changed) => {
                        self.diagnostic = None;
                        Ok(changed)
                    }
                    Err(error) => {
                        document.cancel_manual_mesh_gesture();
                        self.diagnostic = Some(error.to_string());
                        Ok(false)
                    }
                }
            }
            CanvasInput::Up { point } if self.shared.is_some() => {
                let mut changed = self.update_shared(document, point)?;
                let gesture = self.shared.take().unwrap();
                match gesture {
                    SharedGesture::Drag { .. } => {
                        changed |= document.has_manual_mesh_gesture();
                        document.finish_manual_mesh_gesture()?;
                        if changed {
                            self.refresh_source(document)?;
                        }
                    }
                    SharedGesture::Region(mut gesture) => {
                        gesture.update(point, true);
                        if gesture.moved {
                            let (operation, region) = match &gesture.kind {
                                GestureKind::Region {
                                    path, operation, ..
                                } => (*operation, path.region(gesture.start, point)),
                                _ => unreachable!(),
                            };
                            if let Some(region) = region {
                                for index in 0..self.modes.len() {
                                    if !self.shown(index, frame) {
                                        continue;
                                    }
                                    let mode = &mut self.modes[index];
                                    let ids = frame
                                        .geometry
                                        .mesh(&mode.target.object.id)
                                        .into_iter()
                                        .flat_map(|m| &m.vertices)
                                        .filter_map(|&(id, p)| {
                                            region
                                                .contains(gesture.transform.to_viewport(p))
                                                .then_some(id)
                                        })
                                        .collect();
                                    operation.apply(&mut mode.selected, ids);
                                    mode.selected_edge = None;
                                }
                            }
                        } else {
                            let replace = matches!(
                                gesture.kind,
                                GestureKind::Region {
                                    operation: SelectionOperation::Replace,
                                    ..
                                }
                            );
                            if replace {
                                for (index, mode) in self.modes.iter_mut().enumerate() {
                                    if index != self.active {
                                        mode.selected.clear();
                                        mode.selected_edge = None;
                                    }
                                }
                            }
                            self.modes[self.active].gesture = Some(gesture);
                            changed = self.modes[self.active].input(document, frame, input)?;
                        }
                    }
                }
                Ok(changed)
            }
            CanvasInput::Move { point, .. } if !self.dragging() => {
                for mode in &mut self.modes {
                    mode.hover = Hit::Empty;
                }
                self.hovered = self.pick(frame, point);
                if let Some(index) = self.hovered {
                    self.modes[index].input(document, frame, input)?;
                }
                Ok(false)
            }
            CanvasInput::Up { point } => {
                if self.gesture.as_ref().is_some_and(|g| {
                    matches!(
                        g.kind,
                        GestureKind::Connect(_) | GestureKind::Create(Hit::Vertex(_))
                    )
                }) && self
                    .pick(frame, point)
                    .is_some_and(|index| index != self.active)
                {
                    self.cancel(document);
                    self.diagnostic = Some("Connections must stay within one mesh".into());
                    return Ok(false);
                }
                self.modes[self.active].input(document, frame, input)
            }
            CanvasInput::Leave => {
                for mode in &mut self.modes {
                    mode.hover = Hit::Empty;
                }
                self.hovered = None;
                Ok(false)
            }
            _ => self.modes[self.active].input(document, frame, input),
        }
    }
    fn update_shared(&mut self, document: &mut Document, point: ViewportPoint) -> Result<bool> {
        let Some(shared) = &mut self.shared else {
            return Ok(false);
        };
        match shared {
            SharedGesture::Region(gesture) => {
                gesture.update(point, false);
                Ok(false)
            }
            SharedGesture::Drag { press, meshes } => {
                press.update(point, false);
                if !press.moved {
                    return Ok(false);
                }
                let before = press.transform.to_model(press.start);
                let after = press.transform.to_model(point);
                let delta = ModelVector {
                    x: after.x - before.x,
                    y: after.y - before.y,
                };
                let selected: Vec<_> = meshes
                    .iter()
                    .flat_map(|(index, starts, _)| {
                        starts
                            .iter()
                            .map(|v| (self.modes[*index].target.object.id.clone(), v.id))
                    })
                    .collect();
                let operations = meshes
                    .iter()
                    .map(|(index, starts, pose)| -> Result<_> {
                        let mode = &self.modes[*index];
                        let targets = pose.move_targets_with_selection(starts, delta, &selected)?;
                        let model = document.model();
                        let mesh = model.mesh(&mode.target.object.id).unwrap();
                        let current: HashMap<_, _> = mesh
                            .vertex_ids
                            .iter()
                            .copied()
                            .zip(mesh.uvs.iter().copied())
                            .collect();
                        let motion_from = targets
                            .iter()
                            .map(|v| ManualVertexTarget {
                                id: v.id,
                                uv: current[&v.id],
                            })
                            .collect();
                        Ok((
                            mode.target.clone(),
                            ManualMeshOperation::MoveVertices {
                                targets,
                                motion_from: Some(motion_from),
                            },
                        ))
                    })
                    .collect::<Result<Vec<_>>>();
                let result = operations.and_then(|ops| document.preview_manual_mesh_batch(ops));
                match result {
                    Ok(changed) => {
                        self.diagnostic = None;
                        Ok(changed)
                    }
                    Err(error) => {
                        self.diagnostic = Some(error.to_string());
                        Ok(false)
                    }
                }
            }
        }
    }
    pub fn overlay(&self, frame: &CanvasFrame, shift: bool, alt: bool) -> OverlaySnapshot {
        let mut out = self.modes[self.active].overlay(frame, shift, alt);
        out.hovered_object_label = self
            .hovered
            .filter(|&index| {
                self.editable()
                    && self.shown(index, frame)
                    && self
                        .edit_viewport
                        .is_none_or(|v| v == frame.version.binding.viewport)
            })
            .map(|index| {
                frame
                    .geometry
                    .index
                    .name(&self.modes[index].target.object)
                    .to_owned()
            });
        for (index, mode) in self.modes.iter().enumerate() {
            if index == self.active || !self.shown(index, frame) {
                continue;
            }
            let other = mode.overlay(frame, shift, alt);
            out.vertices.extend(other.vertices);
            out.lines.extend(other.lines);
            out.automatic_lines.extend(other.automatic_lines);
            out.accent_lines.extend(other.accent_lines);
            out.hover_lines.extend(other.hover_lines);
            out.glues.extend(other.glues);
            if self.hovered == Some(index) {
                out.manual_mesh_feedback = other.manual_mesh_feedback;
                out.manual_mesh_delete_unavailable = other.manual_mesh_delete_unavailable;
                out.selection_operation = other.selection_operation;
            }
        }
        if let Some(shared) = &self.shared
            && shared.press().version.binding == frame.version.binding
        {
            out.active_gesture = Some(shared.press().kind());
            out.selection_operation = shared.press().selection_operation();
            if let SharedGesture::Region(g) = shared {
                g.selection_overlay(&mut out);
            }
        }
        out
    }
}

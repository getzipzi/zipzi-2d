//! Tool state is independent of GPUI and uses fixed source-space transaction targets.
use crate::{CanvasPoint, Document, ModelPoint, ViewportTransform};
use anyhow::{Result, ensure};
use kasane_core::VertexId;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMode {
    BaseMesh,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolId {
    Vertices,
}
#[derive(Clone, Debug)]
pub struct MeshGeometry {
    pub id: String,
    pub vertices: Vec<(VertexId, ModelPoint)>,
    pub triangles: Vec<[VertexId; 3]>,
    pub unavailable: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameVersion {
    pub document: crate::DocumentId,
    pub scene: u64,
    pub viewport: u64,
    pub request: u64,
}
#[derive(Clone, Debug)]
pub struct CanvasFrame {
    pub version: FrameVersion,
    pub transform: ViewportTransform,
    pub geometry: Arc<[MeshGeometry]>,
}
#[derive(Clone, Debug)]
pub struct OverlayVertex {
    pub id: VertexId,
    pub position: CanvasPoint,
    pub selected: bool,
    pub hovered: bool,
}
#[derive(Clone, Debug, Default)]
pub struct OverlaySnapshot {
    pub vertices: Vec<OverlayVertex>,
    pub lines: Vec<(CanvasPoint, CanvasPoint)>,
    pub unavailable: Option<String>,
}
struct Drag {
    anchor: ModelPoint,
    original: Vec<(VertexId, ModelPoint)>,
}
pub struct ToolController {
    pub mode: EditMode,
    pub active: ToolId,
    selected: BTreeSet<VertexId>,
    hovered: Option<VertexId>,
    drag: Option<Drag>,
}
impl Default for ToolController {
    fn default() -> Self {
        Self {
            mode: EditMode::BaseMesh,
            active: ToolId::Vertices,
            selected: BTreeSet::new(),
            hovered: None,
            drag: None,
        }
    }
}
impl ToolController {
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn clear(&mut self) {
        self.selected.clear();
        self.hovered = None;
    }
    pub fn reconcile(&mut self, document: &Document) {
        let ids = document.selected_vertex_ids();
        self.selected.retain(|id| ids.contains(id));
        if self.hovered.is_some_and(|id| !ids.contains(&id)) {
            self.hovered = None;
        }
    }
    pub fn cancel(&mut self, document: &mut Document) -> bool {
        if self.drag.take().is_some() {
            document.cancel_edit();
            return true;
        }
        false
    }
    fn hit(
        geometry: &MeshGeometry,
        transform: ViewportTransform,
        point: CanvasPoint,
    ) -> Option<VertexId> {
        geometry
            .vertices
            .iter()
            .filter_map(|(id, p)| {
                let p = transform.to_canvas(*p);
                let distance = (p.x - point.x).powi(2) + (p.y - point.y).powi(2);
                (distance <= 8f32.powi(2)).then_some((*id, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            .map(|(id, _)| id)
    }
    pub fn hover(
        &mut self,
        geometry: &MeshGeometry,
        transform: ViewportTransform,
        point: CanvasPoint,
    ) {
        self.hovered = Self::hit(geometry, transform, point);
    }
    pub fn begin(
        &mut self,
        document: &mut Document,
        geometry: &MeshGeometry,
        transform: ViewportTransform,
        point: CanvasPoint,
        shift: bool,
    ) -> Result<()> {
        ensure!(
            geometry.unavailable.is_none(),
            "{}",
            geometry.unavailable.as_deref().unwrap_or("")
        );
        self.cancel(document);
        let Some(id) = Self::hit(geometry, transform, point) else {
            self.clear();
            return Ok(());
        };
        if shift && self.selected.remove(&id) {
            return Ok(());
        }
        if !self.selected.contains(&id) {
            if !shift {
                self.selected.clear();
            }
            self.selected.insert(id);
        }
        document.begin_vertices()?;
        let original = document
            .selected_vertex_positions()
            .into_iter()
            .filter(|(id, _)| self.selected.contains(id))
            .collect();
        self.drag = Some(Drag {
            anchor: transform.to_model(point),
            original,
        });
        Ok(())
    }
    pub fn update(
        &mut self,
        document: &mut Document,
        transform: ViewportTransform,
        point: CanvasPoint,
    ) -> Result<bool> {
        let Some(drag) = &self.drag else {
            return Ok(false);
        };
        let p = transform.to_model(point);
        let dx = p.x - drag.anchor.x;
        let dy = p.y - drag.anchor.y;
        let positions = drag
            .original
            .iter()
            .map(|(_, v)| ModelPoint {
                x: v.x + dx,
                y: v.y + dy,
            })
            .collect::<Vec<_>>();
        // Clicks and duplicate events affect only selection. Compare against the
        // current candidate so returning to the anchor still restores positions.
        let current = document
            .selected_vertex_positions()
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();
        if drag
            .original
            .iter()
            .zip(&positions)
            .all(|((id, _), p)| current.get(id) == Some(p))
        {
            return Ok(false);
        }
        document.preview_vertices(
            &drag.original.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            &positions,
        )?;
        Ok(true)
    }
    pub fn finish(
        &mut self,
        document: &mut Document,
        transform: ViewportTransform,
        point: CanvasPoint,
    ) -> Result<bool> {
        if self.drag.is_none() {
            return Ok(false);
        }
        let result = self.update(document, transform, point);
        self.drag = None;
        let changed = match result {
            Ok(changed) => changed,
            Err(error) => {
                document.cancel_edit();
                return Err(error);
            }
        };
        document.finish_edit()?;
        Ok(changed)
    }
    pub fn overlay(
        &self,
        geometry: Option<&MeshGeometry>,
        transform: ViewportTransform,
    ) -> OverlaySnapshot {
        let Some(g) = geometry else {
            return OverlaySnapshot::default();
        };
        let vertices = g
            .vertices
            .iter()
            .map(|(id, p)| OverlayVertex {
                id: *id,
                position: transform.to_canvas(*p),
                selected: self.selected.contains(id),
                hovered: self.hovered == Some(*id),
            })
            .collect::<Vec<_>>();
        let lookup: std::collections::HashMap<_, _> =
            vertices.iter().map(|v| (v.id, v.position)).collect();
        let mut edges = BTreeSet::new();
        for triangle in &g.triangles {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                edges.insert((a.min(b), a.max(b)));
            }
        }
        let lines = edges
            .into_iter()
            .filter_map(|(a, b)| Some((*lookup.get(&a)?, *lookup.get(&b)?)))
            .collect();
        OverlaySnapshot {
            vertices,
            lines,
            unavailable: g.unavailable.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum CanvasInput {
    Down { point: CanvasPoint, shift: bool },
    Move { point: CanvasPoint, left_down: bool },
    Up { point: CanvasPoint },
    Cancel,
}

//! Canvas interaction state machine. Object selection, component selection and
//! write targets are independent; a click never opens a model transaction.
mod access;
mod brush_select;
mod controls;
mod gesture;
mod glue;
mod hand;
mod hints;
mod inflate;
mod input;
mod lasso;
mod manual_mesh;
mod overlay;
mod region;
mod select;
mod stitch;
mod transform_constraints;
pub use brush_select::{BrushSelectionSettings, BrushShape};
pub use glue::{GlueBrushMode, GluePaintControls, OverlayGlueLabel, OverlayGlueWeight};
pub use hints::{HintAction, InteractionHint};
pub use inflate::BrushMode;
pub use manual_mesh::{ManualGlueControls, ManualMeshEdge, ManualMeshFeedback, ManualTool};

use crate::{
    CanvasScene, ComponentRef, Document, HitTarget, ObjectKind, ObjectRef, ViewportPoint,
    ViewportTransform,
};
use gesture::Gesture;
use std::sync::Arc;
use zipzi_core::state::DocumentState;

pub use overlay::{
    CanvasGestureKind, CanvasSelectionOperation, GluePointState, OverlayDeformer, OverlayGlue,
    OverlayGluePoint, OverlayHandle, OverlayRotationBone, OverlaySelectionBrush,
    OverlaySelectionWeights, OverlaySnapshot, OverlayVertex, OverlayVertexKind,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolId {
    #[default]
    Select,
    Lasso,
    BrushSelect,
    Glue,
    Hand,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeformerVisibility {
    pub hide_unselected_warps: bool,
    pub hide_unselected_rotations: bool,
    pub ancestors: bool,
    pub all: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameVersion {
    pub binding: crate::ViewportBinding,
    pub scene: u64,
    pub viewport: u64,
    pub request: u64,
}
#[derive(Clone, Debug)]
pub struct CanvasFrame {
    pub version: FrameVersion,
    pub transform: ViewportTransform,
    pub geometry: Arc<CanvasScene>,
}
#[derive(Default)]
pub struct ToolController {
    pub(crate) hidden_deformer_viewports:
        std::collections::HashSet<(crate::ViewportId, crate::ControlKind)>,
    pub active: ToolId,
    pub deformer_visibility: DeformerVisibility,
    pub deformer_grid: bool,
    pub warp_whole: bool,
    pub original_points: bool,
    pub(crate) brush_selection: BrushSelectionSettings,
    pub(crate) brush_mode: BrushMode,
    pub(crate) glue_brush: BrushSelectionSettings,
    pub(crate) glue_brush_mode: GlueBrushMode,
    pub(crate) component_weights: std::collections::HashMap<ComponentRef, f32>,
    pub(crate) pointer: Option<(crate::ViewportBinding, ViewportPoint)>,
    pub(crate) components: Vec<ComponentRef>,
    pub(crate) warp_frames: std::collections::HashMap<
        (crate::ViewportId, ObjectRef),
        crate::warp_frame::WarpFrameState,
    >,
    hovered: Option<HitTarget>,
    hovered_binding: Option<crate::ViewportBinding>,
    alt: bool,
    shift: bool,
    gesture: Option<Gesture>,
    manual: Option<manual_mesh::ManualMeshSession>,
    stitch: Option<stitch::StitchSession>,
    access_cache: std::sync::Mutex<Option<access::AccessCache>>,
    #[cfg(test)]
    access_builds: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    overlay_builds: std::sync::atomic::AtomicUsize,
}
impl ToolController {
    pub(crate) fn deformer_visibility_for(&self, frame: &CanvasFrame) -> DeformerVisibility {
        DeformerVisibility {
            hide_unselected_warps: self
                .hidden_deformer_viewports
                .contains(&(frame.version.binding.viewport, crate::ControlKind::Warp)),
            hide_unselected_rotations: self
                .hidden_deformer_viewports
                .contains(&(frame.version.binding.viewport, crate::ControlKind::Rotation)),
            ..self.deformer_visibility
        }
    }
    pub(crate) fn clear_hover(&mut self) {
        self.hovered = None;
        self.hovered_binding = None;
        self.pointer = None;
    }
    pub(crate) fn mesh_vertex_drag(&self) -> bool {
        self.manual.is_none()
            && matches!(&self.gesture,
            Some(Gesture::Controls(g)) if matches!(&g.target,
                HitTarget::Component(c) if c.object.kind == crate::ObjectKind::Mesh))
    }
    fn rotation_length_target(&self) -> Option<&crate::ObjectRef> {
        match &self.gesture {
            Some(Gesture::Controls(g)) if g.adjusting_bone_length() => Some(g.target.object()),
            Some(_) => None,
            None if self.alt => self.hovered.as_ref().and_then(|hit| match hit {
                HitTarget::DeformerHandle(object, crate::DeformerHandle::RotationScale) => {
                    Some(object)
                }
                _ => None,
            }),
            None => None,
        }
    }
    pub(crate) fn set_gesture_modifiers(&mut self, shift: bool, alt: bool) -> bool {
        let changed = self.alt != alt || self.shift != shift;
        self.alt = alt;
        self.shift = shift;
        self.gesture
            .as_mut()
            .is_some_and(|g| g.set_modifiers(shift, alt))
            | changed
    }
    pub(crate) fn warp_selection_gizmo(
        &self,
        document: &Document,
        frame: &CanvasFrame,
    ) -> Option<crate::deformer::WarpGizmos> {
        if let Some(Gesture::Controls(gesture)) = &self.gesture {
            if let Some((object, info)) = gesture.frame_preview() {
                if gesture.press.version.binding.viewport == frame.version.binding.viewport {
                    let mut gizmos = gesture.press.warp_selection.clone().unwrap_or_default();
                    if let Some((_, gizmo)) = gizmos.items.iter_mut().find(|(o, _)| o == object) {
                        *gizmo = if gizmos.objects.is_empty() {
                            info.frame.gizmo(frame.transform)
                        } else {
                            info.frame.object_gizmo(frame.transform)
                        };
                    }
                    return Some(gizmos);
                }
            }
        }
        if let Some(frame) = self.object_selection_frame(document, frame) {
            return Some(frame);
        }
        let components: Vec<_> = self
            .components
            .iter()
            .filter(|c| {
                document.context().selections.contains(&c.object)
                    && frame.geometry.in_isolation_scope(&c.object)
                    && frame.geometry.control(&c.object.id).is_some_and(|control| {
                        control.kind == crate::ControlKind::Warp
                            && frame.geometry.control_visible(
                                control,
                                &document.context().selections,
                                self.active,
                                self.deformer_visibility_for(frame),
                            )
                    })
            })
            .cloned()
            .collect();
        let selection = (!self.warp_whole)
            .then(|| {
                frame
                    .geometry
                    .warp_selection_gizmo(&components, frame.transform)
            })
            .flatten();
        let local = selection.is_some();
        let items = if let Some(selection) = selection {
            vec![(selection.object, selection.gizmo)]
        } else {
            frame
                .geometry
                .controls
                .iter()
                .filter(|c| {
                    c.kind == crate::ControlKind::Warp
                        && document.context().selections.contains(&c.object)
                        && frame.geometry.control_visible(
                            c,
                            &document.context().selections,
                            self.active,
                            self.deformer_visibility_for(frame),
                        )
                })
                .filter_map(|c| {
                    frame
                        .geometry
                        .deformer_gizmo(c, frame.transform)
                        .map(|g| (c.object.clone(), g))
                })
                .collect()
        };
        Some(crate::deformer::WarpGizmos {
            objects: vec![],
            local,
            adjusting: document
                .context()
                .selections
                .iter()
                .filter(|object| {
                    self.warp_frame_info(document, frame, object)
                        .is_some_and(|info| info.adjusting)
                })
                .cloned()
                .collect(),
            items: items
                .into_iter()
                .filter_map(|(object, gizmo)| {
                    if let Some(state) =
                        self.warp_frame_state(document, frame.version.binding.viewport, &object)
                    {
                        state
                            .info
                            .visible
                            .then(|| (object, state.info.frame.gizmo(frame.transform)))
                    } else {
                        Some((object, gizmo))
                    }
                })
                .collect(),
        })
    }
    pub(crate) fn hovered_binding(&self) -> Option<crate::ViewportBinding> {
        self.hovered_binding
    }
    pub(crate) fn interaction_viewport(&self) -> Option<crate::ViewportId> {
        if let Some(stitch) = &self.stitch {
            return stitch.press.map(|_| stitch.viewport);
        }
        if let Some(manual) = &self.manual {
            return manual.viewport();
        }
        self.gesture
            .as_ref()
            .map(|press| press.press().version.binding.viewport)
    }
    pub(crate) fn interaction_hints(&self) -> &'static [InteractionHint] {
        if self.stitch.is_some() {
            return &[];
        }
        if self.gesture.is_none()
            && matches!(
                self.hovered,
                Some(HitTarget::DeformerHandle(
                    _,
                    crate::DeformerHandle::RotationScale
                ))
            )
        {
            return controls::BONE_TIP_HINTS;
        }
        self.gesture
            .as_ref()
            .map_or(controls::HINTS, Gesture::hints)
    }
    pub(crate) fn idle_hints(&self) -> &'static [InteractionHint] {
        if self.stitch.is_some() {
            return &[];
        }
        if self.active == ToolId::BrushSelect
            && self.brush_mode == BrushMode::Deform
            && !self.manual_active()
        {
            return brush_select::DEFORM_HINTS;
        }
        if self.active == ToolId::BrushSelect
            && self.brush_mode == BrushMode::Inflate
            && !self.manual_active()
        {
            return inflate::HINTS;
        }
        if matches!(
            self.hovered,
            Some(HitTarget::DeformerHandle(
                _,
                crate::DeformerHandle::RotationScale
            ))
        ) {
            controls::BONE_TIP_HINTS
        } else {
            self.active.idle_hints()
        }
    }
    pub(crate) fn reconcile_component_weights(&mut self) {
        let selected: std::collections::HashSet<_> = self.components.iter().collect();
        self.component_weights.retain(|c, _| selected.contains(c));
    }
    pub fn dragging(&self) -> bool {
        self.stitch.as_ref().is_some_and(|s| s.press.is_some())
            || self.gesture.is_some()
            || self.manual.as_ref().is_some_and(|mode| mode.dragging())
    }
    /// Completed poses from this control edit can lag subsequent pointer moves.
    /// The press fixes the authoring target; new gestures still require a current frame.
    pub(crate) fn accepts_drag_frame(&self, version: FrameVersion) -> bool {
        if let Some(stitch) = &self.stitch {
            return stitch
                .press
                .is_some_and(|(v, _)| v.binding == version.binding);
        }
        if let Some(manual) = &self.manual {
            return manual.accepts_frame(version);
        }
        self.gesture
            .as_ref()
            .is_some_and(|gesture| gesture.accepts_frame(version))
    }

    /// Clear ordinary tool projections. Document modes have an explicit lifecycle.
    pub fn clear_selection(&mut self) {
        self.components.clear();
        self.component_weights.clear();
        self.warp_frames.clear();
        self.hovered = None;
        self.hovered_binding = None;
        self.pointer = None;
    }
    pub(crate) fn retain_history_selection(
        &mut self,
        document: &Document,
        before: &DocumentState,
        after: &DocumentState,
    ) {
        if before.content_id() == after.content_id() {
            return;
        }
        self.clear_hover();
        self.warp_frames.clear();
        let objects = document.object_state_index();
        let mut compatible = std::collections::HashMap::new();
        // Geometry history keeps the current editor selection. A topology edit
        // can reuse a vertex ID or lattice slot for an entirely different point.
        self.components.retain(|component| {
            if let Some(&keep) = compatible.get(&component.object) {
                return keep;
            }
            let keep = objects.state(&component.object).visible()
                && same_component_layout(before, after, &component.object);
            compatible.insert(component.object.clone(), keep);
            keep
        });
    }
    pub fn reconcile(&mut self, document: &mut Document) {
        if matches!(&self.gesture, Some(Gesture::Glue(g)) if !g.current(document)) {
            self.cancel(document);
        }
        if let Some(mode) = &mut self.manual {
            mode.reconcile(document);
        }
        if let Some(stitch) = &mut self.stitch {
            if let Some(engine) = &mut stitch.engine {
                if engine
                    .targets()
                    .iter()
                    .any(|t| document.validate_manual_target(t).is_err())
                {
                    stitch.press = None;
                }
                engine.reconcile(document);
            }
        }
        let locks = document.object_state_index();
        let model = document.model();
        self.components.retain(|c| {
            !locks.is_locked(&c.object)
                && document.context().selections.contains(&c.object)
                && match c.object.kind {
                    ObjectKind::Mesh => model.mesh(&c.object.id).is_some(),
                    ObjectKind::Deformer => {
                        model.transform(&c.object.id).is_some()
                            && document.deformer_controls_visible(&c.object.id)
                    }
                    _ => false,
                }
        });
        self.hovered = None;
        self.hovered_binding = None;
        self.reconcile_component_weights();
        self.prune_warp_frames(document);
    }
    pub fn cancel(&mut self, document: &mut Document) -> bool {
        if let Some(stitch) = &mut self.stitch {
            stitch.press = None;
        }
        let gesture = self.gesture.take();
        let original = gesture.as_ref().and_then(|g| {
            if let Gesture::Controls(g) = g {
                g.original_frame()
                    .map(|(o, i)| (g.press.version.binding.viewport, o.clone(), i))
            } else {
                None
            }
        });
        let brush = if self.active == ToolId::Glue {
            &mut self.glue_brush
        } else {
            &mut self.brush_selection
        };
        let changed = gesture.is_some_and(|g| {
            g.cancel(
                document,
                &mut self.components,
                brush,
                &mut self.component_weights,
            )
        }) | self
            .manual
            .as_mut()
            .is_some_and(|mode| mode.cancel(document));
        if let Some((viewport, object, info)) = original {
            self.set_warp_frame(document, viewport, object, info);
        }
        self.hovered = None;
        self.hovered_binding = None;
        self.pointer = None;
        changed
    }
    pub fn set_tool(&mut self, tool: ToolId, document: &mut Document) -> bool {
        let changed = self.cancel(document);
        self.stitch = None;
        let preserve_components = self.active.preserves_components(tool);
        self.active = tool;
        if !preserve_components {
            self.clear_selection();
        } else if tool == ToolId::BrushSelect {
            let model = document.model();
            self.components
                .retain(|component| match component.object.kind {
                    ObjectKind::Mesh => true,
                    ObjectKind::Deformer => {
                        crate::warp_bezier::component(component.id).is_none()
                            && model
                                .transform(&component.object.id)
                                .is_some_and(|transform| {
                                    matches!(transform.data, zipzi_core::TransformData::Warp(_))
                                })
                    }
                    _ => false,
                });
        }
        self.reconcile_component_weights();
        changed
    }
    pub(crate) fn begin_stitch(
        &mut self,
        document: &mut Document,
        viewport: crate::ViewportId,
    ) -> anyhow::Result<()> {
        self.exit_manual(document);
        self.clear_selection();
        self.stitch = Some(stitch::StitchSession::new(document, viewport)?);
        self.active = ToolId::Glue;
        Ok(())
    }
    pub(crate) fn stitch_viewport(&self) -> Option<crate::ViewportId> {
        self.stitch.as_ref().map(|s| s.viewport)
    }
    pub(crate) fn stitch_parts(&self) -> usize {
        self.stitch.as_ref().map_or(0, |s| s.parts.len())
    }
    pub(crate) fn stitch_controls(
        &self,
        document: &Document,
        frame: Option<&CanvasFrame>,
    ) -> Option<ManualGlueControls> {
        self.stitch
            .as_ref()?
            .engine
            .as_ref()?
            .glue_controls(document, frame)
    }
    pub(crate) fn toggle_stitch_remove(&mut self) {
        if let Some(s) = &mut self.stitch {
            s.remove = !s.remove;
        }
    }
    pub(crate) fn stitch_removing(&self) -> bool {
        self.stitch.as_ref().is_some_and(|s| s.remove)
    }
    pub fn manual_active(&self) -> bool {
        self.manual.is_some()
    }
    pub fn manual_editable(&self) -> bool {
        self.manual.as_ref().is_some_and(|mode| mode.editable())
    }
    pub fn manual_tool(&self) -> Option<ManualTool> {
        self.manual.as_ref().map(|mode| mode.tool)
    }
    pub fn manual_target(&self) -> Option<&crate::ManualMeshTarget> {
        self.manual.as_ref().map(|mode| &mode.target)
    }
    pub fn manual_targets(&self) -> Vec<crate::ManualMeshTarget> {
        self.manual
            .as_ref()
            .map_or_else(Vec::new, |mode| mode.targets())
    }
    pub(crate) fn manual_glue_controls(
        &self,
        document: &Document,
        frame: Option<&CanvasFrame>,
    ) -> Option<ManualGlueControls> {
        self.manual.as_ref()?.glue_controls(document, frame)
    }
    pub(crate) fn set_manual_glue_target(&mut self, target: Option<String>) {
        if let Some(mode) = &mut self.manual {
            mode.set_glue_target(target);
        }
    }
    pub(crate) fn toggle_manual_sync_insert(&mut self) {
        if let Some(mode) = &mut self.manual {
            mode.toggle_sync_insert();
        }
    }
    pub(crate) fn edit_manual_glue(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
        bind: bool,
    ) -> anyhow::Result<bool> {
        self.manual
            .as_mut()
            .map_or(Ok(false), |mode| mode.edit_glue(document, frame, bind))
    }
    pub fn manual_target_at(
        &self,
        frame: &CanvasFrame,
        point: ViewportPoint,
    ) -> Option<&crate::ManualMeshTarget> {
        self.manual
            .as_ref()
            .map(|mode| mode.target_at(frame, point))
    }
    pub fn activate_manual_target(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
        point: ViewportPoint,
    ) -> bool {
        self.manual
            .as_mut()
            .is_some_and(|mode| mode.activate_at(document, frame, point))
    }
    pub fn manual_edge_at(
        &self,
        frame: &CanvasFrame,
        point: crate::ViewportPoint,
    ) -> Option<crate::ManualMeshEdge> {
        self.manual.as_ref()?.edge_at(frame, point)
    }
    pub(crate) fn manual_viewport(&self) -> Option<crate::ViewportId> {
        self.manual.as_ref().and_then(|m| m.edit_viewport)
    }
    pub(crate) fn manual_uv_plane(&self, viewport: crate::ViewportId) -> bool {
        self.manual
            .as_ref()
            .is_some_and(|m| m.source_view && m.edit_viewport == Some(viewport))
    }
    pub(crate) fn bind_manual_viewport(&mut self, viewport: crate::ViewportId) {
        if let Some(m) = &mut self.manual {
            m.bind_viewport(viewport);
        }
    }
    pub fn manual_source_view(&self) -> bool {
        self.manual.as_ref().is_some_and(|mode| mode.source_view)
    }
    pub fn toggle_manual_source_view(&mut self) {
        if let Some(mode) = &mut self.manual {
            mode.source_view = !mode.source_view;
        }
    }
    pub fn refresh_manual_source(&mut self, document: &Document) -> anyhow::Result<()> {
        if let Some(mode) = &mut self.manual {
            mode.refresh_source(document)?;
        }
        Ok(())
    }
    pub fn after_manual_generation(&mut self, document: &Document) -> anyhow::Result<()> {
        if let Some(mode) = &mut self.manual {
            mode.refresh_source(document)?;
            mode.selected.clear();
            mode.selected_edge = None;
        }
        Ok(())
    }
    pub fn enter_manual(&mut self, document: &mut Document, targets: Vec<crate::ManualMeshTarget>) {
        self.cancel(document);
        self.components.clear();
        self.component_weights.clear();
        self.active = ToolId::Select;
        self.stitch = None;
        self.manual = Some(manual_mesh::ManualMeshSession::new(targets, document));
    }
    pub fn set_manual_tool(&mut self, tool: ManualTool, document: &mut Document) {
        self.cancel(document);
        if let Some(mode) = &mut self.manual {
            self.active = ToolId::Select;
            mode.set_tool(tool);
        }
    }
    pub fn exit_manual(&mut self, document: &mut Document) {
        self.cancel(document);
        self.manual = None;
    }
    pub fn settle_manual(&mut self, document: &mut Document) -> anyhow::Result<bool> {
        self.manual
            .as_mut()
            .map_or(Ok(false), |mode| mode.settle(document))
    }
    pub fn manual_select_all(&mut self, document: &Document) -> bool {
        if let Some(mode) = &mut self.manual {
            mode.select_all(document);
            return true;
        }
        false
    }
    pub fn manual_delete_selection(&mut self, document: &mut Document) -> anyhow::Result<bool> {
        self.manual
            .as_mut()
            .map_or(Ok(false), |mode| mode.delete_selection(document))
    }
}
fn same_component_layout(
    before: &DocumentState,
    after: &DocumentState,
    object: &ObjectRef,
) -> bool {
    let id = &object.id;
    match object.kind {
        ObjectKind::Mesh => before
            .mesh(id)
            .zip(after.mesh(id))
            .is_some_and(|(old, new)| {
                old.vertex_ids == new.vertex_ids
                    && old.uvs == new.uvs
                    && old.triangles == new.triangles
            }),
        ObjectKind::Deformer => {
            let Some((old, new)) = before.transform(id).zip(after.transform(id)) else {
                return false;
            };
            let Some((old_warp, new_warp)) = old.warp().zip(new.warp()) else {
                return old.rotation().is_some() && new.rotation().is_some();
            };
            if (old_warp.rows, old_warp.columns) != (new_warp.rows, new_warp.columns) {
                return false;
            }
            let default_grid = |transform: &zipzi_core::Transform| {
                transform
                    .warp_authoring
                    .as_ref()
                    .and_then(|s| s.bezier_grid)
                    .map_or((1, 1), |grid| (grid.rows, grid.columns))
            };
            let grid = |bezier: &Option<zipzi_core::WarpBezier>, fallback| {
                bezier
                    .as_ref()
                    .map_or(fallback, |grid| (grid.rows, grid.columns))
            };
            let old_default = default_grid(old);
            let new_default = default_grid(new);
            // A point edit can materialize an implicit controller. Compare
            // its dimensions, including the implicit default, rather than its presence.
            let keyform_grids = |model: &DocumentState, fallback| match model
                .binding_for_scene(id)
                .map(|binding| &binding.track)
            {
                Some(zipzi_core::SceneTrack::Warp { keyforms, .. }) => keyforms
                    .iter()
                    .map(|form| grid(&form.bezier, fallback))
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            };
            grid(&old_warp.bezier, old_default) == grid(&new_warp.bezier, new_default)
                && keyform_grids(before, old_default) == keyform_grids(after, new_default)
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug)]
pub enum CanvasInput {
    /// Elapsed time for a captured continuous brush. Pointer events alone never add time.
    BrushTick {
        elapsed: std::time::Duration,
    },
    /// Start a size-only gesture. The same capture/rollback rules apply as painting.
    BrushResizeDown {
        point: ViewportPoint,
    },
    Down {
        point: ViewportPoint,
        shift: bool,
        alt: bool,
        /// Platform primary modifier (Command on macOS), captured on press.
        primary: bool,
    },
    /// Move with the most recent transform modifiers. Native pointer callers
    /// supply current keys through `update_canvas_gesture_with_modifiers`.
    Move {
        point: ViewportPoint,
        left_down: bool,
    },
    Up {
        point: ViewportPoint,
    },
    /// Transform constraints change immediately, even with a stationary pointer.
    ModifiersChanged {
        shift: bool,
        alt: bool,
    },
    Cancel,
    Leave,
}

/// Guidance for an active viewport navigation capture.
pub fn navigation_hints() -> &'static [InteractionHint] {
    hand::CAPTURE_HINTS
}

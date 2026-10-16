use crate::{
    CanvasFrame, CanvasMetrics, CanvasScene, Document, EditorCommand, OverlaySnapshot,
    SceneSnapshot, ViewportId, ViewportState,
};
use anyhow::{Result, ensure};
use kasane_sdk::SaveReceipt;
use std::{collections::HashMap, path::Path, sync::Arc};

use crate::editing_context::{AnimationContext, ModelingContext};

pub struct DocumentSession {
    pub(crate) modeling: ModelingContext,
    pub(crate) animation: AnimationContext,
    pub(crate) document: Document,
    pub(crate) views: HashMap<ViewportId, ViewportState>,
    pub scene_revision: u64,
    cache: Option<SceneSnapshot>,
    geometry: Option<Arc<CanvasScene>>,
    previous_geometry: Option<Arc<CanvasScene>>,
    appearance_geometry: Option<Arc<CanvasScene>>,
    pub evaluations: u64,
}
impl DocumentSession {
    pub fn from_document(document: Document) -> Self {
        Self {
            animation: AnimationContext {
                playback: crate::AnimationSession::new(&document),
                timeline: Default::default(),
            },
            modeling: Default::default(),
            document,
            views: HashMap::new(),
            scene_revision: 1,
            cache: None,
            geometry: None,
            previous_geometry: None,
            appearance_geometry: None,
            evaluations: 0,
        }
    }
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub(crate) fn reconcile_animation(&mut self) {
        if self
            .animation
            .timeline
            .edit
            .as_ref()
            .is_some_and(|edit| edit.base_version.revision != self.document.model_generation())
        {
            self.cancel_timeline_edit();
        }
        self.animation.playback.reconcile(&self.document);
        self.animation.timeline.reconcile(
            &self.document,
            self.animation
                .playback
                .motion
                .as_ref()
                .map(|motion| motion.clip_id.as_str()),
        );
    }
    pub fn evaluated_frame(&self) -> Option<Arc<kasane_core::DrawableFrame>> {
        self.cache
            .as_ref()
            .map(|scene| scene.viewport_frame.clone())
    }
    pub(crate) fn fit_default_view(&mut self, viewport: ViewportId, metrics: CanvasMetrics) {
        if self.views.entry(viewport).or_default().default_fit {
            self.fit_view(viewport, metrics);
        }
    }
    pub(crate) fn fit_view(&mut self, viewport: ViewportId, metrics: CanvasMetrics) {
        if !self.modeling.tools.dragging() {
            self.views
                .entry(viewport)
                .or_default()
                .fit(self.document.canvas(), metrics);
        }
    }
    pub(crate) fn view_state(&self, viewport: ViewportId) -> ViewportState {
        self.views[&viewport]
    }
    pub fn active_tool(&self) -> crate::ToolId {
        self.modeling.tools.active
    }
    fn invalidate_scene(&mut self) {
        self.scene_revision += 1;
        self.cache = None;
        self.previous_geometry = self
            .geometry
            .take()
            .or_else(|| self.previous_geometry.take());
        self.appearance_geometry = None;
        self.modeling.tools.reconcile(&self.document);
    }
    pub fn cancel_gesture(&mut self) {
        if self.modeling.tools.cancel(&mut self.document) {
            self.invalidate_scene();
        }
    }
    pub fn execute(&mut self, command: EditorCommand) -> Result<()> {
        if self.animation.timeline.edit.is_some() {
            match command {
                EditorCommand::Undo | EditorCommand::Redo | EditorCommand::CancelEdit => {
                    self.cancel_timeline_edit();
                    return Ok(());
                }
                EditorCommand::FinishEdit => self.finish_timeline_edit()?,
                _ => self.cancel_timeline_edit(),
            }
        }
        self.cancel_gesture();
        let reusable = if matches!(
            command,
            EditorCommand::PreviewOpacity(_) | EditorCommand::PreviewName(_)
        ) {
            self.geometry
                .clone()
                .or_else(|| self.appearance_geometry.clone())
        } else {
            None
        };
        let had_pending = self.document.has_pending_edit();
        let evaluation_before = self.document.evaluation_revision();
        let history_state = matches!(&command, EditorCommand::Undo | EditorCommand::Redo)
            .then(|| self.document.model().editor_state().clone());
        let is_lock = matches!(&command, EditorCommand::SetObjectsLocked { .. });
        let lock_or_history = matches!(
            &command,
            EditorCommand::SetObjectsLocked { .. } | EditorCommand::Undo | EditorCommand::Redo
        );
        let preview_before = self.document.preview_revision();
        let is_preview = matches!(
            &command,
            EditorCommand::SetPreviewParameter { .. }
                | EditorCommand::SetPreviewParameters { .. }
                | EditorCommand::PreviewKeyform { .. }
        );
        let selection = matches!(
            &command,
            EditorCommand::Select(_)
                | EditorCommand::SelectObjects(_)
                | EditorCommand::SelectCanvasObjects(_)
                | EditorCommand::SelectObject(_)
                | EditorCommand::OpenBinding(_)
                | EditorCommand::Navigate(_)
                | EditorCommand::NavigateBack
        );
        let context_only = selection || matches!(&command, EditorCommand::FocusParameter(_));
        let invalidate =
            !(context_only || matches!(&command, EditorCommand::FinishEdit)) || had_pending;
        let result = match command {
            EditorCommand::CreateParameter(draft) => {
                self.document.create_parameter(draft).map(|_| ())
            }
            EditorCommand::CreateParameterChecked {
                draft,
                expected_version,
            } => {
                self.document.check_parameter_version(expected_version)?;
                self.document.create_parameter(draft).map(|_| ())
            }
            EditorCommand::UpdateParameter {
                id,
                draft,
                expected_version,
            } => self.document.update_parameter(&id, draft, expected_version),
            EditorCommand::DeleteParameter {
                id,
                expected_version,
            } => self.document.delete_parameter(&id, expected_version),
            EditorCommand::CreateParameterGroup {
                draft,
                expected_version,
            } => self
                .document
                .create_parameter_group(draft, expected_version)
                .map(|_| ()),
            EditorCommand::UpdateParameterGroup {
                id,
                draft,
                expected_version,
            } => self
                .document
                .update_parameter_group(&id, draft, expected_version),
            EditorCommand::DeleteParameterGroup {
                id,
                expected_version,
            } => self.document.delete_parameter_group(&id, expected_version),
            EditorCommand::ReorderTree {
                deformer,
                source,
                target,
                position,
            } => self
                .document
                .reorder_tree(deformer, &source, &target, position),
            EditorCommand::MoveOrganization {
                source,
                destination,
            } => self.document.move_organization(&source, destination),
            EditorCommand::MoveParameter { id, destination } => {
                self.document.move_parameter(&id, destination)
            }
            EditorCommand::MoveParameterGroupToRoot(id) => {
                self.document.move_parameter_group_to_root(&id)
            }
            EditorCommand::ReorderParameterGroup {
                source,
                target,
                position,
            } => self
                .document
                .reorder_parameter_group(&source, &target, position),
            EditorCommand::SetTool(tool) => {
                self.modeling.tools.set_tool(tool, &mut self.document);
                return Ok(());
            }
            EditorCommand::GenerateMeshGrid {
                object,
                options,
                expected_revision,
            } => {
                let result = self
                    .document
                    .generate_mesh_grid(&object, options, expected_revision);
                if result.is_ok() {
                    self.modeling.tools.clear();
                }
                result
            }
            EditorCommand::ApplyMeshGeneration(preview) => {
                let result = self.document.apply_mesh_generation(&preview);
                if result.is_ok() {
                    self.modeling.tools.clear();
                }
                result
            }
            EditorCommand::ToggleDeformers => {
                self.modeling.tools.deformer_visibility.all =
                    !self.modeling.tools.deformer_visibility.all;
                return Ok(());
            }
            EditorCommand::ToggleAncestorDeformers => {
                self.modeling.tools.deformer_visibility.ancestors =
                    !self.modeling.tools.deformer_visibility.ancestors;
                return Ok(());
            }
            EditorCommand::SelectObjects(objects) => self.document.select_objects(objects),
            EditorCommand::SelectCanvasObjects(objects) => {
                self.document.select_canvas_objects(objects)
            }
            EditorCommand::SetObjectsLocked { objects, locked } => {
                self.document.set_objects_locked(&objects, locked)
            }
            EditorCommand::ToggleWarpFine => {
                self.modeling.tools.warp_fine = !self.modeling.tools.warp_fine;
                self.modeling.tools.clear();
                return Ok(());
            }
            EditorCommand::ToggleDeformerGrid => {
                self.modeling.tools.deformer_grid = !self.modeling.tools.deformer_grid;
                return Ok(());
            }
            EditorCommand::AdjustDeformer { object, adjustment } => {
                self.document.adjust_deformer(&object, adjustment)
            }
            EditorCommand::SetObjectProperty {
                object,
                field,
                value,
            } => self.document.set_object_property(&object, &field, value),
            EditorCommand::Select(id) => self.document.select(&id),
            EditorCommand::SelectObject(r) => self.document.select_object(r),
            EditorCommand::FocusParameter(id) => self.document.focus_parameter(&id),
            EditorCommand::OpenBinding(r) => self.document.open_binding(r),
            EditorCommand::Navigate(r) => self.document.navigate(r),
            EditorCommand::NavigateBack => self.document.navigate_back(),
            EditorCommand::PreviewKeyform { binding, point } => {
                self.document.preview_keyform(&binding, point)
            }
            EditorCommand::PreviewName(name) => self.document.preview_name(&name),
            EditorCommand::PreviewOpacity(v) => self.document.preview_opacity(v),
            EditorCommand::SetPreviewParameter {
                parameter_id,
                value,
            } => self.document.set_preview_parameter(&parameter_id, value),
            EditorCommand::SetPreviewParameters { values } => {
                self.document.set_preview_parameters(&values)
            }
            EditorCommand::SetObjectVisible { object, visible } => {
                self.document.set_object_visible(&object, visible)
            }
            EditorCommand::FinishEdit => self.document.finish_edit(),
            EditorCommand::CancelEdit => {
                self.document.cancel_edit();
                Ok(())
            }
            EditorCommand::Undo => self.document.undo(),
            EditorCommand::Redo => self.document.redo(),
        };
        if selection || lock_or_history {
            self.modeling.tools.clear();
        }
        // Editor lock edits are standalone transactions. Other metadata history
        // (for example names) must refresh labels captured in canvas topology.
        let lock_history = history_state.is_some_and(|before| {
            let after = self.document.model().editor_state();
            before != *after && before.hidden_objects().eq(after.hidden_objects())
        });
        if invalidate
            && (!(is_lock || lock_history)
                || had_pending
                || self.document.evaluation_revision() != evaluation_before)
            && (!is_preview || had_pending || self.document.preview_revision() != preview_before)
        {
            self.invalidate_scene();
            if result.is_ok() {
                self.appearance_geometry = reusable;
            }
        }
        self.reconcile_animation();
        result
    }
    pub fn save(&mut self, path: &Path) -> Result<SaveReceipt> {
        self.document.save(path)
    }
    pub fn scene(&mut self) -> Result<(SceneSnapshot, Arc<CanvasScene>)> {
        if self.cache.is_none() {
            let scene = self.document.snapshot()?;
            if self.geometry.is_none() {
                let previous = self.previous_geometry.take();
                self.geometry = Some(if let Some(previous) = self.appearance_geometry.take() {
                    Arc::new(previous.with_appearance(&self.document, scene.clone()))
                } else if let Some(previous) = previous {
                    Arc::new(previous.with_positions(&self.document, scene.clone()))
                } else {
                    self.document.geometry(&scene)
                });
            }
            self.cache = Some(scene);
            self.evaluations += 1;
        }
        Ok((
            self.cache.as_ref().unwrap().clone(),
            self.geometry.as_ref().unwrap().clone(),
        ))
    }
    pub fn overlay(&self, frame: &CanvasFrame) -> OverlaySnapshot {
        self.modeling.tools.overlay(&self.document, frame)
    }
    pub fn input(
        &mut self,
        viewport: ViewportId,
        event: crate::CanvasInput,
        frame: Option<CanvasFrame>,
    ) -> Result<bool> {
        if matches!(event, crate::CanvasInput::Cancel) {
            let was = self.modeling.tools.dragging();
            self.cancel_gesture();
            return Ok(was);
        }
        let Some(frame) = frame else {
            ensure!(
                !matches!(event, crate::CanvasInput::Down { .. }),
                "Wait for the current model frame"
            );
            return Ok(false);
        };
        if matches!(event, crate::CanvasInput::Down { .. }) {
            ensure!(
                frame.version.scene == self.scene_revision
                    && frame.version.viewport == self.view_state(viewport).revision,
                "Wait for the current model frame"
            );
            self.document.finish_edit()?;
        }
        let result = self.modeling.tools.input(&mut self.document, &frame, event);
        if result.as_ref().is_ok_and(|changed| *changed) || result.is_err() {
            self.invalidate_scene();
        }
        result
    }
}

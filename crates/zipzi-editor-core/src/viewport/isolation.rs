//! Transient Modeling viewport isolation. Evaluation and document visibility stay intact.
mod compositing;
use crate::{
    CanvasScene, Editor, ObjectKind, ObjectRef, SceneSnapshot, ViewportBinding,
    ViewportInteractionKind, ViewportKind,
};
use anyhow::{Result, ensure};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IsolationDisplay {
    #[default]
    Dim,
    Hide,
}

#[derive(Default)]
pub(crate) struct ViewportIsolation {
    document: Option<crate::DocumentId>,
    scope: Option<Arc<HashSet<ObjectRef>>>,
    display: IsolationDisplay,
    cache: Option<(Arc<CanvasScene>, SceneSnapshot, Arc<CanvasScene>)>,
}
impl ViewportIsolation {
    pub fn bind_document(&mut self, document: crate::DocumentId) {
        if self.document != Some(document) {
            self.document = Some(document);
            self.scope = None;
            self.cache = None;
        }
    }
    pub fn project(
        &mut self,
        source: &SceneSnapshot,
        geometry: &Arc<CanvasScene>,
    ) -> Option<(SceneSnapshot, Arc<CanvasScene>)> {
        let scope = self.scope.as_ref()?;
        if let Some((base, scene, canvas)) = &self.cache
            && Arc::ptr_eq(base, geometry)
        {
            return Some((scene.clone(), canvas.clone()));
        }
        let mut scene = source.clone();
        let frame = Arc::make_mut(&mut scene.viewport_frame);
        // The full-image reference belongs to the same isolation scope as its mesh.
        let mut render_scope = scope.as_ref().clone();
        for drawable in &frame.drawables {
            if let Some(id) = drawable.id.strip_prefix("__manual_mesh_reference_") {
                if scope.contains(&ObjectRef::new(ObjectKind::Mesh, id)) {
                    render_scope.insert(ObjectRef::new(ObjectKind::Mesh, &drawable.id));
                }
            }
        }
        match self.display {
            IsolationDisplay::Dim => compositing::dim_context(frame, &render_scope),
            IsolationDisplay::Hide => {
                for mesh in &mut frame.drawables {
                    if !render_scope.contains(&ObjectRef::new(ObjectKind::Mesh, &mesh.id)) {
                        mesh.visible = false;
                    }
                }
            }
        }
        let canvas = Arc::new(geometry.isolate(scope.clone()));
        self.cache = Some((geometry.clone(), scene.clone(), canvas.clone()));
        Some((scene, canvas))
    }
}
impl Editor {
    pub fn viewport_isolated(&self, binding: ViewportBinding) -> bool {
        self.viewport_binding_valid(binding)
            && self
                .viewport_presentation(binding.viewport)
                .is_some_and(|state| state.isolation.scope.is_some())
    }
    pub fn viewport_isolation_display(&self, binding: ViewportBinding) -> IsolationDisplay {
        self.viewport_presentation(binding.viewport)
            .map_or(IsolationDisplay::Dim, |state| state.isolation.display)
    }
    pub fn can_toggle_viewport_isolation(&self, binding: ViewportBinding) -> bool {
        self.viewport_binding_valid(binding)
            && binding.kind == ViewportKind::Modeling
            && !self.gesture_active()
            && self
                .viewport_interaction
                .is_none_or(|token| token.kind == ViewportInteractionKind::Navigation)
            && (self.viewport_isolated(binding)
                || self.document().is_some_and(|document| {
                    document.context().selections.iter().any(|object| {
                        matches!(
                            object.kind,
                            ObjectKind::Mesh
                                | ObjectKind::Part
                                | ObjectKind::Deformer
                                | ObjectKind::Offscreen
                                | ObjectKind::Glue
                        )
                    })
                }))
    }
    pub fn toggle_viewport_isolation(&mut self, binding: ViewportBinding) -> Result<()> {
        ensure!(
            self.can_toggle_viewport_isolation(binding),
            "Viewport isolation is unavailable"
        );
        let scope = if self.viewport_isolated(binding) {
            None
        } else {
            let session = self.active_session_mut().unwrap();
            let selection = session.document().context().selections.clone();
            let (_, geometry) = session.scene()?;
            let mut roots = selection;
            // Offscreens own a Part's composite; glue owns both endpoint meshes.
            for object in roots.clone() {
                if object.kind == ObjectKind::Offscreen {
                    if let Some(info) = geometry.index.get(&object) {
                        roots.push(ObjectRef::new(ObjectKind::Part, &info.part_id));
                    }
                }
                if object.kind == ObjectKind::Glue {
                    if let Some(glue) = geometry.topology.glues.iter().find(|g| g.object == object)
                    {
                        roots.push(ObjectRef::new(ObjectKind::Mesh, &glue.mesh_a));
                        roots.push(ObjectRef::new(ObjectKind::Mesh, &glue.mesh_b));
                    }
                }
            }
            let mut scope: HashSet<_> =
                geometry
                    .index
                    .objects
                    .iter()
                    .filter(|info| {
                        if roots.contains(&info.reference) {
                            return true;
                        }
                        if geometry.ancestors(&info.reference).iter().any(|parent| {
                            parent.kind == ObjectKind::Deformer && roots.contains(parent)
                        }) {
                            return true;
                        }
                        // Part membership follows ownership only, never a deformation link
                        // from a mesh in another Part.
                        let mut part = info.part_id.as_str();
                        let mut seen = HashSet::new();
                        while !part.is_empty() && seen.insert(part) {
                            let reference = ObjectRef::new(ObjectKind::Part, part);
                            if roots.contains(&reference) {
                                return true;
                            }
                            part = geometry
                                .index
                                .get(&reference)
                                .map_or("", |parent| &parent.part_id);
                        }
                        false
                    })
                    .map(|info| info.reference.clone())
                    .collect();
            for glue in geometry.topology.glues.iter() {
                if scope.contains(&ObjectRef::new(ObjectKind::Mesh, &glue.mesh_a))
                    && scope.contains(&ObjectRef::new(ObjectKind::Mesh, &glue.mesh_b))
                {
                    scope.insert(glue.object.clone());
                }
            }
            ensure!(!scope.is_empty(), "Select canvas objects to isolate");
            Some(Arc::new(scope))
        };
        let tools = &mut self.active_session_mut().unwrap().modeling.tools;
        if !tools.manual_active() {
            tools.clear();
        }
        let presentation = self.viewport_presentation_mut(binding.viewport).unwrap();
        presentation.isolation.scope = scope;
        presentation.isolation.cache = None;
        presentation.invalidate();
        Ok(())
    }
    pub fn set_viewport_isolation_display(
        &mut self,
        binding: ViewportBinding,
        display: IsolationDisplay,
    ) -> Result<()> {
        ensure!(
            self.viewport_binding_valid(binding)
                && binding.kind == ViewportKind::Modeling
                && !self.gesture_active()
                && self
                    .viewport_interaction
                    .is_none_or(|token| { token.kind == ViewportInteractionKind::Navigation }),
            "Viewport isolation is unavailable"
        );
        let presentation = self.viewport_presentation_mut(binding.viewport).unwrap();
        if presentation.isolation.display != display {
            presentation.isolation.display = display;
            presentation.isolation.cache = None;
            if presentation.isolation.scope.is_some() {
                presentation.invalidate();
            }
        }
        Ok(())
    }
}

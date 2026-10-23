//! Runtime viewport registration and binding identity, independent of GPUI.
use crate::{DocumentId, Editor, ViewportInteractionKind, WorkspaceKind};
use anyhow::{Result, ensure};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ViewportId(Uuid);
impl ViewportId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for ViewportId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ViewportKind {
    Modeling,
    Animation,
}
impl From<WorkspaceKind> for ViewportKind {
    fn from(workspace: WorkspaceKind) -> Self {
        match workspace {
            WorkspaceKind::Modeling => Self::Modeling,
            WorkspaceKind::Animation => Self::Animation,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportBinding {
    pub viewport: ViewportId,
    pub kind: ViewportKind,
    pub document: DocumentId,
    pub generation: u64,
}

struct Registration {
    kind: ViewportKind,
    binding: Option<ViewportBinding>,
    enabled: bool,
    metrics: crate::ViewportMetrics,
    presentation: crate::ViewportPresentation,
}
#[derive(Default)]
pub(crate) struct ViewportRegistry {
    registrations: HashMap<ViewportId, Registration>,
    primary: HashMap<ViewportKind, ViewportId>,
    generation: u64,
}
impl ViewportRegistry {
    fn refresh(&mut self, document: Option<DocumentId>) {
        for (&viewport, entry) in &mut self.registrations {
            let document = document.filter(|_| entry.enabled);
            if entry.binding.map(|binding| binding.document) == document {
                continue;
            }
            self.generation += 1;
            entry.presentation.invalidate();
            if let Some(document) = document {
                entry.presentation.isolation.bind_document(document);
            }
            entry.binding = document.map(|document| ViewportBinding {
                viewport,
                kind: entry.kind,
                document,
                generation: self.generation,
            });
        }
    }
}

impl Editor {
    /// Register an instance independently of the primary presentation bridge.
    pub fn register_viewport(&mut self, viewport: ViewportId, kind: ViewportKind) -> Result<()> {
        if let Some(entry) = self.viewports.registrations.get(&viewport) {
            ensure!(entry.kind == kind, "A viewport's kind cannot change");
        } else {
            self.viewports.registrations.insert(
                viewport,
                Registration {
                    kind,
                    binding: None,
                    enabled: true,
                    metrics: Default::default(),
                    presentation: Default::default(),
                },
            );
        }
        self.ensure_viewport_states(viewport);
        self.refresh_viewport_bindings();
        Ok(())
    }
    pub fn register_primary_viewport(
        &mut self,
        viewport: ViewportId,
        kind: ViewportKind,
    ) -> Result<()> {
        self.register_viewport(viewport, kind)?;
        let previous = self.viewports.primary.insert(kind, viewport);
        if previous != Some(viewport) {
            self.set_viewport_visible(viewport, true);
            if let Some(previous) = previous {
                self.set_viewport_visible(previous, false);
            }
        }
        self.refresh_viewport_bindings();
        Ok(())
    }
    pub fn primary_viewport_id(&self) -> Option<ViewportId> {
        self.viewports
            .primary
            .get(&ViewportKind::from(self.workspace()))
            .copied()
    }
    pub fn viewport_metrics(&self, viewport: ViewportId) -> Option<crate::ViewportMetrics> {
        self.viewports
            .registrations
            .get(&viewport)
            .map(|entry| entry.metrics)
    }
    pub fn set_viewport_metrics(&mut self, viewport: ViewportId, metrics: crate::ViewportMetrics) {
        let Some(entry) = self.viewports.registrations.get_mut(&viewport) else {
            return;
        };
        if entry.metrics == metrics {
            return;
        }
        entry.metrics = metrics;
        if let Some(token) = self.viewport_interaction.filter(|token| {
            token.binding.viewport == viewport && token.kind == ViewportInteractionKind::Modeling
        }) {
            self.cancel_viewport_interaction(token);
        }
        self.invalidate_viewport_state(viewport);
        if let Some(presentation) = self.viewport_presentation_mut(viewport) {
            presentation.requested_modeling = None;
            presentation.requested_animation = None;
        }
    }
    pub fn viewport_ids(&self) -> Vec<ViewportId> {
        self.viewports.registrations.keys().copied().collect()
    }
    pub fn unregister_viewport(&mut self, viewport: ViewportId) {
        if let Some(token) = self
            .viewport_interaction
            .filter(|token| token.binding.viewport == viewport)
        {
            self.cancel_viewport_interaction(token);
        }
        if let Some(entry) = self.viewports.registrations.remove(&viewport) {
            if self.viewports.primary.get(&entry.kind) == Some(&viewport) {
                self.viewports.primary.remove(&entry.kind);
            }
        }
    }
    pub fn set_viewport_visible(&mut self, viewport: ViewportId, visible: bool) {
        if !visible {
            if let Some(token) = self
                .viewport_interaction
                .filter(|token| token.binding.viewport == viewport)
            {
                self.cancel_viewport_interaction(token);
            }
        }
        if let Some(entry) = self.viewports.registrations.get_mut(&viewport) {
            entry.enabled = visible;
        }
        self.refresh_viewport_bindings();
    }
    pub(crate) fn viewport_presentation(
        &self,
        viewport: ViewportId,
    ) -> Option<&crate::ViewportPresentation> {
        self.viewports
            .registrations
            .get(&viewport)
            .map(|entry| &entry.presentation)
    }
    pub(crate) fn viewport_presentation_mut(
        &mut self,
        viewport: ViewportId,
    ) -> Option<&mut crate::ViewportPresentation> {
        self.viewports
            .registrations
            .get_mut(&viewport)
            .map(|entry| &mut entry.presentation)
    }
    pub fn visible_viewport_bindings(&self) -> Vec<ViewportBinding> {
        self.viewports
            .registrations
            .values()
            .filter_map(|entry| entry.binding)
            .collect()
    }
    pub fn viewport_binding(&self, viewport: ViewportId) -> Option<ViewportBinding> {
        self.viewports.registrations.get(&viewport)?.binding
    }
    pub fn primary_viewport_binding(&self) -> Option<ViewportBinding> {
        self.viewport_binding(
            *self
                .viewports
                .primary
                .get(&ViewportKind::from(self.workspace()))?,
        )
    }
    pub fn viewport_binding_valid(&self, binding: ViewportBinding) -> bool {
        self.active_id() == Some(binding.document)
            && self.viewport_binding(binding.viewport) == Some(binding)
            && !self.is_busy()
            && !self.exiting()
    }
    pub(crate) fn invalidate_viewport_bindings(&mut self) {
        self.invalidate_timeline_bindings();
        for entry in self.viewports.registrations.values_mut() {
            entry.binding = None;
            entry.presentation.invalidate();
        }
        self.refresh_viewport_bindings();
    }
    pub fn viewport_interaction_active(&self) -> bool {
        self.viewport_interaction.is_some()
    }
    pub fn viewport_interaction_binding(&self) -> Option<ViewportBinding> {
        self.viewport_interaction
            .filter(|token| self.viewport_interaction_valid(*token))
            .map(|token| token.binding)
    }
    pub(crate) fn refresh_viewport_bindings(&mut self) {
        self.refresh_timeline_bindings();
        let document = self
            .active_id()
            .filter(|_| !self.is_busy() && !self.exiting());
        self.viewports.refresh(document);
        if self
            .viewport_interaction
            .is_some_and(|token| !self.viewport_binding_valid(token.binding))
        {
            // Context transitions cancel the domain operation before rebinding.
            self.viewport_interaction = None;
        }
    }
}

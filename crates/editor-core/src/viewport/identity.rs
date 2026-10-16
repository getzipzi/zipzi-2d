//! Runtime viewport registration and binding identity, independent of GPUI.
use crate::{CanvasInput, DocumentId, Editor, WorkspaceKind};
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
}
#[derive(Default)]
pub(crate) struct ViewportRegistry {
    registrations: HashMap<ViewportId, Registration>,
    primary: HashMap<ViewportKind, ViewportId>,
    generation: u64,
}
impl ViewportRegistry {
    fn refresh(&mut self, document: Option<DocumentId>, workspace: WorkspaceKind) {
        for (&viewport, entry) in &mut self.registrations {
            let document = document.filter(|_| {
                entry.enabled
                    && entry.kind == ViewportKind::from(workspace)
                    && self.primary.get(&entry.kind) == Some(&viewport)
            });
            if entry.binding.map(|binding| binding.document) == document {
                continue;
            }
            self.generation += 1;
            entry.binding = document.map(|document| ViewportBinding {
                viewport,
                kind: entry.kind,
                document,
                generation: self.generation,
            });
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewportInteractionKind {
    Navigation,
    Modeling,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportInteractionToken {
    pub binding: ViewportBinding,
    pub kind: ViewportInteractionKind,
    serial: u64,
}

impl Editor {
    /// Transitional primary slots address the existing single surface. Other
    /// instances will become executable once presentation/state are separated.
    pub fn register_primary_viewport(
        &mut self,
        viewport: ViewportId,
        kind: ViewportKind,
    ) -> Result<()> {
        if let Some(entry) = self.viewports.registrations.get(&viewport) {
            ensure!(entry.kind == kind, "A viewport's kind cannot change");
        } else {
            self.viewports.registrations.insert(
                viewport,
                Registration {
                    kind,
                    binding: None,
                    enabled: true,
                },
            );
        }
        if self
            .viewports
            .primary
            .get(&kind)
            .is_some_and(|id| *id != viewport)
        {
            if let Some(token) = self
                .viewport_interaction
                .filter(|token| token.binding.kind == kind)
            {
                self.cancel_viewport_interaction(token);
            }
        }
        self.viewports.primary.insert(kind, viewport);
        self.refresh_viewport_bindings();
        Ok(())
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
        for entry in self.viewports.registrations.values_mut() {
            entry.binding = None;
        }
        self.refresh_viewport_bindings();
    }
    pub fn viewport_interaction_active(&self) -> bool {
        self.viewport_interaction.is_some()
    }
    pub(crate) fn refresh_viewport_bindings(&mut self) {
        let document = self
            .active_id()
            .filter(|_| !self.is_busy() && !self.exiting());
        self.viewports.refresh(document, self.workspace());
        if self
            .viewport_interaction
            .is_some_and(|token| !self.viewport_binding_valid(token.binding))
        {
            // Context transitions cancel the domain operation before rebinding.
            self.viewport_interaction = None;
        }
    }
    pub fn begin_viewport_interaction(
        &mut self,
        binding: ViewportBinding,
        kind: ViewportInteractionKind,
    ) -> Result<ViewportInteractionToken> {
        ensure!(
            self.viewport_binding_valid(binding),
            "Viewport binding changed"
        );
        ensure!(
            kind == ViewportInteractionKind::Navigation || binding.kind == ViewportKind::Modeling,
            "Modeling interaction requires a Modeling viewport"
        );
        if let Some(previous) = self.viewport_interaction {
            self.cancel_viewport_interaction(previous);
        }
        self.next_viewport_interaction += 1;
        let token = ViewportInteractionToken {
            binding,
            kind,
            serial: self.next_viewport_interaction,
        };
        self.viewport_interaction = Some(token);
        Ok(token)
    }
    pub fn viewport_interaction_valid(&self, token: ViewportInteractionToken) -> bool {
        self.viewport_interaction == Some(token) && self.viewport_binding_valid(token.binding)
    }
    pub fn finish_viewport_interaction(&mut self, token: ViewportInteractionToken) -> bool {
        if self.viewport_interaction != Some(token) {
            return false;
        }
        self.viewport_interaction = None;
        true
    }
    pub fn cancel_viewport_interaction(&mut self, token: ViewportInteractionToken) -> bool {
        if !self.finish_viewport_interaction(token) {
            return false;
        }
        if token.kind == ViewportInteractionKind::Modeling
            && self.active_id() == Some(token.binding.document)
        {
            self.cancel_gesture();
        }
        true
    }
    pub fn canvas_interaction_input(
        &mut self,
        token: ViewportInteractionToken,
        input: CanvasInput,
    ) -> Result<bool> {
        ensure!(
            self.viewport_interaction_valid(token)
                && token.kind == ViewportInteractionKind::Modeling,
            "Interaction owner changed"
        );
        self.canvas_input_for(token.binding, input)
    }
    pub fn canvas_input_for(
        &mut self,
        binding: ViewportBinding,
        input: CanvasInput,
    ) -> Result<bool> {
        ensure!(
            self.viewport_binding_valid(binding),
            "Viewport binding changed"
        );
        ensure!(
            binding.kind == ViewportKind::Modeling,
            "Viewport does not support Modeling input"
        );
        self.canvas_input(input)
    }
}

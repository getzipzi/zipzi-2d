//! Runtime Timeline panel ownership, separate from the document's clip state.
use crate::{DocumentId, Editor, TimelineCandidateIdentity};
use anyhow::{Result, ensure};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimelineId(Uuid);
impl TimelineId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for TimelineId {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineBinding {
    pub panel: TimelineId,
    pub document: DocumentId,
    pub generation: u64,
}
#[derive(Default)]
pub(crate) struct TimelinePanels {
    entries: HashMap<TimelineId, (bool, Option<TimelineBinding>)>,
    generation: u64,
}
impl Editor {
    pub fn set_timeline_visible(&mut self, panel: TimelineId, visible: bool) {
        let document = self
            .active_id()
            .filter(|_| visible && !self.is_busy() && !self.exiting());
        let entry = self.timeline_panels.entries.entry(panel).or_default();
        entry.0 = visible;
        if entry.1.map(|binding| binding.document) == document {
            return;
        }
        let previous = entry.1.take();
        self.timeline_panels.generation += 1;
        entry.1 = document.map(|document| TimelineBinding {
            panel,
            document,
            generation: self.timeline_panels.generation,
        });
        if let Some(binding) = previous {
            if self.timeline_scrub_owner == Some(binding) {
                if self.active_id() == Some(binding.document) {
                    self.pause_animation();
                }
                self.timeline_scrub_owner = None;
            }
            if self
                .timeline_edit()
                .is_some_and(|edit| edit.owner == Some(binding))
            {
                self.cancel_timeline_edit();
            }
        }
    }
    pub fn unregister_timeline(&mut self, panel: TimelineId) {
        self.set_timeline_visible(panel, false);
        self.timeline_panels.entries.remove(&panel);
    }
    pub fn timeline_binding(&self, panel: TimelineId) -> Option<TimelineBinding> {
        self.timeline_panels
            .entries
            .get(&panel)?
            .1
            .filter(|binding| {
                self.active_id() == Some(binding.document) && !self.is_busy() && !self.exiting()
            })
    }
    pub fn timeline_binding_valid(&self, binding: TimelineBinding) -> bool {
        self.timeline_binding(binding.panel) == Some(binding)
    }
    pub(crate) fn refresh_timeline_bindings(&mut self) {
        let panels: Vec<_> = self
            .timeline_panels
            .entries
            .iter()
            .map(|(&id, &(visible, _))| (id, visible))
            .collect();
        for (id, visible) in panels {
            self.set_timeline_visible(id, visible);
        }
    }
    pub(crate) fn invalidate_timeline_bindings(&mut self) {
        for entry in self.timeline_panels.entries.values_mut() {
            entry.1 = None;
        }
        self.refresh_timeline_bindings();
    }
    pub fn begin_timeline_edit_for(
        &mut self,
        binding: TimelineBinding,
        label: impl Into<String>,
    ) -> Result<TimelineCandidateIdentity> {
        ensure!(
            self.timeline_binding_valid(binding),
            "Timeline panel binding changed"
        );
        let token = self.begin_timeline_edit(binding.document, label)?;
        self.active_session_mut()
            .unwrap()
            .animation
            .timeline
            .edit
            .as_mut()
            .unwrap()
            .owner = Some(binding);
        Ok(token)
    }
    pub fn timeline_edit_owned(
        &self,
        binding: TimelineBinding,
        token: TimelineCandidateIdentity,
    ) -> bool {
        self.timeline_binding_valid(binding)
            && self
                .timeline_edit()
                .is_some_and(|edit| edit.owner == Some(binding) && edit.identity.edit == token.edit)
    }
    pub fn scrub_timeline_for(&mut self, binding: TimelineBinding, time: f32) -> Result<()> {
        ensure!(
            self.timeline_binding_valid(binding),
            "Timeline panel binding changed"
        );
        self.scrub_timeline(binding.document, time)?;
        self.timeline_scrub_owner = Some(binding);
        Ok(())
    }
    pub fn timeline_scrub_owned(&self, binding: TimelineBinding) -> bool {
        self.timeline_binding_valid(binding)
            && self.timeline_scrub_owner == Some(binding)
            && self
                .animation()
                .is_some_and(|animation| animation.is_scrubbing())
    }
    /// End only this panel's current scrub. A deferred cleanup must not pause
    /// playback or a scrub subsequently started by another panel.
    pub fn cancel_timeline_scrub_for(&mut self, binding: TimelineBinding) -> bool {
        if !self.timeline_scrub_owned(binding) {
            return false;
        }
        self.pause_animation();
        true
    }
    pub fn seek_timeline_for(&mut self, binding: TimelineBinding, time: f32) -> Result<()> {
        ensure!(
            self.timeline_binding_valid(binding),
            "Timeline panel binding changed"
        );
        self.seek_timeline(binding.document, time)
    }
}

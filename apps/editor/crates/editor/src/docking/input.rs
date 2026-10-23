//! Dock composition lists surfaces; each surface resolves its own command target.
use super::*;
use crate::input_routing::InputRegion;

impl DockWorkspace {
    pub(crate) fn visible_input_regions(&self, cx: &App) -> Vec<InputRegion> {
        let mut regions: Vec<_> = self
            .input_regions
            .iter()
            .filter(|(kind, _)| self.displayed(**kind, cx))
            .map(|(_, region)| region.clone())
            .collect();
        if self.displayed(PanelKind::AnimationTimeline, cx) {
            regions.push(self.timeline.read(cx).input_region.clone());
        }
        regions
    }
    pub(crate) fn all_input_regions(&self, cx: &App) -> Vec<InputRegion> {
        self.input_regions
            .values()
            .cloned()
            .chain(std::iter::once(self.timeline.read(cx).input_region.clone()))
            .collect()
    }
}

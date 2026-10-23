//! Panel selection, inspection, and navigation history reconciliation.
use super::{ObjectIndex, ObjectRef};

#[derive(Clone, Debug, Default)]
pub struct PanelContext {
    /// Ordered object selection; `selection` is the primary (last) object.
    pub selections: Vec<ObjectRef>,
    pub selection: Option<ObjectRef>,
    pub parameter_focus: Option<ObjectRef>,
    pub active_binding: Option<ObjectRef>,
    pub inspected: Option<ObjectRef>,
    pub back: Vec<NavigationContext>,
}
#[derive(Clone, Debug)]
pub struct NavigationContext {
    pub selections: Vec<ObjectRef>,
    pub selection: Option<ObjectRef>,
    pub parameter_focus: Option<ObjectRef>,
    pub active_binding: Option<ObjectRef>,
    pub inspected: Option<ObjectRef>,
}
impl PanelContext {
    pub fn remember(&mut self) {
        self.back.push(NavigationContext {
            selections: self.selections.clone(),
            selection: self.selection.clone(),
            parameter_focus: self.parameter_focus.clone(),
            active_binding: self.active_binding.clone(),
            inspected: self.inspected.clone(),
        });
        if self.back.len() > 64 {
            self.back.remove(0);
        }
    }
    pub fn reconcile(&mut self, index: &ObjectIndex) {
        self.selections.retain(|r| index.get(r).is_some());
        for r in [
            &mut self.selection,
            &mut self.parameter_focus,
            &mut self.inspected,
        ] {
            if r.as_ref().is_some_and(|r| index.get(r).is_none()) {
                *r = None;
            }
        }
        if self.selection.is_none() {
            self.selection = self.selections.last().cloned();
        }
        if self.active_binding.as_ref().is_some_and(|r| {
            index
                .binding(r)
                .is_none_or(|b| Some(&b.target) != self.selection.as_ref())
        }) {
            self.active_binding = None;
        }
        self.back
            .retain(|n| n.inspected.as_ref().is_none_or(|r| index.get(r).is_some()));
    }
}

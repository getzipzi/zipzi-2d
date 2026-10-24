//! Refresh expression payloads and relation labels without rebuilding slot maps.
use super::*;
use serde_json::json;
use zipzi_core::document::{ExpressionBlend, ExpressionTarget};

impl ObjectIndex {
    /// Requires unchanged identities, order and reference endpoints. Callers use
    /// ChangeSet (or the preview's target comparison) to establish that condition.
    pub(crate) fn refresh_expression(&mut self, model: &Model, id: &str) -> bool {
        let Some(expression) = model.get_expression(id) else {
            return false;
        };
        let owner = ObjectRef::new(ObjectKind::Expression, id);
        let Some(&slot) = self.structure.slots.get(&owner) else {
            return false;
        };
        let name = if expression.name.is_empty() {
            id
        } else {
            &expression.name
        };
        let properties = json!(expression);
        if self.objects[slot].properties != properties || self.objects[slot].name != name {
            let info = &mut self.objects[slot];
            info.name = name.into();
            info.properties = properties;
        }
        for entry in &expression.entries {
            let object = ObjectRef::new(ObjectKind::ExpressionEntry, &entry.id);
            let Some(&slot) = self.structure.slots.get(&object) else {
                return false;
            };
            let properties = json!(entry);
            if self.objects[slot].properties != properties {
                self.objects[slot].properties = properties;
            }
        }
        let mut entries = expression
            .entries
            .iter()
            .filter(|e| e.target != ExpressionTarget::Unspecified);
        for &edge in self.structure.outgoing.get(&owner).into_iter().flatten() {
            if self.relations[edge].kind == RelationKind::ExpressionWrites {
                let Some(entry) = entries.next() else {
                    return false;
                };
                let detail = format!(
                    "{} · {:?}",
                    entry.value,
                    entry.blend.unwrap_or(ExpressionBlend::Add)
                );
                if self.relations[edge].detail != detail {
                    self.relations[edge].detail = detail;
                }
            }
        }
        entries.next().is_none()
    }
}

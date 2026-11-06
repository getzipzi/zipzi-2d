use super::{RecordChange, RecordDomain, RecordWrite};
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

/// Directory and order detach separately. Reading/deleting never clones payloads.
#[derive(Debug, Clone)]
pub(super) struct Records<T> {
    values: Arc<HashMap<String, Arc<T>>>,
    order: Arc<Vec<String>>,
}

impl<T> Default for Records<T> {
    fn default() -> Self {
        Self {
            values: Arc::default(),
            order: Arc::default(),
        }
    }
}

impl<T> Records<T> {
    pub fn shared_map(&self) -> &HashMap<String, Arc<T>> {
        &self.values
    }
    pub fn order_vec(&self) -> &Vec<String> {
        &self.order
    }
    pub fn set_order(&mut self, order: Vec<String>) -> Result<(), crate::Status> {
        if order.len() != self.values.len()
            || order.iter().collect::<BTreeSet<_>>().len() != order.len()
            || order.iter().any(|id| !self.values.contains_key(id))
        {
            return Err(crate::Status::error(
                "INVALID_ORDER",
                "Order must contain every object exactly once",
            ));
        }
        self.order = Arc::new(order);
        Ok(())
    }
    pub fn get(&self, id: &str) -> Option<&T> {
        self.values.get(id).map(Arc::as_ref)
    }
    pub fn order(&self) -> &[String] {
        &self.order
    }
    pub fn insert(&mut self, id: String, value: T) {
        if !self.values.contains_key(&id) {
            Arc::make_mut(&mut self.order).push(id.clone());
        }
        Arc::make_mut(&mut self.values).insert(id, Arc::new(value));
    }
    pub fn remove(&mut self, id: &str) {
        Arc::make_mut(&mut self.values).remove(id);
        Arc::make_mut(&mut self.order).retain(|entry| entry != id);
    }
}

impl<T: PartialEq> Records<T> {
    pub fn finish(
        &mut self,
        base: &Self,
        touched: BTreeSet<String>,
        domain: RecordDomain,
        writes: &mut Vec<RecordWrite>,
    ) -> bool {
        if touched.is_empty() {
            return false;
        }
        let start = writes.len();
        for id in touched {
            let before = base.get(&id);
            let after = self.get(&id);
            let change = match (before, after) {
                (None, Some(_)) => Some(RecordChange::Created),
                (Some(_), None) => Some(RecordChange::Deleted),
                (Some(before), Some(after)) if before != after => Some(RecordChange::Replaced),
                _ => None,
            };
            if let Some(change) = change {
                writes.push(RecordWrite { domain, id, change });
            } else if let Some(original) = base.values.get(&id) {
                // Reverted records recover their original sharing even when
                // another record in this batch has a net change.
                Arc::make_mut(&mut self.values).insert(id, original.clone());
            }
        }
        if writes.len() == start {
            self.values = base.values.clone();
        }
        let order_changed = !Arc::ptr_eq(&self.order, &base.order) && self.order != base.order;
        if !order_changed {
            self.order = base.order.clone();
        }
        order_changed
    }
}

//! Editor protection policy, separate from runtime evaluation and SDK authoring.
use super::Document;
use crate::{ObjectKind as K, ObjectRef};
use anyhow::{Result, ensure};
use kasane_core::document::ObjectLocks;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LockState {
    pub explicit: bool,
    pub inherited: Vec<ObjectRef>,
}
impl LockState {
    pub fn locked(&self) -> bool {
        self.explicit || !self.inherited.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct LockIndex {
    states: HashMap<ObjectRef, LockState>,
    organization: HashMap<ObjectRef, Vec<ObjectRef>>,
    deformation: HashMap<ObjectRef, Vec<ObjectRef>>,
}
impl LockIndex {
    pub fn state(&self, object: &ObjectRef) -> &LockState {
        static EMPTY: LockState = LockState {
            explicit: false,
            inherited: Vec::new(),
        };
        self.states.get(object).unwrap_or(&EMPTY)
    }
    pub fn is_locked(&self, object: &ObjectRef) -> bool {
        self.state(object).locked()
    }
    pub fn lockable(&self, object: &ObjectRef) -> bool {
        self.states.contains_key(object)
    }

    fn build(model: &kasane_core::Document) -> Self {
        let mut organization = HashMap::new();
        let mut deformation = HashMap::new();
        let mut add = |kind, id: &str, part: &str, deformer: &str| {
            let object = ObjectRef::new(kind, id);
            organization.insert(
                object.clone(),
                (!part.is_empty()).then(|| ObjectRef::new(K::Part, part)),
            );
            deformation.insert(
                object,
                (!deformer.is_empty()).then(|| ObjectRef::new(K::Deformer, deformer)),
            );
        };
        for id in model.part_order() {
            if let Some(p) = model.get_part(id) {
                add(K::Part, id, &p.parent_id, "");
            }
        }
        for id in model.transform_order() {
            if let Some(t) = model.get_transform(id) {
                add(K::Deformer, id, t.part(), t.parent());
            }
        }
        for id in model.mesh_order() {
            if let Some(m) = model.get_mesh(id) {
                add(K::Mesh, id, &m.part_id, &m.deformer_id);
            }
        }
        for id in model.offscreen_order() {
            if let Some(o) = model.get_offscreen(id) {
                add(K::Offscreen, id, &o.part_id, "");
            }
        }
        fn ancestors(
            object: &ObjectRef,
            parents: &HashMap<ObjectRef, Option<ObjectRef>>,
        ) -> Vec<ObjectRef> {
            let mut result = Vec::new();
            let mut seen = HashSet::from([object.clone()]);
            let mut cursor = parents.get(object).and_then(Option::as_ref);
            while let Some(parent) = cursor {
                if !parents.contains_key(parent) || !seen.insert(parent.clone()) {
                    break;
                }
                result.push(parent.clone());
                cursor = parents.get(parent).and_then(Option::as_ref);
            }
            result
        }
        let explicit: HashSet<_> = model.object_locks().objects.iter().collect();
        let mut index = Self::default();
        for object in organization.keys() {
            let parts = ancestors(object, &organization);
            let deformers = ancestors(object, &deformation);
            let inherited = parts
                .iter()
                .chain(&deformers)
                .filter(|r| explicit.contains(&r.id))
                .cloned()
                .collect();
            index.states.insert(
                object.clone(),
                LockState {
                    explicit: explicit.contains(&object.id),
                    inherited,
                },
            );
            index.organization.insert(object.clone(), parts);
            index.deformation.insert(object.clone(), deformers);
        }
        index
    }
}

impl Document {
    pub fn lock_index(&self) -> Arc<LockIndex> {
        // Pending geometry gestures never change lock membership or hierarchy.
        let revision = self.session.document().revision();
        let mut cache = self.lock_index.lock().unwrap();
        if cache.as_ref().is_none_or(|(r, _)| *r != revision) {
            *cache = Some((
                revision,
                Arc::new(LockIndex::build(self.session.document())),
            ));
        }
        cache.as_ref().unwrap().1.clone()
    }
    pub fn ensure_editable(&self, objects: &[ObjectRef]) -> Result<()> {
        let locks = self.lock_index();
        for object in objects {
            let target = if object.kind.is_binding() {
                self.object_index()
                    .binding(object)
                    .map(|b| b.target.clone())
                    .unwrap_or_else(|| object.clone())
            } else {
                object.clone()
            };
            ensure!(
                !locks.is_locked(&target),
                "Object is locked: {}",
                self.object_index().name(&target)
            );
        }
        Ok(())
    }
    pub(crate) fn ensure_tree_editable(&self, object: &ObjectRef, deformer: bool) -> Result<()> {
        self.ensure_editable(std::slice::from_ref(object))?;
        let locks = self.lock_index();
        let ancestry = if deformer {
            &locks.deformation
        } else {
            &locks.organization
        };
        for (child, ancestors) in ancestry {
            if ancestors.contains(object) {
                self.ensure_editable(std::slice::from_ref(child))?;
            }
        }
        Ok(())
    }
    pub fn set_objects_locked(&mut self, objects: &[ObjectRef], locked: bool) -> Result<()> {
        let index = self.lock_index();
        ensure!(!objects.is_empty(), "Select an object first");
        for object in objects {
            ensure!(
                index.lockable(object),
                "Object cannot be locked: {}",
                object.id
            );
        }
        // Do not let an unpublished gesture commit after protection has changed.
        self.cancel_edit();
        let mut ids: HashSet<_> = self
            .session
            .document()
            .object_locks()
            .objects
            .iter()
            .cloned()
            .collect();
        for object in objects {
            if locked {
                ids.insert(object.id.clone());
            } else {
                ids.remove(&object.id);
            }
        }
        let mut objects: Vec<_> = ids.into_iter().collect();
        objects.sort();
        let (_, receipt) = self.session.edit(
            if locked {
                "Lock objects"
            } else {
                "Unlock objects"
            },
            None,
            |edit| edit.replace_object_locks(ObjectLocks { objects }),
        )?;
        self.session.drain_events();
        if receipt.changed {
            self.projection_revision += 1;
            self.pristine_blank = false;
        }
        Ok(())
    }
    pub fn select_canvas_objects(&mut self, objects: Vec<ObjectRef>) -> Result<()> {
        self.ensure_editable(&objects)?;
        self.select_objects(objects)
    }
    pub(crate) fn explicit_locks(&self) -> &ObjectLocks {
        self.session.document().object_locks()
    }
    pub(crate) fn evaluation_revision(&self) -> u64 {
        self.session.evaluation_revision()
    }
}

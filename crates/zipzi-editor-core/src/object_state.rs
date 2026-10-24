//! Shared editor visibility and protection policy for trees, canvas, and tools.
use super::Document;
use crate::{ObjectKind as K, ObjectRef};
use anyhow::{Result, ensure};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use zipzi_core::document::ObjectEditorState;

/// One resolved editor state for every scene object. Only `explicit` is persisted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectState {
    pub explicit: ObjectEditorState,
    pub locked_by: Vec<ObjectRef>,
    pub hidden_by: Vec<ObjectRef>,
}
impl ObjectState {
    pub fn locked(&self) -> bool {
        self.explicit.locked || !self.locked_by.is_empty()
    }
    pub fn visible(&self) -> bool {
        !self.explicit.hidden_in_editor && self.hidden_by.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct ObjectStateIndex {
    states: HashMap<ObjectRef, ObjectState>,
    organization: HashMap<ObjectRef, Vec<ObjectRef>>,
}
impl ObjectStateIndex {
    pub fn state(&self, object: &ObjectRef) -> &ObjectState {
        static EMPTY: ObjectState = ObjectState {
            explicit: ObjectEditorState {
                locked: false,
                hidden_in_editor: false,
            },
            locked_by: Vec::new(),
            hidden_by: Vec::new(),
        };
        self.states.get(object).unwrap_or(&EMPTY)
    }
    pub fn is_locked(&self, object: &ObjectRef) -> bool {
        self.state(object).locked()
    }
    pub fn contains(&self, object: &ObjectRef) -> bool {
        self.states.contains_key(object)
    }

    fn build(model: &zipzi_core::Document) -> Self {
        let mut organization = HashMap::new();
        let mut add = |kind, id: &str, part: &str| {
            let object = ObjectRef::new(kind, id);
            organization.insert(
                object.clone(),
                (!part.is_empty()).then(|| ObjectRef::new(K::Part, part)),
            );
        };
        for id in model.part_order() {
            if let Some(p) = model.get_part(id) {
                add(K::Part, id, &p.parent_id);
            }
        }
        for id in model.transform_order() {
            if let Some(t) = model.get_transform(id) {
                add(K::Deformer, id, t.part());
            }
        }
        for id in model.mesh_order() {
            if let Some(m) = model.get_mesh(id) {
                add(K::Mesh, id, &m.part_id);
            }
        }
        for id in model.offscreen_order() {
            if let Some(o) = model.get_offscreen(id) {
                add(K::Offscreen, id, &o.part_id);
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
        for id in model.glue_order() {
            add(K::Glue, id, "");
        }
        let editor = model.editor_state();
        let mut composites: HashMap<&str, Vec<&str>> = HashMap::new();
        for id in model.offscreen_order() {
            if editor.get(id).hidden_in_editor {
                if let Some(group) = model.get_offscreen(id) {
                    composites.entry(&group.part_id).or_default().push(id);
                }
            }
        }
        let mut index = Self::default();
        for object in organization.keys() {
            let parts = ancestors(object, &organization);
            let locked_by = parts
                .iter()
                .filter(|r| editor.get(&r.id).locked)
                .cloned()
                .collect();
            let mut hidden_by: Vec<_> = parts
                .iter()
                .filter(|r| editor.get(&r.id).hidden_in_editor)
                .cloned()
                .collect();
            // Composite output restrictions apply to artwork, never controls.
            if matches!(object.kind, K::Mesh | K::Offscreen) {
                for part in &parts {
                    if let Some(groups) = composites.get(part.id.as_str()) {
                        hidden_by.extend(
                            groups
                                .iter()
                                .filter(|id| **id != object.id)
                                .map(|id| ObjectRef::new(K::Offscreen, *id)),
                        );
                    }
                }
            }
            index.states.insert(
                object.clone(),
                ObjectState {
                    explicit: editor.get(&object.id),
                    locked_by,
                    hidden_by,
                },
            );
            index.organization.insert(object.clone(), parts);
        }
        index
    }
}

impl Document {
    pub fn object_state_index(&self) -> Arc<ObjectStateIndex> {
        // Pending geometry gestures never change lock membership or hierarchy.
        let revision = self.session.document().revision();
        let mut cache = self.object_state_index.lock().unwrap();
        if cache.as_ref().is_none_or(|(r, _)| *r != revision) {
            *cache = Some((
                revision,
                Arc::new(ObjectStateIndex::build(&self.session.document())),
            ));
        }
        cache.as_ref().unwrap().1.clone()
    }
    pub fn object_state(&self, object: &ObjectRef) -> ObjectState {
        self.object_state_index().state(object).clone()
    }
    pub fn ensure_editable(&self, objects: &[ObjectRef]) -> Result<()> {
        let locks = self.object_state_index();
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
        // Reordering a deformer does not edit the objects it deforms.
        if deformer {
            return Ok(());
        }
        let locks = self.object_state_index();
        for (child, ancestors) in &locks.organization {
            if ancestors.contains(object) {
                self.ensure_editable(std::slice::from_ref(child))?;
            }
        }
        Ok(())
    }
    pub fn set_object_visible(&mut self, object: &ObjectRef, visible: bool) -> Result<()> {
        self.edit_object_state(std::slice::from_ref(object), StateEdit::Hidden(!visible))
    }
    pub fn set_objects_locked(&mut self, objects: &[ObjectRef], locked: bool) -> Result<()> {
        self.edit_object_state(objects, StateEdit::Locked(locked))
    }
    fn edit_object_state(&mut self, objects: &[ObjectRef], change: StateEdit) -> Result<()> {
        let index = self.object_state_index();
        ensure!(!objects.is_empty(), "Select an object first");
        for object in objects {
            ensure!(
                index.contains(object),
                "Object does not support editor state: {}",
                object.id
            );
        }
        match change {
            StateEdit::Locked(_) => self.cancel_edit(),
            StateEdit::Hidden(_) => self.finish_edit()?,
        }
        let mut state = self.model().editor_state().clone();
        for object in objects {
            let flags = state.objects.entry(object.id.clone()).or_default();
            match change {
                StateEdit::Locked(value) => flags.locked = value,
                StateEdit::Hidden(value) => flags.hidden_in_editor = value,
            }
        }
        let label = match change {
            StateEdit::Locked(true) => "Lock objects",
            StateEdit::Locked(false) => "Unlock objects",
            StateEdit::Hidden(true) => "Hide in editor",
            StateEdit::Hidden(false) => "Show in editor",
        };
        let (_, receipt) = self
            .session
            .edit(label, None, |edit| edit.replace_editor_state(state))?;
        self.session.acknowledge_receipts();
        if receipt.changed {
            self.invalidate_projection(false);
            self.pristine_blank = None;
        }
        Ok(())
    }
    pub fn select_canvas_objects(&mut self, objects: Vec<ObjectRef>) -> Result<()> {
        self.ensure_editable(&objects)?;
        self.select_objects(objects)
    }
    pub(crate) fn evaluation_revision(&self) -> u64 {
        self.session.evaluation_revision()
    }
}

#[derive(Clone, Copy)]
enum StateEdit {
    Hidden(bool),
    Locked(bool),
}

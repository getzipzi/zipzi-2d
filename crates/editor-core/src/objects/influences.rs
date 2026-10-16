//! Direct parameter targets and transitive deformation/Glue influence paths.
use super::{ObjectIndex, ObjectKind, ObjectRef, RelationKind};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InfluencePath {
    pub target: ObjectRef,
    pub path: Vec<ObjectRef>,
    pub blend: bool,
}
#[derive(Clone, Debug, Default)]
pub struct ParameterBindings {
    pub regular: Vec<ObjectRef>,
    pub blend: Vec<ObjectRef>,
    pub indirect: Vec<InfluencePath>,
}
impl ParameterBindings {
    pub fn direct_count(&self, index: &ObjectIndex) -> usize {
        self.regular
            .iter()
            .filter_map(|r| index.binding(r))
            .map(|b| &b.target)
            .collect::<HashSet<_>>()
            .len()
    }
    pub fn indirect_count(&self) -> usize {
        self.indirect
            .iter()
            .map(|p| &p.target)
            .collect::<HashSet<_>>()
            .len()
    }
}
impl ObjectIndex {
    pub(super) fn build_influences(&mut self) {
        let mut children: HashMap<ObjectRef, Vec<ObjectRef>> = HashMap::new();
        for e in &self.relations {
            if e.kind == RelationKind::DeformParent {
                children
                    .entry(e.target.clone())
                    .or_default()
                    .push(e.source.clone());
            } else if matches!(e.kind, RelationKind::GlueA | RelationKind::GlueB) {
                children
                    .entry(e.source.clone())
                    .or_default()
                    .push(e.target.clone());
            }
        }
        fn walk(
            children: &HashMap<ObjectRef, Vec<ObjectRef>>,
            node: &ObjectRef,
            path: &mut Vec<ObjectRef>,
            out: &mut Vec<InfluencePath>,
            blend: bool,
        ) {
            for child in children.get(node).into_iter().flatten() {
                if path.contains(child) {
                    continue;
                }
                path.push(child.clone());
                out.push(InfluencePath {
                    target: child.clone(),
                    path: path.clone(),
                    blend,
                });
                walk(children, child, path, out, blend);
                path.pop();
            }
        }
        for (id, p) in &mut self.parameters {
            for r in p.regular.iter().chain(&p.blend) {
                let b = &self.bindings[self.binding_slots[r]];
                let mut path = vec![
                    ObjectRef::new(ObjectKind::Parameter, id),
                    r.clone(),
                    b.target.clone(),
                ];
                walk(&children, &b.target, &mut path, &mut p.indirect, b.blend);
            }
        }
    }
}

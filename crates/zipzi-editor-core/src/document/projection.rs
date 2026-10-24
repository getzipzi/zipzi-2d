//! Parameter and hierarchy presentation derived from the current document.
use super::*;

impl Document {
    pub fn parameters(&self) -> Vec<ParameterValue> {
        let groups: HashMap<_, _> = self
            .model()
            .display_info()
            .parameters
            .iter()
            .flatten()
            .filter_map(|entry| match entry {
                zipzi_core::document::CdiParameterEntry::Resolved {
                    parameter_id,
                    group_id,
                    ..
                } => Some((parameter_id.as_str(), group_id.clone())),
                _ => None,
            })
            .collect();
        sorting::ordered_parameter_ids(self.model())
            .iter()
            .filter_map(|id| {
                let parameter = self.model().get_parameter(id)?;
                Some(ParameterValue {
                    id: id.clone(),
                    group_id: groups.get(id.as_str()).cloned().flatten(),
                    name: if parameter.name.is_empty() {
                        parameter.runtime_id.clone()
                    } else {
                        parameter.name.clone()
                    },
                    minimum: parameter.minimum,
                    maximum: parameter.maximum,
                    default_value: parameter.default_value,
                    value: self
                        .session
                        .preview_values()
                        .get(id)
                        .copied()
                        .unwrap_or(parameter.default_value)
                        .clamp(parameter.minimum, parameter.maximum),
                    decimal_places: parameter.decimal_places.clamp(0, 9) as usize,
                })
            })
            .collect()
    }

    pub fn parameter_model_revision(&self) -> u64 {
        self.projection_revision
    }

    pub fn parameter_visuals(&self) -> HashMap<String, ParameterVisual> {
        let mut visuals = self.parameter_visuals_for_targets(
            &self
                .context
                .selections
                .iter()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>(),
        );
        if let Some(b) = self
            .context
            .active_binding
            .as_ref()
            .and_then(|r| self.object_index().binding(r).cloned())
        {
            for axis in &b.axes {
                if let Some(v) = visuals.get_mut(&axis.parameter_id) {
                    v.keys = axis
                        .keys
                        .iter()
                        .map(|&value| ParameterKey {
                            value,
                            selected: true,
                            partial: false,
                            targets: vec![b.target.id.clone()],
                            base: false,
                        })
                        .collect();
                }
            }
        }
        visuals
    }

    /// Aggregate keys for an explicit selection without losing target coverage.
    pub fn parameter_visuals_for_targets(
        &self,
        selected_targets: &[String],
    ) -> HashMap<String, ParameterVisual> {
        let selected_targets: std::collections::HashSet<_> = selected_targets.iter().collect();
        let model = self.model();
        let mut visuals: HashMap<_, _> = self
            .session
            .parameter_ids()
            .iter()
            .filter_map(|id| {
                let p = model.get_parameter(id)?;
                Some((
                    id.clone(),
                    ParameterVisual {
                        kind: p.kind,
                        repeat: p.repeat,
                        keys: Vec::new(),
                    },
                ))
            })
            .collect();
        let mut add_axis = |parameter_id: &str, values: &[f32], target: &str, base: Option<f32>| {
            if let Some(visual) = visuals.get_mut(parameter_id) {
                for &value in values {
                    if let Some(key) = visual.keys.iter_mut().find(|key| key.value == value) {
                        if !key.targets.iter().any(|id| id == target) {
                            key.targets.push(target.to_owned());
                        }
                        key.base |= base == Some(value);
                    } else {
                        visual.keys.push(ParameterKey {
                            value,
                            selected: false,
                            partial: false,
                            targets: vec![target.to_owned()],
                            base: base == Some(value),
                        });
                    }
                }
            }
        };
        for id in model.binding_order() {
            if let Some(binding) = model.get_binding(id) {
                for axis in &binding.axes {
                    add_axis(&axis.parameter_id, &axis.keys, &binding.mesh_id, None);
                }
            }
        }
        for id in model.scene_binding_order() {
            if let Some(binding) = model.get_scene_binding(id) {
                for axis in &binding.axes {
                    add_axis(&axis.parameter_id, &axis.keys, binding.target_id(), None);
                }
            }
        }
        for id in model.glue_order() {
            if let Some(binding) = model.get_glue(id).and_then(|glue| glue.binding.as_ref()) {
                for axis in &binding.axes {
                    add_axis(&axis.parameter_id, &axis.keys, id, None);
                }
            }
        }
        // A key table alone is not an object binding (it can exist unused).
        for id in model.blend_binding_order() {
            if let Some(binding) = model.get_blend_binding(id) {
                if let Some(table) = model.get_blend_key_table(&binding.key_table_id) {
                    add_axis(
                        &table.parameter_id,
                        &table.keys,
                        &binding.target_id,
                        table.keys.get(table.base_key_idx).copied(),
                    );
                }
            }
        }
        for visual in visuals.values_mut() {
            for key in &mut visual.keys {
                key.targets.sort();
                let count = key
                    .targets
                    .iter()
                    .filter(|id| selected_targets.contains(id))
                    .count();
                key.selected = count > 0;
                key.partial = count > 0 && count < selected_targets.len();
            }
            visual.keys.sort_by(|a, b| a.value.total_cmp(&b.value));
        }
        visuals
    }

    pub fn mesh_tree(&self) -> Vec<TreeNode> {
        self.structure_tree(false)
    }
    pub fn deformer_tree(&self) -> Vec<TreeNode> {
        self.structure_tree(true)
    }
    fn structure_tree(&self, deformer: bool) -> Vec<TreeNode> {
        use crate::{ObjectKind as K, RelationKind as R};
        let index = self.object_index();
        let mut nodes = Vec::new();
        let mut references = Vec::new();
        for x in &index.objects {
            let kind = match x.reference.kind {
                K::Part if !deformer => TreeNodeKind::Part,
                K::Mesh => TreeNodeKind::Mesh,
                K::Deformer => match self
                    .model()
                    .get_transform(&x.reference.id)
                    .map(|t| t.kind())
                {
                    Some(zipzi_core::TransformKind::Rotation) => TreeNodeKind::RotationDeformer,
                    _ => TreeNodeKind::WarpDeformer,
                },
                K::Offscreen if !deformer => TreeNodeKind::Offscreen,
                _ => continue,
            };
            let mut node = TreeNode::new(x.reference.id.clone(), x.name.clone(), kind);
            node.diagnostic = x.diagnostic.clone();
            node.visible = Some(
                !self
                    .model()
                    .editor_state()
                    .get(&x.reference.id)
                    .hidden_in_editor,
            );
            if kind == TreeNodeKind::Mesh {
                node.triangles = self.model().get_mesh(&node.id).map(|m| m.triangles.len());
            }
            references.push(x.reference.clone());
            nodes.push(node);
        }
        let slots: HashMap<_, _> = references.iter().enumerate().map(|(i, r)| (r, i)).collect();
        let relation = if deformer {
            R::DeformParent
        } else {
            R::PartOwner
        };
        let mut parents = Vec::new();
        let mut unowned = Vec::new();
        let mut broken = Vec::new();
        for (i, r) in references.iter().enumerate() {
            let edge = index.outgoing(r).find(|e| e.kind == relation);
            let parent = edge.and_then(|e| slots.get(&e.target)).copied();
            if let Some(e) = edge.filter(|_| parent.is_none()) {
                nodes[i].diagnostic = Some(format!("Missing parent: {}", e.target.id));
                broken.push(i);
            } else if parent.is_none()
                && (r.kind == K::Mesh
                    || (!deformer && matches!(r.kind, K::Deformer | K::Offscreen)))
            {
                unowned.push(i);
            }
            parents.push(parent);
        }
        for i in 0..nodes.len() {
            let mut visited = std::collections::HashSet::new();
            let mut cursor = Some(i);
            while let Some(j) = cursor {
                if !visited.insert(j) {
                    nodes[i].diagnostic = Some("Parent cycle".into());
                    broken.push(i);
                    break;
                }
                cursor = parents[j];
            }
        }
        let mut add_group = |id: &str, name: &str, items: &[usize], nodes: &mut Vec<TreeNode>| {
            if !items.is_empty() {
                let slot = nodes.len();
                nodes.push(TreeNode::new(id.into(), name.into(), TreeNodeKind::Group));
                for &i in items {
                    parents[i] = Some(slot);
                }
                parents.push(None);
            }
        };
        unowned.retain(|i| !broken.contains(i));
        add_group(
            "unowned",
            if deformer {
                "No deformer parent"
            } else {
                "Unassigned to Part"
            },
            &unowned,
            &mut nodes,
        );
        add_group(
            "invalid",
            "Missing references / cycles",
            &broken,
            &mut nodes,
        );
        let mut tree = nest_nodes(nodes, parents);
        let order = self.model().hierarchy_order();
        let ids = if deformer {
            &order.deformation
        } else {
            &order.organization
        };
        let ranks: HashMap<_, _> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        fn sort(nodes: &mut [TreeNode], ranks: &HashMap<&str, usize>) {
            nodes.sort_by_key(|node| ranks.get(node.id.as_str()).copied().unwrap_or(usize::MAX));
            for node in nodes {
                sort(&mut node.children, ranks);
            }
        }
        sort(&mut tree, &ranks);
        tree
    }
}

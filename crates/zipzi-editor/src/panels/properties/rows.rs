//! Cached list projection and folding. Rendering only constructs visible controls.
use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Row {
    TextureAction {
        object: ObjectRef,
        control: crate::authoring::texture::Control,
        label: SharedString,
    },
    BindingAction {
        object: ObjectRef,
        control: crate::authoring::binding::Control,
        label: SharedString,
    },
    BindingNavigate {
        object: ObjectRef,
        label: SharedString,
    },
    Group {
        key: String,
        label: String,
        collapsed: bool,
    },
    Array {
        key: String,
        label: String,
        count: usize,
        collapsed: bool,
    },
    Value {
        field: String,
        array_key: Option<String>,
        label: String,
        item_index: Option<usize>,
        value: String,
    },
    Field(PropertyKey),
    Navigation,
    AnimationActions(ObjectRef),
    BusinessActions(ObjectRef),
    Detail(properties::DetailRow),
}
impl Row {
    fn same_layout(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Value { field: a, .. }, Self::Value { field: b, .. }) => a == b,
            _ => self == other,
        }
    }
}

#[derive(PartialEq)]
pub(super) struct RowProjectionKey {
    texture_revision: Option<u64>,
    texture_loading: bool,
    binding_revision: Option<u64>,
    target: PropertyTarget,
    revision: u64,
    details: Option<properties::DetailKey>,
    locale: String,
    fields: Vec<PropertyKey>,
    navigation: bool,
    folds: u64,
    row_height: Pixels,
    editable: bool,
}

impl PropertiesPanel {
    pub(super) fn sync_rows(
        &mut self,
        editor: &zipzi_editor_core::Editor,
        document: DocumentId,
        doc: &zipzi_editor_core::Document,
        cx: &App,
    ) {
        let Some(target) = self.target.as_ref() else {
            return;
        };
        let details_expanded = !self.collapsed.contains("$details");
        let key = RowProjectionKey {
            texture_revision: (target.object.kind == zipzi_editor_core::ObjectKind::Texture)
                .then(|| doc.model_generation()),
            texture_loading: self
                .owner
                .upgrade()
                .is_some_and(|o| o.read(cx).texture_loading(&target.object.id)),
            binding_revision: (target.context == zipzi_editor_core::EditingContext::Modeling
                && matches!(
                    target.object.kind,
                    zipzi_editor_core::ObjectKind::Mesh
                        | zipzi_editor_core::ObjectKind::Deformer
                        | zipzi_editor_core::ObjectKind::Part
                        | zipzi_editor_core::ObjectKind::MeshBinding
                        | zipzi_editor_core::ObjectKind::SceneBinding
                        | zipzi_editor_core::ObjectKind::BindingAxis
                        | zipzi_editor_core::ObjectKind::BindingSample
                ))
            .then(|| doc.model_generation()),
            target: target.clone(),
            revision: if details_expanded || self.definition.navigation.is_some() {
                doc.properties_revision()
            } else {
                0
            },
            details: details_expanded
                .then(|| properties::Details::key(editor, Some(target)))
                .flatten(),
            locale: rust_i18n::locale().to_string(),
            fields: self
                .definition
                .fields()
                .filter(|spec| self.form.fields.contains_key(&spec.binding.key))
                .map(|spec| spec.binding.key)
                .collect(),
            navigation: self.definition.navigation.is_some(),
            folds: self.fold_revision,
            row_height: compact::scaled(compact::ROW + compact::GAP, cx),
            editable: target.editable(editor),
        };
        if self.rows_key.as_ref() == Some(&key) {
            return;
        }
        self.rows_key = Some(key);
        #[cfg(test)]
        {
            self.projection_builds += 1;
        }
        let index = doc.object_index();
        let inspected = self.target.as_ref().unwrap().object.clone();
        let mut rows = Vec::new();
        rows.extend(super::binding::rows(doc, target));
        rows.extend(super::texture::rows(
            doc,
            target,
            self.owner
                .upgrade()
                .is_some_and(|o| o.read(cx).texture_loading(&target.object.id)),
        ));
        if target.timeline.is_none()
            && matches!(
                target.object.kind,
                zipzi_editor_core::ObjectKind::Expression
                    | zipzi_editor_core::ObjectKind::ExpressionEntry
            )
        {
            rows.push(Row::AnimationActions(target.object.clone()));
        }
        if target.timeline.is_none()
            && matches!(
                target.object.kind,
                zipzi_editor_core::ObjectKind::Pose
                    | zipzi_editor_core::ObjectKind::PoseGroup
                    | zipzi_editor_core::ObjectKind::PoseEntry
                    | zipzi_editor_core::ObjectKind::PoseLink
                    | zipzi_editor_core::ObjectKind::Physics
            )
        {
            rows.push(Row::BusinessActions(target.object.clone()));
        }
        let mut section = None;
        let mut entity = None;
        for spec in &self.definition.fields {
            if !self.form.fields.contains_key(&spec.binding.key) {
                continue;
            }
            let next_entity = spec.binding.key.entity_id();
            if spec.section != section || next_entity != entity {
                if let Some(label) = spec.section {
                    rows.push(Row::Detail(properties::DetailRow::Heading(
                        t!(label).into(),
                    )));
                }
                if spec.binding.key.object_kind() == zipzi_editor_core::ObjectKind::ExpressionEntry
                {
                    if let Some(id) = &next_entity {
                        rows.push(Row::AnimationActions(ObjectRef::new(
                            zipzi_editor_core::ObjectKind::ExpressionEntry,
                            id,
                        )));
                    }
                }
                section = spec.section;
                entity = next_entity;
            }
            rows.push(Row::Field(spec.binding.key));
            if let Some(status) =
                properties::animation::registration_status(doc, target, spec.binding.key)
            {
                rows.push(Row::Detail(status));
            }
        }
        if !target.has_object_details() {
            rows.extend(
                properties::selection_details(target, editor)
                    .into_iter()
                    .map(Row::Detail),
            );
        }
        if self.definition.navigation.is_some() {
            rows.push(Row::Navigation);
        }
        let collapsed = self.collapsed.contains("$details");
        if target.has_object_details() {
            rows.push(Row::Group {
                key: "$details".into(),
                label: t!("properties.details").into_owned(),
                collapsed,
            });
        }
        if !collapsed && target.has_object_details() {
            rows.extend(
                self.details
                    .rows(editor, Some(target))
                    .iter()
                    .cloned()
                    .map(Row::Detail),
            );
            let key = (
                document,
                inspected.clone(),
                doc.properties_revision(),
                rust_i18n::locale().to_string(),
            );
            if self.readonly_key.as_ref() != Some(&key) {
                let fields: Vec<_> = index
                    .get(&inspected)
                    .map_or_else(Vec::new, |info| {
                        crate::panels::document_panel::property_rows(&info.properties)
                    })
                    .into_iter()
                    .filter(|(field, _)| {
                        (properties::provider(inspected.kind).show_readonly)(field)
                            && !self
                                .definition
                                .fields()
                                .any(|spec| spec.binding.path == field)
                    })
                    .collect();
                let diagnostic = index
                    .get(&inspected)
                    .and_then(|info| info.diagnostic.clone());
                self.readonly_rows = build_readonly_rows(fields, diagnostic).into();
                self.readonly_key = Some(key);
                self.readonly_builds += 1;
            }
            for row in self.readonly_rows.iter() {
                match row {
                    Row::Array {
                        key, label, count, ..
                    } => {
                        let collapsed = self.collapsed.contains(&format!("$array:{key}"));
                        rows.push(Row::Array {
                            key: key.clone(),
                            label: label.clone(),
                            count: *count,
                            collapsed,
                        });
                    }
                    Row::Value { array_key, .. } => {
                        if array_key
                            .as_ref()
                            .is_none_or(|key| !self.collapsed.contains(&format!("$array:{key}")))
                        {
                            rows.push(row.clone());
                        }
                    }
                    _ => rows.push(row.clone()),
                }
            }
        }
        let key = target.clone();
        let changed = self.rows.as_ref() != rows.as_slice();
        if self.scroll_key.as_ref() != Some(&key) {
            self.scroll.reset_with_uniform_height(
                rows.len(),
                compact::scaled(compact::ROW + compact::GAP, cx),
            );
            self.scroll_key = Some(key);
        } else if self.rows.len() != rows.len()
            || self.rows.iter().zip(&rows).any(|(a, b)| !a.same_layout(b))
        {
            // Keep the scroll anchor and measured heights outside the changed
            // group, rather than jumping to the top when a group is folded.
            let prefix = self
                .rows
                .iter()
                .zip(&rows)
                .take_while(|(a, b)| a.same_layout(b))
                .count();
            let suffix = self.rows[prefix..]
                .iter()
                .rev()
                .zip(rows[prefix..].iter().rev())
                .take_while(|(a, b)| a.same_layout(b))
                .count();
            self.scroll.splice(
                prefix..self.rows.len() - suffix,
                rows.len() - prefix - suffix,
            );
            // Newly expanded rows need size estimates too; zero-height unknown
            // rows otherwise make the scrollbar stop at the first few items.
            self.scroll = self
                .scroll
                .clone()
                .with_uniform_item_height(compact::scaled(compact::ROW + compact::GAP, cx));
        }
        if changed {
            self.rows = rows.into();
        }
    }
}

fn parse_array_field(field: &str) -> Option<(String, usize, String)> {
    let close = field.rfind(']')?;
    let open = field[..close].rfind('[')?;
    let index = field[open + 1..close].parse::<usize>().ok()?;
    let array_key = field[..open].to_owned();
    let suffix = field[close + 1..].trim_start_matches('.').to_owned();
    Some((array_key, index, suffix))
}

fn build_readonly_rows(fields: Vec<(String, String)>, diagnostic: Option<String>) -> Vec<Row> {
    let mut counts = std::collections::HashMap::<String, usize>::new();
    for (field, _) in &fields {
        if let Some((array_key, index, _)) = parse_array_field(field) {
            let entry = counts.entry(array_key).or_insert(0);
            *entry = (*entry).max(index + 1);
        }
    }
    let mut rows = Vec::with_capacity(fields.len() + counts.len() + 1);
    let mut current_array: Option<(String, Option<usize>)> = None;
    for (field, value) in fields {
        if let Some((array_key, index, sub_label)) = parse_array_field(&field) {
            let is_new_array = current_array
                .as_ref()
                .is_none_or(|(key, _)| key != &array_key);
            if is_new_array {
                let label = array_key
                    .rsplit_once('.')
                    .map_or(array_key.as_str(), |(_, l)| l)
                    .to_owned();
                let count = counts.get(&array_key).copied().unwrap_or(1);
                rows.push(Row::Array {
                    key: array_key.clone(),
                    label,
                    count,
                    collapsed: false,
                });
                current_array = Some((array_key.clone(), None));
            }
            let prev_index = current_array.as_ref().and_then(|(_, idx)| *idx);
            let item_index = (prev_index != Some(index)).then_some(index);
            current_array = Some((array_key.clone(), Some(index)));
            rows.push(Row::Value {
                field,
                array_key: Some(array_key),
                label: sub_label,
                item_index,
                value,
            });
        } else {
            current_array = None;
            let label = field
                .rsplit_once('.')
                .map_or(field.as_str(), |(_, l)| l)
                .to_owned();
            rows.push(Row::Value {
                field,
                array_key: None,
                label,
                item_index: None,
                value,
            });
        }
    }
    if let Some(diagnostic) = diagnostic {
        rows.push(Row::Value {
            field: "diagnostic".into(),
            array_key: None,
            label: "diagnostic".into(),
            item_index: None,
            value: diagnostic,
        });
    }
    rows
}

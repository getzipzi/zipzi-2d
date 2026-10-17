mod controls;
mod details;

use crate::ui::property_form::{self, PropertyForm, PropertyTarget};
use crate::{EditorView, t, ui::compact};
use controls::FieldControl;
use editor_core::{DocumentId, ObjectRef, PropertyId};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{BasePanel, Panel, PanelEvent},
    scroll::ScrollableElement,
};
use gpui_kit::{prelude::*, *};
use std::sync::Arc;

#[derive(Clone, PartialEq, Eq)]
enum Row {
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
    Field(PropertyId),
    Management,
    Detail(details::Row),
}
impl Row {
    fn same_layout(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Value { field: a, .. }, Self::Value { field: b, .. }) => a == b,
            _ => self == other,
        }
    }
}

pub struct PropertiesPanel {
    target: Option<PropertyTarget>,
    form: PropertyForm,
    definition: property_form::FormDefinition,
    details: details::Details,
    readonly_key: Option<(DocumentId, ObjectRef, u64, String)>,
    readonly_rows: Arc<[Row]>,
    readonly_builds: u64,
    controls: std::collections::HashMap<PropertyId, FieldControl>,
    preview: Option<(PropertyId, editor_core::PropertyEditToken)>,
    epoch: u64,
    context: Option<editor_core::EditingContextToken>,
    snapshot: Option<(PropertyTarget, u64)>,
    disabled: bool,
    visible: bool,
    scroll: ListState,
    scroll_key: Option<(DocumentId, ObjectRef)>,
    rows: Arc<[Row]>,
    #[cfg(test)]
    row_builds: usize,
    collapsed: std::collections::HashSet<String>,
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    _observe: Subscription,
}

impl PropertiesPanel {
    pub fn new(owner: WeakEntity<EditorView>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            cx.on_release(|this: &mut Self, cx| {
                let owner = this.owner.clone();
                let preview = this.preview.take();
                let target = this.target.clone();
                cx.defer(move |cx| {
                    if let Some((id, token)) = preview {
                        let _ = owner.update(cx, |state, cx| {
                            state.editor.cancel_property_edit(&token);
                            if let Some(target) = target {
                                if let Some(form) = state.property_forms.entries.get_mut(&target) {
                                    form.reload_field(id);
                                }
                            }
                            state.redraw(cx);
                        });
                    }
                });
            })
            .detach();
            Self {
                target: None,
                form: PropertyForm::default(),
                definition: Default::default(),
                details: Default::default(),
                readonly_key: None,
                readonly_rows: Arc::from([]),
                readonly_builds: 0,
                controls: Default::default(),
                preview: None,
                epoch: 0,
                context: None,
                snapshot: None,
                disabled: true,
                visible: false,
                scroll: ListState::new(0, ListAlignment::Top, px(24.))
                    .with_uniform_item_height(px(24.)),
                scroll_key: None,
                rows: Arc::from([]),
                #[cfg(test)]
                row_builds: 0,
                collapsed: ["$details".into()].into(),
                focus: cx.focus_handle(),
                _observe: cx.observe_in(
                    &owner.upgrade().unwrap(),
                    window,
                    |this, _, window, cx| {
                        this.sync_form(window, cx);
                        cx.notify();
                    },
                ),
                owner,
            }
        })
    }
}

impl EventEmitter<PanelEvent> for PropertiesPanel {}
impl Focusable for PropertiesPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for PropertiesPanel {
    fn panel_name(&self) -> &'static str {
        "Properties"
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for PropertiesPanel {
    fn inner_padding(&self, _: &App) -> bool {
        false
    }
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!("editor.properties")
    }
}

impl PropertiesPanel {
    fn summary(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let state = owner.read(cx);
        let (Some(doc), Some(target)) = (state.editor.document(), &self.target) else {
            return div().into_any_element();
        };
        let index = doc.object_index();
        let inspected = &target.object;
        let info = index.get(inspected);
        let name = index.name(inspected).to_owned();
        let tooltip = name.clone();
        let kind = match inspected.kind {
            editor_core::ObjectKind::Mesh => "ArtMesh".to_owned(),
            editor_core::ObjectKind::Deformer => {
                let rotation = info.is_some_and(|info| info.properties["kind"] == "rotation");
                t!(if rotation {
                    "editor.tree_type_rotation"
                } else {
                    "editor.tree_type_warp"
                })
                .into_owned()
            }
            kind => {
                let key = format!("properties.type_{kind:?}");
                t!(&key).into_owned()
            }
        };
        let locked = doc
            .ensure_editable(std::slice::from_ref(inspected))
            .is_err();
        div()
            .id("property-identity")
            .test_support()
            .min_h(compact::rem(compact::DOCK_HEADER))
            .flex_shrink_0()
            .w_full()
            .min_w_0()
            .px(compact::rem(compact::PADDING))
            .py(compact::rem(compact::GAP))
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .items_center()
            .gap(compact::rem(compact::PADDING))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(compact::rem(compact::PADDING))
                    .child(
                        div()
                            .id("property-title-name")
                            .test_support()
                            .aria_label(name.clone())
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .tooltip(move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(tooltip.clone())
                                    .build(window, cx)
                            })
                            .child(name),
                    )
                    .child(
                        div()
                            .id("property-title-type")
                            .test_support()
                            .aria_label(kind.clone())
                            .flex_shrink_0()
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(kind),
                    ),
            )
            .when(locked, |element| {
                element.child(
                    div()
                        .id("property-lock")
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .child(
                            gpui_kit::component::Icon::new(gpui_kit::assets::IconName::Lock)
                                .xsmall(),
                        )
                        .tooltip(|window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(
                                t!("editor.object_locked").into_owned(),
                            )
                            .build(window, cx)
                        }),
                )
            })
            .into_any_element()
    }

    fn management(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let state = owner.read(cx);
        let (Some(doc), Some(target)) = (state.editor.document(), &self.target) else {
            return div().into_any_element();
        };
        let inspected = &target.object;
        let document = target.document;
        let mut body = div().flex().gap(compact::rem(compact::GAP));
        if inspected.kind == editor_core::ObjectKind::ParameterGroup {
            use super::parameters::ParameterAction;
            let actions = [
                ParameterAction::EditGroup(inspected.id.clone()),
                ParameterAction::DeleteGroup(inspected.id.clone()),
            ];
            for (i, action) in actions.into_iter().enumerate() {
                let owner = self.owner.clone();
                body = body.child(
                    compact::button(Button::new(if i == 0 {
                        "property-edit-parameter"
                    } else {
                        "property-delete-parameter"
                    }))
                    .label(action.label())
                    .disabled(
                        state.modal
                            || state.editor.is_busy()
                            || doc
                                .ensure_editable(std::slice::from_ref(&inspected))
                                .is_err(),
                    )
                    .on_click(move |_, window, cx| {
                        let Some(owner) = owner.upgrade() else {
                            return;
                        };
                        if owner.read(cx).editor.active_id() != Some(document) {
                            return;
                        }
                        let panel = owner.read(cx).dock.as_ref().map(|d| d.parameters.clone());
                        if let Some(panel) = panel {
                            panel.update(cx, |p, cx| p.open_management(action.clone(), window, cx));
                        }
                    }),
                );
            }
        }
        body.into_any_element()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn properties_cache_stats(&self) -> (u64, usize) {
        (self.readonly_builds + self.details.builds, self.rows.len())
    }

    #[allow(dead_code)]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn last_row_id(&self) -> Option<ElementId> {
        match self.rows.last()? {
            Row::Value { field, .. } => {
                Some(SharedString::from(format!("object-property-{field}")).into())
            }
            Row::Detail(row) => row.id(),
            Row::Field(id) => {
                Some(SharedString::from(format!("object-property-{}", id.path())).into())
            }
            Row::Group { key, .. } => {
                Some(SharedString::from(format!("property-group-{key}")).into())
            }
            Row::Array { key, .. } => {
                Some(SharedString::from(format!("property-array-{key}")).into())
            }
            Row::Management => Some("property-delete-parameter".into()),
        }
    }

    #[cfg(test)]
    pub(crate) fn row_builds(&self) -> usize {
        self.row_builds
    }

    #[cfg(test)]
    pub(crate) fn field_input(
        &self,
        id: PropertyId,
    ) -> Entity<gpui_kit::component::input::InputState> {
        self.controls[&id].input.clone()
    }

    fn render_row(&mut self, row: &Row, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(test)]
        {
            self.row_builds += 1;
        }
        let content = match row {
            Row::Field(id) => self.render_field(*id, window, cx),
            Row::Management => self.management(cx),
            Row::Detail(row) => row.render(cx),
            Row::Group {
                key,
                label,
                collapsed,
            } => {
                let label = label.clone();
                let key = key.clone();
                Button::new(SharedString::from(format!("property-group-{key}")))
                    .ghost()
                    .xsmall()
                    .h(compact::rem(compact::ROW))
                    .w_full()
                    .justify_start()
                    .accessibility_label(label.clone())
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .items_center()
                            .gap(compact::rem(6.))
                            .child(
                                gpui_kit::component::Icon::new(if *collapsed {
                                    gpui_kit::assets::IconName::ChevronRight
                                } else {
                                    gpui_kit::assets::IconName::ChevronDown
                                })
                                .xsmall(),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(cx.theme().foreground)
                                    .child(label),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.collapsed.remove(&key) {
                            this.collapsed.insert(key.clone());
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            }
            Row::Array {
                key,
                label,
                count,
                collapsed,
            } => {
                let label = label.clone();
                let collapse_key = format!("$array:{key}");
                let count = *count;
                Button::new(SharedString::from(format!("property-array-{key}")))
                    .ghost()
                    .xsmall()
                    .h(compact::rem(compact::ROW))
                    .w_full()
                    .justify_start()
                    .accessibility_label(label.clone())
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap(compact::rem(compact::PADDING))
                            .child(
                                div()
                                    .flex()
                                    .min_w_0()
                                    .items_center()
                                    .gap(compact::rem(4.))
                                    .child(
                                        gpui_kit::component::Icon::new(if *collapsed {
                                            gpui_kit::assets::IconName::ChevronRight
                                        } else {
                                            gpui_kit::assets::IconName::ChevronDown
                                        })
                                        .xsmall(),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(cx.theme().muted_foreground)
                                            .child(label),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .px(compact::rem(5.))
                                    .rounded(compact::rem(compact::RADIUS))
                                    .bg(cx.theme().muted)
                                    .text_size(compact::rem(compact::CAPTION))
                                    .text_color(cx.theme().muted_foreground)
                                    .font_family(".AppleSystemUIFontMonospaced")
                                    .child(count.to_string()),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.collapsed.remove(&collapse_key) {
                            this.collapsed.insert(collapse_key.clone());
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            }
            Row::Value {
                field,
                array_key,
                label,
                item_index,
                value,
            } => {
                let value_el = render_readonly_value(value, cx);
                if array_key.is_some() {
                    let tooltip = SharedString::from(match (item_index, label.is_empty()) {
                        (Some(idx), true) => format!("#{idx}"),
                        (Some(idx), false) => format!("#{idx} {label}"),
                        (None, _) => label.clone(),
                    });
                    div()
                        .id(SharedString::from(format!("object-property-{field}")))
                        .test_support()
                        .flex()
                        .flex_wrap()
                        .items_start()
                        .min_w_0()
                        .min_h(compact::rem(compact::ROW))
                        .gap(compact::rem(compact::GAP))
                        .text_size(compact::rem(compact::BODY))
                        .line_height(compact::rem(16.))
                        .ml(compact::rem(2.))
                        .pl(compact::rem(7.))
                        .border_l_1()
                        .border_color(cx.theme().border)
                        .child(
                            div()
                                .id(SharedString::from(format!("property-label-{field}")))
                                .test_support()
                                .w(compact::rem(122.))
                                .min_w_0()
                                .py(compact::rem(3.))
                                .flex()
                                .items_center()
                                .gap(compact::rem(4.))
                                .text_color(cx.theme().muted_foreground)
                                .tooltip(move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(tooltip.clone())
                                        .build(window, cx)
                                })
                                .child(
                                    div()
                                        .w(compact::rem(20.))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .when_some(*item_index, |slot, idx| {
                                            slot.child(
                                                div()
                                                    .px(compact::rem(3.))
                                                    .rounded(compact::rem(2.))
                                                    .bg(cx.theme().muted)
                                                    .text_size(compact::rem(compact::CAPTION))
                                                    .line_height(compact::rem(14.))
                                                    .font_family(".AppleSystemUIFontMonospaced")
                                                    .child(format!("#{idx}")),
                                            )
                                        }),
                                )
                                .when(!label.is_empty(), |el| {
                                    el.child(div().min_w_0().truncate().child(label.clone()))
                                }),
                        )
                        .child(
                            div()
                                .flex_grow(1.)
                                .flex_basis(compact::rem(70.))
                                .min_w_0()
                                .min_h(compact::rem(compact::CONTROL))
                                .flex()
                                .items_center()
                                .child(value_el),
                        )
                        .into_any_element()
                } else {
                    compact::property_row(label.clone(), value_el, cx)
                        .id(SharedString::from(format!("object-property-{field}")))
                        .test_support()
                        .into_any_element()
                }
            }
        };
        div()
            .w_full()
            .px(compact::rem(compact::PADDING))
            .pb(compact::rem(compact::GAP))
            .child(content)
            .into_any_element()
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

fn render_readonly_value(value: &str, cx: &App) -> AnyElement {
    if value.is_empty() || value == "[]" {
        return div()
            .id("property-value")
            .test_support()
            .w_full()
            .min_w_0()
            .flex()
            .justify_end()
            .child(
                div()
                    .px(compact::rem(4.))
                    .rounded(compact::rem(2.))
                    .bg(cx.theme().muted)
                    .text_size(compact::rem(compact::CAPTION))
                    .text_color(cx.theme().muted_foreground)
                    .child("—"),
            )
            .into_any_element();
    }
    if value.starts_with('[') && value.ends_with(']') {
        if let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(value) {
            if !items.is_empty() {
                let strings: Vec<String> = items
                    .iter()
                    .map(|item| match item {
                        serde_json::Value::String(s) => s.clone(),
                        _ => item.to_string(),
                    })
                    .collect();
                let tooltip = SharedString::from(strings.join(", "));
                let max_inline = 4;
                let shown = strings.len().min(max_inline);
                let extra = strings.len().saturating_sub(max_inline);
                let mut row = div()
                    .id("property-value")
                    .test_support()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .items_center()
                    .gap(compact::rem(compact::GAP))
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(tooltip.clone())
                            .build(window, cx)
                    });
                for item in strings.into_iter().take(shown) {
                    let numeric = item.parse::<f64>().is_ok();
                    row = row.child(
                        div()
                            .max_w(compact::rem(84.))
                            .truncate()
                            .px(compact::rem(4.))
                            .rounded(compact::rem(2.))
                            .bg(cx.theme().muted)
                            .text_size(compact::rem(compact::CAPTION))
                            .when(numeric, |pill| {
                                pill.font_family(".AppleSystemUIFontMonospaced")
                            })
                            .child(item),
                    );
                }
                if extra > 0 {
                    row = row.child(
                        div()
                            .px(compact::rem(4.))
                            .rounded(compact::rem(2.))
                            .bg(cx.theme().muted)
                            .text_size(compact::rem(compact::CAPTION))
                            .text_color(cx.theme().muted_foreground)
                            .font_family(".AppleSystemUIFontMonospaced")
                            .child(format!("+{extra}")),
                    );
                }
                return row.into_any_element();
            }
        }
    }
    compact::property_value(value.to_owned()).into_any_element()
}

impl Render for PropertiesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_form(window, cx);
        let root = div()
            .id("properties-panel")
            .test_support()
            .size_full()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|this, action: &crate::actions::RunCommand, window, cx| {
                    if !matches!(
                        action.id,
                        editor_core::CommandId::CommitProperty
                            | editor_core::CommandId::CancelProperty
                    ) {
                        cx.propagate();
                        return;
                    }
                    if action.invocation.is_some_and(|command| {
                        command.document() != this.target.as_ref().map(|target| target.document)
                    }) {
                        cx.stop_propagation();
                        return;
                    }
                    this.property_action(action.id, window, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    && this.preview.is_some()
                    && this.escape(window, cx)
                {
                    cx.stop_propagation();
                }
            }))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_xs();
        let Some(owner) = self.owner.upgrade() else {
            return root.into_any_element();
        };
        let state = owner.read(cx);
        let Some(doc) = state.editor.document() else {
            self.scroll_key = None;
            self.rows = Arc::from([]);
            self.scroll.reset(0);
            return root
                .child(div().p_2().child(t!("editor.no_document")))
                .into_any_element();
        };
        let Some(document) = state.editor.active_id() else {
            return root.into_any_element();
        };
        if self.target.is_none() {
            self.rows = Arc::from([]);
            self.scroll_key = None;
            self.scroll.reset(0);
            return root
                .child(
                    div()
                        .id("properties-empty")
                        .test_support()
                        .p_3()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("workspace.select_motion").into_owned()),
                )
                .into_any_element();
        }
        let index = doc.object_index();
        let inspected = state
            .editor
            .active_inspection()
            .and_then(|source| source.object())
            .unwrap_or_else(|| index.objects[0].reference.clone());
        let mut rows = Vec::new();
        rows.extend(
            self.definition
                .fields
                .iter()
                .filter(|spec| self.form.fields.contains_key(&spec.binding.id))
                .map(|spec| Row::Field(spec.binding.id)),
        );
        if inspected.kind == editor_core::ObjectKind::ParameterGroup {
            rows.push(Row::Management);
        }
        let collapsed = self.collapsed.contains("$details");
        rows.push(Row::Group {
            key: "$details".into(),
            label: t!("properties.details").into_owned(),
            collapsed,
        });
        if !collapsed {
            if let Some(animation) = state.editor.animation().filter(|_| {
                self.target.as_ref().map(|target| target.context)
                    == Some(editor_core::EditingContext::Animation)
            }) {
                rows.extend(
                    self.details
                        .rows(doc, document, animation)
                        .iter()
                        .cloned()
                        .map(Row::Detail),
                );
            }
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
                        super::document_panel::property_rows(&info.properties)
                    })
                    .into_iter()
                    .filter(|(field, _)| {
                        !self
                            .definition
                            .fields()
                            .any(|spec| spec.binding.id.path() == field)
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
        let key = (document, inspected);
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
        let rows = self.rows.clone();
        let panel = cx.entity();
        let summary = self.summary(cx);
        root.child(summary)
            .child(
                div()
                    .id("object-properties")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .relative()
                    .overflow_hidden()
                    .child(
                        list(self.scroll.clone(), move |index, window, cx| {
                            panel.update(cx, |this, cx| this.render_row(&rows[index], window, cx))
                        })
                        .size_full()
                        .py(compact::rem(compact::PADDING)),
                    )
                    .vertical_scrollbar(&self.scroll),
            )
            .into_any_element()
    }
}

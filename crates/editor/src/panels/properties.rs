mod controls;
mod observe;
mod rich_controls;
mod rows;
mod selection;
use crate::properties;
use crate::ui::property_details as details;
use rows::{Row, RowProjectionKey};

use crate::properties::{PropertyTarget, form::PropertyForm};
use crate::{EditorView, t, ui::compact};
use controls::FieldControl;
use editor_core::{DocumentId, ObjectRef, PropertyKey};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{BasePanel, Panel, PanelEvent},
    scroll::ScrollableElement,
};
use gpui_kit::{prelude::*, *};
use std::sync::Arc;

pub struct PropertiesPanel {
    pub(crate) timeline_id: editor_core::TimelineId,
    target: Option<PropertyTarget>,
    form: PropertyForm,
    definition: properties::FormDefinition,
    details: properties::Details,
    readonly_key: Option<(DocumentId, ObjectRef, u64, String)>,
    readonly_rows: Arc<[Row]>,
    readonly_builds: u64,
    controls: std::collections::HashMap<PropertyKey, FieldControl>,
    selects: std::collections::HashMap<PropertyKey, controls::SelectControl>,
    colors: std::collections::HashMap<PropertyKey, rich_controls::ColorControl>,
    preview: Option<(PropertyKey, properties::PreviewToken)>,
    epoch: u64,
    context: Option<editor_core::EditingContextToken>,
    snapshot: Option<(PropertyTarget, u64, Option<u32>)>,
    disabled: bool,
    visible: bool,
    scroll: ListState,
    scroll_key: Option<PropertyTarget>,
    rows: Arc<[Row]>,
    rows_key: Option<RowProjectionKey>,
    fold_revision: u64,
    #[cfg(test)]
    projection_builds: usize,
    #[cfg(test)]
    choice_builds: usize,
    #[cfg(test)]
    renders: usize,
    #[cfg(test)]
    row_builds: usize,
    collapsed: std::collections::HashSet<String>,
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    observed: Option<observe::ObservedState>,
    _observe: Subscription,
}

impl PropertiesPanel {
    pub fn new(owner: WeakEntity<EditorView>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            cx.on_release(|this: &mut Self, cx| {
                let panel = this.timeline_id;
                let owner = this.owner.clone();
                let preview = this.preview.take();
                let target = this.target.clone();
                cx.defer(move |cx| {
                    if let Some((id, token)) = preview {
                        let _ = owner.update(cx, |state, cx| {
                            state.editor.unregister_timeline(panel);
                            token.cancel(&mut state.editor);
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
                timeline_id: editor_core::TimelineId::new(),
                target: None,
                form: PropertyForm::default(),
                definition: Default::default(),
                details: Default::default(),
                readonly_key: None,
                readonly_rows: Arc::from([]),
                readonly_builds: 0,
                controls: Default::default(),
                selects: Default::default(),
                colors: Default::default(),
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
                rows_key: None,
                fold_revision: 0,
                #[cfg(test)]
                projection_builds: 0,
                #[cfg(test)]
                choice_builds: 0,
                #[cfg(test)]
                renders: 0,
                #[cfg(test)]
                row_builds: 0,
                collapsed: ["$details".into()].into(),
                focus: cx.focus_handle(),
                observed: None,
                _observe: cx.observe_in(
                    &owner.upgrade().unwrap(),
                    window,
                    |this, _, window, cx| {
                        this.observe_owner(window, cx);
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
        let Some(target) = &self.target else {
            return Empty.into_any_element();
        };
        let Some((name, kind, locked)) = target.title(&state.editor) else {
            return Empty.into_any_element();
        };
        crate::ui::object_identity::render_title("property", name, kind, locked, cx)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn properties_cache_stats(&self) -> (u64, usize) {
        (self.readonly_builds + self.details.builds, self.rows.len())
    }

    #[cfg(test)]
    pub(crate) fn work_counts(&self) -> (usize, usize, usize) {
        (self.projection_builds, self.choice_builds, self.renders)
    }

    #[allow(dead_code)]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn last_row_id(&self) -> Option<ElementId> {
        match self.rows.last()? {
            Row::Navigation => Some("property-navigation".into()),
            Row::Value { field, .. } => {
                Some(SharedString::from(format!("object-property-{field}")).into())
            }
            Row::Detail(row) => row.id(),
            Row::Field(id) => {
                Some(SharedString::from(format!("object-property-{}", id.name())).into())
            }
            Row::Group { key, .. } => {
                Some(SharedString::from(format!("property-group-{key}")).into())
            }
            Row::Array { key, .. } => {
                Some(SharedString::from(format!("property-array-{key}")).into())
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn row_builds(&self) -> usize {
        self.row_builds
    }

    #[cfg(test)]
    pub(crate) fn field_input(
        &self,
        id: PropertyKey,
    ) -> Entity<gpui_kit::component::input::InputState> {
        self.controls[&id].input.clone()
    }

    fn render_row(&mut self, row: &Row, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(test)]
        {
            self.row_builds += 1;
        }
        let content = match row {
            Row::Navigation => self.render_navigation(cx),
            Row::Field(id) => self.render_field(*id, window, cx),
            Row::Detail(row) => details::render(row, cx),
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
                        this.fold_revision += 1;
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
                        this.fold_revision += 1;
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
        #[cfg(test)]
        {
            self.renders += 1;
        }
        self.sync_form(window, cx);
        self.remember_observed(cx);
        let root = div()
            .id("properties-panel")
            .test_support()
            .size_full()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                let finish = this.preview.as_ref().is_some_and(|(id, _)| {
                    this.field_spec(*id).live_text
                        && this.controls.get(id).is_some_and(|control| {
                            !control
                                .input
                                .read(cx)
                                .input_bounds()
                                .contains(&event.position)
                        })
                });
                if finish {
                    this.finish_text_preview(cx);
                }
            }))
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
                if event.keystroke.key == "escape" && this.escape(window, cx) {
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
            self.rows_key = None;
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
            self.rows_key = None;
            self.scroll_key = None;
            self.scroll.reset(0);
            return root.into_any_element();
        }
        self.sync_rows(&state.editor, document, doc, cx);
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

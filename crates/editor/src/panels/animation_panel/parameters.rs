//! Read-only projection using the authoring panel's grouping and row components.
use super::*;
use crate::panels::{
    group_header,
    parameters::{
        grouping::{ListRow, ParameterLayout},
        row, track,
    },
};
use editor_core::{DocumentId, ParameterValue};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
struct ViewState {
    collapsed: HashSet<Option<String>>,
    scroll: UniformListScrollHandle,
}

#[derive(Default)]
pub(super) struct PreviewParameters {
    source: Option<(DocumentId, u64)>,
    parameters: Vec<ParameterValue>,
    runtime_ids: HashMap<String, String>,
    layout: ParameterLayout,
    documents: HashMap<DocumentId, ViewState>,
}

impl AnimationPanel {
    pub(super) fn render_parameters(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let state = owner.read(cx);
        let mut content = div()
            .id("AnimationParameters")
            .test_support()
            .key_context("KasaneDocument")
            .track_focus(&self.focus)
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_xs();
        let (Some(document), Some(id)) = (state.editor.document(), state.editor.active_id()) else {
            return content
                .child(div().p_3().child(t!("editor.welcome")))
                .into_any_element();
        };
        let source = (id, document.parameter_model_revision());
        if self.parameters.source != Some(source) {
            self.parameters.parameters = document.parameters();
            self.parameters.layout =
                ParameterLayout::new(&document.parameter_groups(), &self.parameters.parameters);
            self.parameters.runtime_ids = document
                .animation_catalog()
                .parameters
                .iter()
                .map(|p| (p.id.clone(), p.runtime_id.clone()))
                .collect();
            self.parameters.source = Some(source);
        }
        let snapshot = state.displayed_animation();
        // Keep status outside the virtual list so values scroll without losing
        // the reminder that this panel describes the displayed preview frame.
        content = content.child(
            div()
                .id("animation-parameters-status")
                .tooltip(|window, cx| {
                    Tooltip::new(t!("workspace.parameters_hint").into_owned()).build(window, cx)
                })
                .px_3()
                .py_2()
                .flex_shrink_0()
                .border_b_1()
                .border_color(cx.theme().border)
                .text_color(cx.theme().muted_foreground)
                .child(t!("workspace.inspector_frame_hint"))
                .when_some(snapshot, |status, snapshot| {
                    status.child(
                        div()
                            .id("animation-parameters-time")
                            .test_support()
                            .aria_value(snapshot.time.to_string())
                            .child(t!(
                                "workspace.presented_time",
                                time = format!("{:.3}", snapshot.time)
                            )),
                    )
                }),
        );
        if snapshot.is_none() {
            let status = state
                .editor
                .animation()
                .and_then(|a| a.error.as_ref())
                .map(super::super::animation_feedback::failure_text)
                .unwrap_or_else(|| t!("workspace.evaluating").into_owned());
            return content.child(div().p_3().child(status)).into_any_element();
        }
        if self.parameters.parameters.is_empty() {
            return content
                .child(div().p_3().child(t!("workspace.no_parameters")))
                .into_any_element();
        }
        let view = self.parameters.documents.entry(id).or_default();
        let rows = self.parameters.layout.visible(&view.collapsed);
        let scroll = view.scroll.clone();
        let panel = cx.entity();
        content
            .child(
                uniform_list(
                    "animation-parameters-scroll",
                    rows.len(),
                    move |range, _, cx| {
                        panel.update(cx, |this, cx| {
                            range
                                .map(|index| match rows[index].clone() {
                                    ListRow::Group {
                                        id: group,
                                        name,
                                        depth,
                                        count,
                                    } => {
                                        let expanded = !this.parameters.documents[&id]
                                            .collapsed
                                            .contains(&group);
                                        let key = group.as_deref().unwrap_or("ungrouped");
                                        group_header::row_style(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "animation-parameter-group-{key}"
                                                )))
                                                .test_support()
                                                .aria_expanded(expanded),
                                            depth,
                                            cx,
                                        )
                                        .child(
                                            group_header::toggle(
                                                SharedString::from(format!(
                                                    "animation-parameter-group-toggle-{key}"
                                                )),
                                                SharedString::from(format!(
                                                    "animation-parameter-group-count-{key}"
                                                )),
                                                name,
                                                count,
                                                expanded,
                                                depth,
                                                cx,
                                            )
                                            .on_click(
                                                cx.listener(move |this, _, window, cx| {
                                                    if window.has_active_dialog(cx) {
                                                        return;
                                                    }
                                                    let collapsed = &mut this
                                                        .parameters
                                                        .documents
                                                        .entry(id)
                                                        .or_default()
                                                        .collapsed;
                                                    if !collapsed.remove(&group) {
                                                        collapsed.insert(group.clone());
                                                    }
                                                    cx.notify();
                                                }),
                                            ),
                                        )
                                        .into_any_element()
                                    }
                                    ListRow::Parameter { index, depth } => {
                                        let parameter = &this.parameters.parameters[index];
                                        let value = this.owner.upgrade().and_then(|owner| {
                                            owner.read(cx).displayed_animation().and_then(|s| {
                                                s.parameters.get(&parameter.id).copied()
                                            })
                                        });
                                        let runtime_id = this
                                            .parameters
                                            .runtime_ids
                                            .get(&parameter.id)
                                            .cloned()
                                            .unwrap_or_else(|| parameter.id.clone());
                                        preview_row(index, parameter, depth, value, runtime_id, cx)
                                    }
                                })
                                .collect()
                        })
                    },
                )
                .flex_1()
                .min_h_0()
                .track_scroll(&scroll),
            )
            .into_any_element()
    }
}

fn preview_row(
    index: usize,
    parameter: &ParameterValue,
    depth: usize,
    value: Option<f32>,
    runtime_id: String,
    cx: &App,
) -> AnyElement {
    let text = value.map_or_else(|| "—".to_string(), |value| format!("{value:.4}"));
    let tooltip = format!(
        "{} · {}\n{} – {}\n{}",
        parameter.name, runtime_id, parameter.minimum, parameter.maximum, text
    );
    row::style(
        div()
            .id(("animation-parameter", index))
            .test_support()
            .aria_label(parameter.name.clone())
            .aria_value(text.clone()),
        cx,
    )
    .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
    .child(row::name(depth).child(parameter.name.clone()))
    .child(row::track().when_some(value, |track, value| {
        track.child(track::readonly_track(parameter, value, cx))
    }))
    .child(
        row::value()
            .w(px(64.))
            .justify_end()
            .overflow_hidden()
            .child(text),
    )
    .into_any_element()
}

//! Compact mapping tables; cells share the inspector's field and edit lifecycle.
use super::*;
use crate::ui::slide_number_input::format_one_decimal;
use gpui_kit::component::tooltip::Tooltip;
use std::collections::BTreeMap;

struct Column {
    width: Option<f32>,
    label: String,
    help: String,
}
fn columns(output: bool, manual: bool) -> Vec<Column> {
    let mut columns = vec![Column {
        width: None,
        label: t!("physics.parameter").into_owned(),
        help: t!("physics.parameter").into_owned(),
    }];
    if output {
        columns.push(Column {
            width: Some(32.),
            label: "#".into(),
            help: t!("physics.sample_node").into_owned(),
        });
    }
    columns.push(Column {
        width: Some(48.),
        label: t!("physics.axis_short").into_owned(),
        help: t!("physics.axis").into_owned(),
    });
    if output {
        columns.push(Column {
            width: Some(44.),
            label: "×".into(),
            help: t!("physics.scale").into_owned(),
        });
    }
    columns.push(Column {
        width: Some(46.),
        label: "%".into(),
        help: t!("physics.weight").into_owned(),
    });
    columns.push(Column {
        width: Some(26.),
        label: "±".into(),
        help: t!("physics.reflect").into_owned(),
    });
    columns.push(Column {
        width: Some(64.),
        label: t!(if manual {
            "physics.input_value"
        } else {
            "timeline.value"
        })
        .into_owned(),
        help: t!(if manual {
            "physics.test_input"
        } else {
            "timeline.value"
        })
        .into_owned(),
    });
    columns
}
fn cell(column: &Column, content: impl IntoElement, cx: &App) -> Div {
    div()
        .min_w_0()
        .px(compact::rem(2.))
        .flex()
        .items_center()
        .when_some(column.width, |cell, width| {
            cell.w(compact::rem(width)).flex_shrink_0()
        })
        .when(column.width.is_none(), |cell| {
            cell.flex_1().min_w(compact::rem(92.))
        })
        .border_r_1()
        .border_color(cx.theme().border.opacity(0.45))
        .child(div().w_full().min_w_0().child(content))
}

impl PropertiesPanel {
    pub(super) fn physics_table(
        &mut self,
        output: bool,
        asset: &PhysicsAsset,
        parameters: &[editor_core::ParameterValue],
        rig_index: usize,
        trace: Option<&kasane_sdk::PhysicsRigSnapshot>,
        values: &BTreeMap<String, f32>,
        manual: Option<&editor_core::PhysicsPreviewSettings>,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rig = &asset.data.settings[rig_index];
        let columns = columns(output, manual.is_some());
        let mut header = div()
            .flex()
            .items_center()
            .h(compact::rem(compact::HEADER_CONTROL))
            .text_size(compact::rem(compact::CAPTION))
            .text_color(cx.theme().muted_foreground)
            .bg(cx.theme().muted.opacity(0.5))
            .border_b_1()
            .border_color(cx.theme().border);
        for (index, column) in columns.iter().enumerate() {
            let help = column.help.clone();
            header = header.child(cell(
                column,
                div()
                    .id(SharedString::from(format!(
                        "physics-{output}-column-{index}"
                    )))
                    .truncate()
                    .child(column.label.clone())
                    .tooltip(move |window, cx| Tooltip::new(help.clone()).build(window, cx)),
                cx,
            ));
        }
        let mut table = div()
            .w_full()
            .min_w(compact::rem(if output { 352. } else { 280. }))
            .flex()
            .flex_col()
            .child(header);
        let count = if output {
            rig.outputs.len()
        } else {
            rig.inputs.len()
        };
        for i in 0..count {
            let prefix = if output { "output" } else { "input" };
            let selected = if output {
                self.physics.output == i
            } else {
                self.physics.input == i
            };
            let source = if output {
                &rig.outputs[i].destination
            } else {
                &rig.inputs[i].source
            };
            let parameter = crate::properties::physics::field_key(
                if output {
                    "output_parameter"
                } else {
                    "input_parameter"
                },
                i,
                rig,
            );
            let axis = crate::properties::physics::field_key(
                if output { "output_axis" } else { "input_axis" },
                i,
                rig,
            );
            let reflect = crate::properties::physics::field_key(
                if output {
                    "output_reflect"
                } else {
                    "input_reflect"
                },
                i,
                rig,
            );
            let mut cells = vec![self.render_field_control(parameter, window, cx)];
            if output {
                cells.push(self.render_field_control(
                    crate::properties::physics::field_key("output_node", i, rig),
                    window,
                    cx,
                ));
            }
            cells.push(self.render_field_control(axis, window, cx));
            if output {
                cells.push(self.physics_number_control(
                    NumberField::OutputScale(i),
                    t!("physics.scale").into_owned(),
                    rig.outputs[i].scale,
                    (-10000., 10000., 0.1),
                    disabled,
                    window,
                    cx,
                ));
            }
            let weight = if output {
                NumberField::OutputWeight(i)
            } else {
                NumberField::InputWeight(i)
            };
            let current_weight = if output {
                rig.outputs[i].weight
            } else {
                rig.inputs[i].weight
            };
            cells.push(self.physics_number_control(
                weight,
                t!("physics.weight").into_owned(),
                current_weight,
                (0., 100., 0.1),
                disabled,
                window,
                cx,
            ));
            cells.push(self.render_field_control(reflect, window, cx));
            let observed = if output {
                source.parameter_id().and_then(|id| values.get(id)).copied()
            } else {
                trace
                    .and_then(|trace| {
                        trace
                            .input_ids
                            .iter()
                            .position(|id| id == &rig.inputs[i].id)
                            .and_then(|slot| trace.inputs.get(slot))
                    })
                    .copied()
            };
            let mut value = div()
                .id(SharedString::from(format!(
                    "animation-physics-{prefix}-{rig_index}-{i}"
                )))
                .test_support()
                .aria_label(observed.map_or_else(|| "—".into(), |v| format_one_decimal(v as f64)))
                .w_full()
                .text_align(TextAlign::Right)
                .text_size(compact::rem(compact::CAPTION));
            if let Some((settings, parameter)) = manual.and_then(|settings| {
                source
                    .parameter_id()
                    .and_then(|id| parameters.iter().find(|p| &p.id == id))
                    .map(|parameter| (settings, parameter))
            }) {
                let temporary = settings
                    .inputs
                    .iter()
                    .rev()
                    .find(|input| input.parameter == parameter.id)
                    .map_or_else(
                        || {
                            settings
                                .baseline
                                .get(&parameter.id)
                                .copied()
                                .unwrap_or(parameter.default_value)
                        },
                        |input| input.value,
                    );
                value = value.child(self.physics_number_control(
                    NumberField::Manual(parameter.id.clone()),
                    t!("physics.test_input").into_owned(),
                    temporary,
                    (
                        parameter.minimum,
                        parameter.maximum,
                        (parameter.maximum - parameter.minimum) / 200.,
                    ),
                    disabled,
                    window,
                    cx,
                ));
            } else {
                value = value
                    .child(observed.map_or_else(|| "—".into(), |v| format_one_decimal(v as f64)));
            }
            cells.push(value.into_any_element());
            let node = output.then(|| rig.node_index(&rig.outputs[i].node_id).unwrap_or(0));
            let mut row = div()
                .id(SharedString::from(format!("physics-{prefix}-row-{i}")))
                .test_support()
                .role(gpui::Role::Row)
                .aria_selected(selected)
                .flex()
                .items_center()
                .min_h(compact::rem(compact::HEADER_CONTROL + 2.))
                .border_b_1()
                .border_color(cx.theme().border.opacity(0.5))
                .when(selected, |row| row.bg(cx.theme().primary.opacity(0.10)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        if output {
                            this.physics.output = i;
                            if let Some(node) = node {
                                this.physics.node = node;
                            }
                        } else {
                            this.physics.input = i;
                        }
                        cx.notify();
                    }),
                );
            for (column, content) in columns.iter().zip(cells) {
                row = row.child(cell(column, content, cx));
            }
            table = table.child(row);
        }
        div()
            .id(if output {
                "physics-output-table"
            } else {
                "physics-input-table"
            })
            .test_support()
            .w_full()
            .min_w_0()
            .overflow_x_scroll()
            .child(table)
            .into_any_element()
    }
}

//! Independently observed, virtualized parameter row views.
use super::*;
use gpui_kit::component::Icon;

// Each row owns its slider dependency so changing one thumb does not rebuild
// the other visible sliders (including their labels, tooltips and hover rings).
pub(super) struct ParameterRowView {
    pub(super) panel: WeakEntity<ParametersPanel>,
    pub(super) document_id: Option<DocumentId>,
    pub(super) parameter: ParameterValue,
    pub(super) visual: ParameterVisual,
    pub(super) slider: Entity<SliderState>,
    pub(super) input: Entity<InputState>,
    pub(super) number_input: Entity<SlideNumberInput>,
    pub(super) disabled: bool,
    pub(super) summary: String,
    pub(super) directly_related: bool,
    pub(super) indirectly_related: bool,
    pub(super) selected: bool,
    pub(super) cursor: bool,
    pub(super) depth: usize,
    pub(super) _observe: Subscription,
    pub(super) sorting: Entity<SortState>,
    pub(super) _sorting_observe: Subscription,
}

impl Render for ParameterRowView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _span = crate::benchmark::parameter_overlay::ui_span("ui.parameter_row_render");
        let parameter = &self.parameter;
        let slider = &self.slider;
        let disabled = self.disabled;
        let document_id = self.document_id;
        self.number_input.update(cx, |input, cx| {
            input.configure(
                parameter.name.clone(),
                slide_number_options(parameter, disabled),
                cx,
            );
        });
        let theme = cx.theme();
        let value = match slider.read(cx).value() {
            SliderValue::Single(value) => value,
            _ => parameter.value,
        };
        let parameter_id = parameter.id.clone();
        let focus_parameter_id = parameter_id.clone();
        let focus_panel = self.panel.clone();
        let tooltip = format!(
            "{} {}\n{} – {}",
            parameter.name, self.summary, parameter.minimum, parameter.maximum
        );
        div()
            .id(SharedString::from(format!("parameter-row-{parameter_id}")))
            .test_support()
            .aria_selected(self.selected)
            .map(|element| {
                sorting::drop_target(
                    element,
                    &self.sorting,
                    SortItem::Parameter(parameter_id.clone()),
                    None,
                    cx,
                )
            })
            .capture_action(
                cx.listener(|this, action: &crate::actions::RunCommand, window, cx| {
                    use zipzi_editor_core::CommandId;
                    if this.input.read(cx).focus_handle(cx).is_focused(window)
                        && matches!(
                            action.id,
                            CommandId::CommitProperty | CommandId::CancelProperty
                        )
                    {
                        if let Some(id) = this.document_id {
                            let _ = this.panel.update(cx, |panel, cx| {
                                if action.id == CommandId::CancelProperty {
                                    panel.cancel_input(id, &this.parameter.id, window, cx);
                                } else {
                                    panel.commit_input(id, &this.parameter.id, window, cx);
                                }
                            });
                        }
                        cx.stop_propagation();
                    }
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::input::Escape, window, cx| {
                    if this.input.read(cx).focus_handle(cx).is_focused(window) {
                        if let Some(id) = this.document_id {
                            let _ = this.panel.update(cx, |panel, cx| {
                                panel.cancel_input(id, &this.parameter.id, window, cx)
                            });
                        }
                        cx.stop_propagation();
                    }
                }),
            )
            .map(|element| row::style(element, cx))
            .bg(if self.selected {
                theme.primary.opacity(0.15)
            } else {
                gpui::transparent_black()
            })
            .border_1()
            .border_color(if self.cursor {
                theme.primary
            } else {
                gpui::transparent_black()
            })
            .child(
                row::name(self.depth)
                    .id(SharedString::from(format!("parameter-name-{parameter_id}")))
                    .when_some(document_id, |element, document| {
                        sorting::drag_source(
                            element,
                            &self.sorting,
                            document,
                            SortItem::Parameter(parameter_id.clone()),
                            parameter.name.clone(),
                            !disabled,
                        )
                    })
                    .test_support()
                    .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
                    .on_click(move |_, window, cx| {
                        let _ = focus_panel.update(cx, |panel, cx| {
                            if let Some(id) = document_id {
                                panel.focus_parameter(id, &focus_parameter_id, window, cx);
                            }
                        });
                    })
                    .text_color(if self.directly_related {
                        theme.primary
                    } else {
                        theme.foreground
                    })
                    .flex()
                    .items_center()
                    .gap(crate::ui::compact::rem(4.))
                    .h_full()
                    .child(
                        div()
                            .min_w_0()
                            .text_ellipsis()
                            .child(parameter.name.clone()),
                    )
                    .when(self.visual.repeat, |name| {
                        name.child(name_badge(
                            format!("parameter-repeat-{parameter_id}"),
                            t!("parameter.repeat").into_owned(),
                            Icon::new(IconName::Repeat).size(crate::ui::compact::rem(12.)),
                            theme.muted_foreground,
                        ))
                    })
                    .when(self.indirectly_related, |name| {
                        name.child(
                            crate::ui::compact::header_button(
                                Button::new(SharedString::from(format!(
                                    "parameter-indirect-{parameter_id}"
                                )))
                                .icon(IconName::Link)
                                .accessibility_label(t!("parameter.select_nearest_influence"))
                                .tooltip(format!(
                                    "{}\n{}",
                                    t!("parameter.indirect_selection"),
                                    t!("parameter.select_nearest_influence"),
                                ))
                                .disabled(disabled)
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        cx.stop_propagation();
                                        if let Some(document) = this.document_id {
                                            let _ = this.panel.update(cx, |panel, cx| {
                                                panel.select_nearest_direct_influence(
                                                    document,
                                                    &this.parameter.id,
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }
                                    },
                                )),
                            )
                            .ghost()
                            .text_color(theme.primary),
                        )
                    }),
            )
            .child(row::track().child(parameter_track(
                slider,
                parameter,
                &self.visual,
                value,
                disabled,
                theme,
                self.panel.clone(),
                document_id,
            )))
            .child(
                row::value()
                    .id(SharedString::from(format!(
                        "parameter-value-{parameter_id}"
                    )))
                    .test_support()
                    .aria_label(parameter.name.clone())
                    .child(self.number_input.clone()),
            )
            .map(|element| {
                management::row_menu(
                    element,
                    self.panel.clone(),
                    document_id,
                    vec![
                        ParameterAction::CreateAt {
                            kind: zipzi_editor_core::ObjectKind::Parameter,
                            selection: vec![zipzi_editor_core::ObjectRef::new(
                                zipzi_editor_core::ObjectKind::Parameter,
                                &parameter_id,
                            )],
                        },
                        ParameterAction::CreateAt {
                            kind: zipzi_editor_core::ObjectKind::ParameterGroup,
                            selection: vec![zipzi_editor_core::ObjectRef::new(
                                zipzi_editor_core::ObjectKind::Parameter,
                                &parameter_id,
                            )],
                        },
                        ParameterAction::Edit(parameter_id.clone()),
                        ParameterAction::Delete(parameter_id),
                    ],
                )
            })
    }
}

fn name_badge(
    id: String,
    label: impl Into<SharedString>,
    content: impl IntoElement,
    color: Hsla,
) -> impl IntoElement {
    let label = label.into();
    div()
        .id(SharedString::from(id))
        .test_support()
        .aria_label(label.clone())
        .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
        .flex()
        .items_center()
        .flex_shrink_0()
        .text_color(color)
        .child(content)
}

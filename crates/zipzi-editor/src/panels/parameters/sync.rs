//! Reconcile document changes without rebuilding unchanged row entities.
//! Structure changes replace controls and subscriptions; value changes retain
//! them, including focused drafts. Keep notification gates at each update site.
use super::*;

impl ParametersPanel {
    pub(super) fn sync_with_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let (document_id, parameters, revision, selection) = {
            let owner = owner.read(cx);
            if owner.editor.is_busy() && self.document_id == owner.editor.active_id() {
                return;
            }
            let doc = owner.editor.document();
            (
                owner.editor.active_id(),
                doc.map_or_else(Vec::new, |doc| doc.parameters()),
                doc.map(|doc| doc.parameter_model_revision()),
                doc.map(|doc| {
                    format!(
                        "{:?}/{:?}",
                        doc.context().selections,
                        doc.context().active_binding
                    )
                }),
            )
        };
        let by_id: HashMap<_, _> = self
            .rows
            .iter()
            .map(|row| (row.parameter.id.as_str(), &row.parameter))
            .collect();
        let structure_changed = self.document_id != document_id
            || self.rows.len() != parameters.len()
            || parameters.iter().any(|parameter| {
                by_id.get(parameter.id.as_str()).is_none_or(|old| {
                    old.minimum != parameter.minimum
                        || old.maximum != parameter.maximum
                        || old.decimal_places != parameter.decimal_places
                })
            });
        let order_changed = self
            .rows
            .iter()
            .zip(&parameters)
            .any(|(row, parameter)| row.parameter.id != parameter.id);
        let layout_changed = structure_changed
            || order_changed
            || self.visual_revision != revision
            || self
                .rows
                .iter()
                .zip(&parameters)
                .any(|(row, parameter)| row.parameter.group_id != parameter.group_id);
        if layout_changed {
            if self.document_id != document_id {
                self.collapsed_groups.clear();
            }
            let groups = owner
                .read(cx)
                .editor
                .document()
                .map_or_else(Vec::new, |doc| doc.parameter_groups());
            self.collapsed_groups.retain(|id| {
                id.as_ref()
                    .is_none_or(|id| groups.iter().any(|group| &group.id == id))
            });
            self.layout = ParameterLayout::new(&groups, &parameters, &t!("parameter.ungrouped"));
        }
        let visuals_changed = structure_changed
            || self.visual_revision != revision
            || self.visual_selection != selection;
        let visuals = visuals_changed.then(|| {
            owner
                .read(cx)
                .editor
                .document()
                .map_or_else(HashMap::new, |doc| doc.parameter_visuals())
        });
        self.visual_revision = revision;
        self.visual_selection = selection;

        if structure_changed {
            self.rebuild_rows(document_id, parameters, visuals.as_ref(), window, cx);
        } else {
            self.update_rows(parameters, visuals.as_ref(), order_changed, window, cx);
        }
        if layout_changed {
            self.refresh_visible_rows(cx);
        }
        if visuals_changed {
            self.refresh_binding_hints(&owner, cx);
        }
        self.sync_navigation(cx);
    }

    fn rebuild_rows(
        &mut self,
        document_id: Option<DocumentId>,
        parameters: Vec<ParameterValue>,
        visuals: Option<&HashMap<String, ParameterVisual>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_slider_drag(cx);
        self.document_id = document_id;
        self.scroll = UniformListScrollHandle::new();
        self.subscriptions.clear();
        let panel = cx.weak_entity();
        self.rows = parameters
            .into_iter()
            .map(|parameter| {
                let visual = visuals
                    .and_then(|visuals| visuals.get(&parameter.id))
                    .cloned()
                    .unwrap_or_else(|| ParameterVisual {
                        kind: Default::default(),
                        repeat: false,
                        keys: Vec::new(),
                    });
                let slider = cx.new(|_| {
                    let span = parameter.maximum - parameter.minimum;
                    SliderState::new()
                        .min(parameter.minimum)
                        .max(parameter.maximum)
                        .step(
                            10_f32
                                .powi(-(parameter.decimal_places as i32))
                                .min(span)
                                .max(span / 1000.)
                                .max(f32::EPSILON),
                        )
                        .default_value(parameter.value)
                });
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .step(0.1)
                        .default_value(format_value(parameter.value))
                });
                let number_input = cx.new(|cx| {
                    SlideNumberInput::new(
                        input.clone(),
                        parameter.name.clone(),
                        slide_number_options(&parameter, false),
                        window,
                        cx,
                    )
                    .format_sliding_value(format_one_decimal)
                });
                let view = cx.new(|cx| ParameterRowView {
                    panel: panel.clone(),
                    document_id,
                    parameter: parameter.clone(),
                    visual: visual.clone(),
                    slider: slider.clone(),
                    input: input.clone(),
                    number_input: number_input.clone(),
                    disabled: false,
                    summary: String::new(),
                    related: false,
                    selected: false,
                    cursor: false,
                    depth: 0,
                    _observe: cx.observe(&slider, |_, _, cx| cx.notify()),
                    sorting: self.sorting.clone(),
                    _sorting_observe: cx.observe(&self.sorting, |_, _, cx| cx.notify()),
                });
                ParameterRow {
                    view,
                    parameter,
                    visual,
                    slider,
                    input,
                    number_input,
                    input_dirty: false,
                }
            })
            .collect();
        for row in &self.rows {
            let parameter_id = row.parameter.id.clone();
            self.subscriptions.push(cx.subscribe_in(
                &row.number_input,
                window,
                move |this: &mut Self, source, event: &SlideNumberChange, window, cx| {
                    let Some(document_id) = document_id else {
                        return;
                    };
                    if this
                        .rows
                        .iter()
                        .any(|row| row.parameter.id == parameter_id && &row.number_input == source)
                    {
                        this.apply_values(
                            document_id,
                            &[(parameter_id.clone(), event.0 as f32)],
                            window,
                            cx,
                        );
                    }
                },
            ));
            let parameter_id = row.parameter.id.clone();
            let slider = row.slider.clone();
            let subscription = cx.subscribe_in(
                &slider,
                window,
                move |this: &mut Self, source, event: &SliderEvent, window, cx| {
                    let Some(document_id) = document_id else {
                        return;
                    };
                    if this.document_id != Some(document_id)
                        || !this
                            .rows
                            .iter()
                            .any(|row| row.parameter.id == parameter_id && &row.slider == source)
                    {
                        return;
                    }
                    let SliderValue::Single(value) = (match event {
                        SliderEvent::Change(value) | SliderEvent::Release(value) => value,
                    }) else {
                        return;
                    };
                    let Some(row) = this.rows.iter().find(|r| r.parameter.id == parameter_id)
                    else {
                        return;
                    };
                    // Kit rounds around zero and can overshoot a non-aligned range endpoint.
                    let value = value.clamp(row.parameter.minimum, row.parameter.maximum);
                    this.apply_values(document_id, &[(parameter_id.clone(), value)], window, cx);
                },
            );
            self.subscriptions.push(subscription);
            let input = row.input.clone();
            let parameter_id = row.parameter.id.clone();
            let input_subscription = cx.subscribe_in(
                &input,
                window,
                move |this: &mut Self, source, event: &InputEvent, window, cx| {
                    let Some(document_id) = document_id else {
                        return;
                    };
                    if this.document_id != Some(document_id) {
                        return;
                    }
                    let Some(row) = this
                        .rows
                        .iter_mut()
                        .find(|row| row.parameter.id == parameter_id && &row.input == source)
                    else {
                        return;
                    };
                    match event {
                        InputEvent::Change => row.input_dirty = true,
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            this.commit_input(document_id, &parameter_id, window, cx)
                        }
                        InputEvent::Focus => {}
                    }
                },
            );
            self.subscriptions.push(input_subscription);
        }
    }

    fn update_rows(
        &mut self,
        parameters: Vec<ParameterValue>,
        visuals: Option<&HashMap<String, ParameterVisual>>,
        order_changed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if order_changed {
            let ranks: HashMap<_, _> = parameters
                .iter()
                .enumerate()
                .map(|(index, parameter)| (parameter.id.as_str(), index))
                .collect();
            self.rows
                .sort_by_key(|row| ranks[row.parameter.id.as_str()]);
        }
        for (row, parameter) in self.rows.iter_mut().zip(parameters) {
            {
                let current = match row.slider.read(cx).value() {
                    SliderValue::Single(value) => value,
                    _ => parameter.default_value,
                };
                if current != parameter.value {
                    row.slider.update(cx, |slider, cx| {
                        slider.set_value(parameter.value, window, cx);
                    });
                }
            }
            if !row.input_dirty && !row.input.read(cx).focus_handle(cx).is_focused(window) {
                let desired = format_value(parameter.value);
                if row.input.read(cx).value().as_ref() != desired {
                    row.input.update(cx, |input, cx| {
                        input.set_value(desired, window, cx);
                    });
                }
            }
            if let Some(visual) = visuals.and_then(|v| v.get(&parameter.id)) {
                if row.visual != *visual {
                    row.visual = visual.clone();
                    row.view.update(cx, |view, cx| {
                        view.visual = visual.clone();
                        cx.notify();
                    });
                }
            }
            row.view.update(cx, |view, cx| {
                if view.parameter != parameter {
                    view.parameter = parameter.clone();
                    cx.notify();
                }
            });
            row.parameter = parameter;
        }
    }

    fn refresh_binding_hints(&self, owner: &Entity<EditorView>, cx: &mut Context<Self>) {
        if let Some(doc) = owner.read(cx).editor.document() {
            let index = doc.object_index();
            let selection = doc.context().selections.clone();
            for row in &self.rows {
                let bindings = index.parameter(&row.parameter.id);
                let summary = format!(
                    "· {} {} · {} {} · B{}",
                    t!("editor.direct_bindings"),
                    bindings.direct_count(&index),
                    t!("editor.indirect_influences"),
                    bindings.indirect_count(),
                    bindings.blend.len()
                );
                let related = selection.iter().any(|r| {
                    bindings
                        .regular
                        .iter()
                        .chain(&bindings.blend)
                        .any(|reference| index.binding(reference).is_some_and(|b| &b.target == r))
                        || bindings.indirect.iter().any(|path| &path.target == r)
                });
                row.view.update(cx, |v, cx| {
                    v.related = related;
                    v.summary = summary;
                    cx.notify();
                });
            }
        }
    }
}

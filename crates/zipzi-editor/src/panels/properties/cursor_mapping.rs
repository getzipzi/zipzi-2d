//! Mouse mapping table backed by the standard property form.
use super::*;
use crate::properties::Choice;
use crate::properties::cursor_mapping::field_keys;
use gpui_kit::assets::IconName;
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectState};
use gpui_kit::component::tooltip::Tooltip;
use zipzi_editor_core::{CursorPreviewAxis, CursorPreviewBinding, CursorPreviewBindings};

const WIDTHS: [Option<f32>; 5] = [None, Some(80.), Some(60.), Some(26.), Some(26.)];
const SCROLLBAR_GUTTER: f32 = 12.;

pub(super) struct CursorMappingPicker {
    document: DocumentId,
    add: Entity<SelectState<SearchableVec<Choice>>>,
    choices: Vec<Choice>,
    _subscription: Subscription,
}

impl PropertiesPanel {
    pub(crate) fn reveal_cursor_mapping(&mut self, cx: &mut Context<Self>) {
        self.locked_target = None;
        cx.notify();
    }

    pub(super) fn render_cursor_mapping(
        &mut self,
        parameters: Vec<zipzi_editor_core::ParameterValue>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let actions = self.cursor_mapping_actions(window, cx);
        let labels = [
            t!("physics.parameter"),
            t!("physics.cursor_axis"),
            "%".into(),
            "±".into(),
            "".into(),
        ];
        let help = [
            t!("physics.parameter"),
            t!("physics.cursor_axis"),
            t!("physics.cursor_influence"),
            t!("physics.reflect"),
            t!("physics.cursor_remove"),
        ];
        let mut header = table::header(cx);
        for (index, (width, (label, help))) in WIDTHS
            .into_iter()
            .zip(labels.into_iter().zip(help))
            .enumerate()
        {
            let help: SharedString = help.into();
            header = header.child(table::cell(
                width,
                div()
                    .id(SharedString::from(format!("cursor-mapping-column-{index}")))
                    .test_support()
                    .role(gpui::Role::ColumnHeader)
                    .aria_label(help.clone())
                    .truncate()
                    .child(label)
                    .tooltip(move |window, cx| Tooltip::new(help.clone()).build(window, cx)),
                cx,
            ));
        }
        let width: f32 = WIDTHS
            .into_iter()
            .map(|width| width.unwrap_or(table::FLEX_MIN_WIDTH))
            .sum();
        let mut rows = div().flex().flex_col().w_full();
        for parameter in parameters {
            let fields = field_keys(&parameter.id);
            if self.form.fields.contains_key(&fields[0]) {
                rows = rows.child(self.cursor_mapping_row(parameter, fields, window, cx));
            }
        }
        div()
            .id("object-properties")
            .test_support()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .p(compact::rem(compact::PADDING))
            .gap(compact::rem(compact::PADDING))
            .child(actions)
            .child(
                div()
                    .id("cursor-mapping-table")
                    .test_support()
                    .role(gpui::Role::Table)
                    .aria_label(t!("physics.cursor_mapping"))
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .size_full()
                            .child(
                                div()
                                    .w_full()
                                    .h_full()
                                    .min_w(compact::rem(width + SCROLLBAR_GUTTER))
                                    .flex()
                                    .flex_col()
                                    .child(header.pr(compact::rem(SCROLLBAR_GUTTER)))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_h_0()
                                            .pr(compact::rem(SCROLLBAR_GUTTER))
                                            .child(rows)
                                            .overflow_y_scrollbar(),
                                    ),
                            )
                            .overflow_x_scrollbar(),
                    ),
            )
            .into_any_element()
    }

    fn cursor_mapping_actions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return Empty.into_any_element();
        };
        let editor = &owner.read(cx).editor;
        let (Some(doc), Some(document)) = (editor.document(), editor.active_id()) else {
            return Empty.into_any_element();
        };
        let bindings = doc.cursor_preview_bindings();
        let customized = doc.has_custom_cursor_preview_bindings();
        let choices: Vec<_> = doc
            .parameters()
            .into_iter()
            .filter(|parameter| {
                bindings
                    .get(&parameter.id)
                    .is_none_or(|binding| binding.axis == CursorPreviewAxis::Disabled)
            })
            .map(|parameter| Choice {
                value: parameter.id,
                label: parameter.name.into(),
            })
            .collect();
        let disabled = !self.allowed(self.epoch, window, cx);
        if self
            .cursor_mapping
            .as_ref()
            .is_none_or(|picker| picker.document != document)
        {
            let add = cx.new(|cx| {
                SelectState::new(SearchableVec::new(choices.clone()), None, window, cx)
                    .searchable(true)
            });
            let subscription =
                cx.subscribe_in(&add, window, move |this, source, event, window, cx| {
                    let SelectEvent::Confirm(Some(id)) = event else {
                        return;
                    };
                    if this
                        .cursor_mapping
                        .as_ref()
                        .is_none_or(|picker| picker.add != *source)
                    {
                        return;
                    }
                    this.change_cursor_mapping(document, window, cx, |mut bindings| {
                        bindings
                            .entry(id.clone())
                            .and_modify(|binding| binding.axis = CursorPreviewAxis::X)
                            .or_insert(CursorPreviewBinding {
                                axis: CursorPreviewAxis::X,
                                influence_percent: 100.,
                                reversed: false,
                            });
                        Some(bindings)
                    });
                    source.update(cx, |state, cx| state.set_selected_index(None, window, cx));
                });
            self.cursor_mapping = Some(CursorMappingPicker {
                document,
                add,
                choices: choices.clone(),
                _subscription: subscription,
            });
        }
        let picker = self.cursor_mapping.as_mut().unwrap();
        if picker.choices != choices {
            picker.add.update(cx, |state, cx| {
                state.set_items(SearchableVec::new(choices.clone()), window, cx);
            });
            picker.choices = choices;
        }

        div()
            .id("cursor-mapping-actions")
            .test_support()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(compact::rem(compact::GAP))
            .child(compact::property_row(
                t!("physics.cursor_add"),
                compact::select(Select::new(&picker.add))
                    .id("cursor-mapping-add")
                    .w_full()
                    .accessibility_label(t!("physics.cursor_add"))
                    .disabled(disabled || picker.choices.is_empty()),
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(compact::rem(compact::GAP))
                    .child(
                        compact::button(Button::new("cursor-mapping-defaults"))
                            .label(t!("physics.cursor_defaults"))
                            .disabled(disabled || !customized)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.change_cursor_mapping(document, window, cx, |_| None);
                            })),
                    )
                    .child(
                        compact::button(Button::new("cursor-mapping-clear"))
                            .label(t!("physics.cursor_clear"))
                            .disabled(disabled || bindings.is_empty())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.change_cursor_mapping(document, window, cx, |_| {
                                    Some(CursorPreviewBindings::new())
                                });
                            })),
                    ),
            )
            .into_any_element()
    }

    fn cursor_mapping_row(
        &mut self,
        parameter: zipzi_editor_core::ParameterValue,
        [axis, influence, reversed]: [PropertyKey; 3],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let document = self.target.as_ref().unwrap().document;
        let label: SharedString = parameter.name.into();
        let parameter = parameter.id;
        let row = table::row(format!("cursor-mapping-row-{parameter}"), cx)
            .test_support()
            .aria_label(label.clone());
        let name = div()
            .id("cursor-mapping-parameter")
            .truncate()
            .child(label.clone())
            .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx));
        let remove = compact::button(Button::new(format!("cursor-remove-{parameter}")))
            .ghost()
            .icon(IconName::X)
            .tooltip(t!("physics.cursor_remove"))
            .accessibility_label(t!("physics.cursor_remove"))
            .disabled(self.disabled)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.change_cursor_mapping(document, window, cx, |mut bindings| {
                    bindings.remove(&parameter);
                    Some(bindings)
                });
            }));
        let reversed = self.render_field_control(reversed, window, cx);
        let cells = [
            name.into_any_element(),
            self.render_field_control(axis, window, cx),
            self.render_field_control(influence, window, cx),
            reversed,
            div()
                .flex()
                .justify_center()
                .child(remove)
                .into_any_element(),
        ];
        row.children(
            WIDTHS
                .into_iter()
                .zip(cells)
                .map(|(width, content)| table::cell(width, content, cx)),
        )
        .into_any_element()
    }

    fn change_cursor_mapping(
        &mut self,
        document: DocumentId,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(CursorPreviewBindings) -> Option<CursorPreviewBindings>,
    ) {
        if !self.allowed(self.epoch, window, cx)
            || !self
                .target
                .as_ref()
                .is_some_and(|target| target.document == document && target.is_cursor_mapping())
        {
            return;
        }
        let _ = self.owner.update(cx, |view, cx| {
            let result =
                properties::cursor_mapping::edit_bindings(&mut view.editor, document, |bindings| {
                    Ok(change(bindings))
                });
            view.complete_edit(result, cx);
        });
    }
}

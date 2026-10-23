//! Color fields and mesh-reference lists share the ordinary form lifecycle.
use super::*;
use crate::ui::color_picker::{ColorField, ColorPickerEvent, ColorPickerState};
use editor_core::PropertyValue;

pub(super) struct ColorControl {
    pub state: Entity<ColorPickerState>,
    _event: Subscription,
}

impl PropertiesPanel {
    pub(super) fn render_color(
        &mut self,
        id: PropertyKey,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = match self.form.fields[&id].parsed {
            Ok(PropertyValue::Color(color)) => color,
            _ => match self.form.fields[&id].baseline {
                PropertyValue::Color(color) => color,
                _ => [0.; 3],
            },
        };
        let rgba = Rgba {
            r: color[0],
            g: color[1],
            b: color[2],
            a: 1.,
        };
        if !self.colors.contains_key(&id) {
            let state = cx.new(|cx| ColorPickerState::new(rgba, false, window, cx));
            let epoch = self.epoch;
            let event = cx.subscribe_in(&state, window, move |this, source, event, window, cx| {
                if !this.allowed(epoch, window, cx)
                    || this
                        .colors
                        .get(&id)
                        .is_none_or(|control| &control.state != source)
                {
                    return;
                }
                match event {
                    ColorPickerEvent::Begin => this.begin_preview(id, cx),
                    ColorPickerEvent::Preview(color) => this.update_preview_value(
                        id,
                        PropertyValue::Color([color.r, color.g, color.b]),
                        cx,
                    ),
                    ColorPickerEvent::Commit(_)
                        if this.preview.as_ref().is_some_and(|(field, _)| *field == id) =>
                    {
                        this.finish_preview(cx)
                    }
                    ColorPickerEvent::Cancel
                        if this.preview.as_ref().is_some_and(|(field, _)| *field == id) =>
                    {
                        this.cancel_preview(cx)
                    }
                    _ => {}
                }
            });
            self.colors.insert(
                id,
                ColorControl {
                    state,
                    _event: event,
                },
            );
        }
        let label = t!(self.field_spec(id).label).into_owned();
        let state = self.colors[&id].state.clone();
        state.update(cx, |state, cx| {
            state.sync(rgba, label, disabled, window, cx)
        });
        ColorField::new(format!("property-color-{}", id.name()), &state)
            .input_id(format!("property-input-{}", id.name()))
            .when(id == editor_core::property::mesh::MULTIPLY.key, |field| {
                field.reset_color(rgb(0xffffff))
            })
            .when(id == editor_core::property::mesh::SCREEN.key, |field| {
                field.reset_color(rgb(0x000000))
            })
            .into_any_element()
    }

    pub(super) fn render_references(
        &mut self,
        id: PropertyKey,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let select = self.render_select(id, disabled, window, cx);
        let ids = match &self.form.fields[&id].parsed {
            Ok(PropertyValue::References(ids)) => Arc::new(ids.clone()),
            _ => Arc::new(vec![]),
        };
        let index = self.owner.upgrade().and_then(|owner| {
            owner
                .read(cx)
                .editor
                .document()
                .map(|doc| doc.object_index())
        });
        let mut body = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(compact::rem(compact::GAP));
        let epoch = self.epoch;
        if !ids.is_empty() {
            let panel = cx.entity();
            let count = ids.len();
            let scroll = self.selects[&id].reference_scroll.clone();
            let list = uniform_list(
                SharedString::from(format!("property-references-{}", id.name())),
                count,
                move |range, _, cx| {
                    panel.update(cx, |_, cx| {
                        range
                            .map(|slot| {
                                let reference = ids[slot].clone();
                                let object =
                                    ObjectRef::new(editor_core::ObjectKind::Mesh, &reference);
                                let label = index
                                    .as_ref()
                                    .map_or(reference.as_str(), |index| index.name(&object))
                                    .to_owned();
                                div()
                                    .h(compact::rem(compact::ROW))
                                    .flex()
                                    .items_center()
                                    .w_full()
                                    .min_w_0()
                                    .gap(compact::rem(compact::GAP))
                                    .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                                    .child(
                                        compact::button(Button::new(SharedString::from(format!(
                                            "property-remove-{}-{slot}",
                                            id.name()
                                        ))))
                                        .ghost()
                                        .icon(gpui_kit::assets::IconName::X)
                                        .disabled(disabled)
                                        .accessibility_label(t!(
                                            "properties.remove_mask",
                                            name = label
                                        ))
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                if this.allowed(epoch, window, cx) {
                                                    if let Ok(PropertyValue::References(ids)) =
                                                        &this.form.fields[&id].parsed
                                                    {
                                                        let remaining = ids
                                                            .iter()
                                                            .filter(|value| **value != reference)
                                                            .cloned()
                                                            .collect();
                                                        this.set_value(
                                                            id,
                                                            PropertyValue::References(remaining),
                                                            cx,
                                                        );
                                                    }
                                                }
                                            }),
                                        ),
                                    )
                                    .into_any_element()
                            })
                            .collect()
                    })
                },
            )
            .track_scroll(&scroll)
            .h(compact::rem(compact::ROW * count.min(4) as f32))
            .w_full();
            body = body.child(
                div()
                    .id(SharedString::from(format!(
                        "property-references-{}",
                        id.name()
                    )))
                    .test_support()
                    .relative()
                    .when(count > 4, |body| body.pr(compact::rem(14.)))
                    .child(list)
                    .vertical_scrollbar(&scroll),
            );
        }
        body.child(select).into_any_element()
    }
}

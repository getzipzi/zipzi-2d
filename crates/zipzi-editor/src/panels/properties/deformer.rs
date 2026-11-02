//! Warp density is a definition edit with an explicit, cancellable preview.
use super::*;
use crate::properties;
use zipzi_editor_core::property::deformer as fields;

impl PropertiesPanel {
    /// Append type-specific tools after the shared scene-object form.
    pub(super) fn deformer_rows(&self, doc: &zipzi_editor_core::Document) -> Vec<Row> {
        let target = self.target.as_ref().unwrap();
        let warp = self.form.fields.contains_key(&fields::COLUMNS.key);
        if properties::deformer::requires_single_selection(doc, &target.object) {
            return vec![Row::Detail(properties::DetailRow::Hint(
                t!("properties.deformer_single_selection").into(),
            ))];
        }
        let mut rows = vec![Row::Detail(properties::DetailRow::Hint(
            t!(properties::deformer::shape_label(&target.scope)).into(),
        ))];
        let section_fields = |section| {
            self.definition
                .fields
                .iter()
                .filter(|spec| spec.section == section && spec.binding.key != fields::NAME.key)
                .map(|spec| Row::Field(spec.binding.key))
                .collect::<Vec<_>>()
        };
        let mut group = |key: &str, label: &str, contents: Vec<Row>| {
            let collapsed = self.collapsed.contains(key);
            rows.push(Row::Group {
                key: key.into(),
                label: t!(label).into_owned(),
                collapsed,
            });
            if !collapsed {
                rows.extend(contents);
            }
        };
        if warp {
            group(
                "deformer-warp",
                "properties.deformer_warp",
                vec![Row::WarpGrid, Row::WarpBezierGrid],
            );
            let mut bezier = section_fields(Some("properties.deformer_warp_authoring"));
            bezier.push(Row::WarpShapeActions);
            group(
                "deformer-bezier",
                "properties.deformer_warp_authoring",
                bezier,
            );
        } else {
            group(
                "deformer-rotation",
                "properties.deformer_rotation",
                section_fields(Some("properties.deformer_rotation")),
            );
        }
        group(
            "deformer-original",
            "properties.deformer_original",
            std::iter::once(Row::DeformerOriginal)
                .chain(section_fields(Some("properties.deformer_original")))
                .collect(),
        );
        rows
    }

    pub(super) fn warp_shape_actions(&self, cx: &Context<Self>) -> AnyElement {
        let target = self.target.clone().unwrap();
        let epoch = self.epoch;
        let panel = cx.weak_entity();
        let context = self
            .owner
            .upgrade()
            .and_then(|o| crate::authoring::AuthoringContext::capture(&o.read(cx).editor));
        let mut body = div()
            .id("property-warp-shape")
            .test_support()
            .flex()
            .gap_2();
        for (id, label, hint, action) in [
            (
                "property-warp-reset",
                "properties.warp_reset_controller",
                "properties.warp_reset_controller_hint",
                zipzi_core::WarpShapeAction::ResetController,
            ),
            (
                "property-warp-reform",
                "properties.warp_reform",
                "properties.warp_reform_hint",
                zipzi_core::WarpShapeAction::Reform,
            ),
        ] {
            let owner = self.owner.clone();
            let target = target.clone();
            let panel = panel.clone();
            body = body.child(
                compact::button(Button::new(id))
                    .flex_1()
                    .min_w_0()
                    .label(t!(label).into_owned())
                    .tooltip(t!(hint))
                    .disabled(self.disabled || !target.scope.shape_editable())
                    .on_click(move |_, window, cx| {
                        if window.has_active_dialog(cx)
                            || !panel.upgrade().is_some_and(|p| {
                                let p = p.read(cx);
                                p.visible && p.epoch == epoch && p.target.as_ref() == Some(&target)
                            })
                        {
                            return;
                        }
                        let _ = owner.update(cx, |view, cx| {
                            if view.modal
                                || crate::authoring::AuthoringContext::capture(&view.editor)
                                    != context
                            {
                                return;
                            }
                            if !view.prepare_property_inputs(target.document, window, cx) {
                                return;
                            }
                            let result = (|| {
                                let binding = view
                                    .property_modeling_viewport()
                                    .ok_or_else(|| anyhow::anyhow!("Warp viewport closed"))?;
                                let target = view
                                    .editor
                                    .warp_transform_target_for(binding, &target.object)
                                    .ok_or_else(|| anyhow::anyhow!("Warp changed"))?;
                                view.editor
                                    .execute(zipzi_editor_core::EditorCommand::WarpShape {
                                        target,
                                        action,
                                    })
                            })();
                            view.complete_edit(result, cx);
                        });
                    }),
            );
        }
        body.into_any_element()
    }
    pub(super) fn warp_density_fields(
        &mut self,
        bezier: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dimensions = if bezier {
            [fields::BEZIER_COLUMNS, fields::BEZIER_ROWS]
        } else {
            [fields::COLUMNS, fields::ROWS]
        };
        let label = t!(if bezier {
            "properties.warp_bezier_density"
        } else {
            "properties.warp_density"
        })
        .into_owned();
        let x = self.render_field_control(dimensions[0].key, window, cx);
        let y = self.render_field_control(dimensions[1].key, window, cx);
        compact::property_row(
            label,
            div()
                .flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_1()
                .child(div().flex_1().min_w_0().child(x))
                .child("×")
                .child(div().flex_1().min_w_0().child(y)),
            cx,
        )
        .into_any_element()
    }
}

impl EditorView {
    pub(crate) fn property_modeling_viewport(&self) -> Option<zipzi_editor_core::ViewportBinding> {
        self.focused_viewport
            .and_then(|v| self.editor.viewport_binding(v))
            .filter(|b| b.kind == zipzi_editor_core::ViewportKind::Modeling)
            .or_else(|| self.editor.primary_viewport_binding())
    }
}

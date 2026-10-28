//! Mesh-generation settings and preview layout.
use super::*;

impl Render for MeshGenerationForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let side = preview_side(window, cx);
        let mut settings = div()
            .id("mesh-generation-settings")
            .test_support()
            .flex()
            .flex_col()
            .gap_3()
            .w(crate::ui::compact::rem(240.))
            .flex_shrink_0()
            .max_h(crate::ui::compact::rem(side + 36.))
            .overflow_y_scroll()
            .child(
                div()
                    .text_size(crate::ui::compact::rem(crate::ui::compact::BODY))
                    .font_weight(FontWeight::MEDIUM)
                    .truncate()
                    .child(self.name.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_xs()
                    .child(t!("mesh_generation.algorithm"))
                    .child(
                        div().flex_shrink_0().child(
                            crate::ui::compact::select(Select::new(&self.algorithm))
                                .id("mesh-generation-algorithm")
                                .accessibility_label(t!("mesh_generation.algorithm"))
                                .w_full(),
                        ),
                    ),
            );
        let selected = self.algorithm.read(cx).selected_value().copied();
        let replaces = matches!(selected, Some("manual-edge" | "manual-full"))
            || (selected == Some("alpha-cdt") && self.preview.source.replaces_modeling_topology());
        if replaces {
            settings = settings.child(
                div()
                    .id("mesh-generation-replacement-notice")
                    .test_support()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("mesh_generation.replacement_notice")),
            );
        }
        if self.is_alpha(cx) {
            let mut presets = div().flex().gap_1();
            for (i, (id, label)) in [
                ("mesh-generation-alpha-light", t!("mesh_generation.light")),
                (
                    "mesh-generation-alpha-standard",
                    t!("mesh_generation.standard"),
                ),
                ("mesh-generation-alpha-heavy", t!("mesh_generation.heavy")),
            ]
            .into_iter()
            .enumerate()
            {
                let preset = MeshSettings {
                    clip_to_image: self.preview.source.manual_available(),
                    ..alpha_presets()[i]
                };
                let selected = self.validation.as_ref().ok() == Some(&self.wrap_alpha(preset, cx));
                presets =
                    presets.child(
                        crate::ui::compact::button(Button::new(id))
                            .label(label)
                            .when(selected, |b| b.primary())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.alpha_preset(i, window, cx)
                            })),
                    );
            }
            settings = settings.child(presets).child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("mesh_generation.pixel_units")),
            );
            for (id, label, select) in [
                (
                    "mesh-generation-edge-mode",
                    t!("mesh_generation.edge_mode"),
                    self.edge_mode.clone(),
                ),
                (
                    "mesh-generation-fill-algorithm",
                    t!("mesh_generation.fill_algorithm"),
                    self.fill_algorithm.clone(),
                ),
            ] {
                settings = settings.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .text_xs()
                        .child(label)
                        .child(
                            crate::ui::compact::select(Select::new(&select))
                                .id(id)
                                .w_full(),
                        ),
                );
            }
            for (i, (id, label)) in [
                (
                    "mesh-generation-edge-spacing",
                    t!("mesh_generation.edge_spacing"),
                ),
                (
                    "mesh-generation-interior-spacing",
                    t!("mesh_generation.interior_spacing"),
                ),
                (
                    "mesh-generation-outer-margin",
                    t!("mesh_generation.outer_margin"),
                ),
                (
                    "mesh-generation-edge-width",
                    t!("mesh_generation.edge_width"),
                ),
                ("mesh-generation-wrap", t!("mesh_generation.wrap")),
                (
                    "mesh-generation-alpha-threshold",
                    t!("mesh_generation.alpha_threshold"),
                ),
                (
                    "mesh-generation-edge-ratio",
                    t!("mesh_generation.edge_ratio"),
                ),
                ("mesh-generation-gradation", t!("mesh_generation.gradation")),
                ("mesh-generation-jitter", t!("mesh_generation.jitter")),
                ("mesh-generation-angle", t!("mesh_generation.fill_angle")),
                ("mesh-generation-max-rows", t!("mesh_generation.max_rows")),
            ]
            .into_iter()
            .enumerate()
            {
                let edge = self.edge_kind(cx);
                let fill = self.fill_kind(cx);
                if (i == 2 && edge != MeshEdgeMode::Single)
                    || (i == 3 && edge == MeshEdgeMode::Single)
                    || (i >= 6 && fill == MeshFillAlgorithm::SimpleTriangles)
                    || (i == 8 && fill != MeshFillAlgorithm::GradedPoisson)
                    || (i == 9
                        && !matches!(
                            fill,
                            MeshFillAlgorithm::AdaptiveQuadtree
                                | MeshFillAlgorithm::TriangleFractal
                        ))
                    || (i == 10 && fill != MeshFillAlgorithm::ContourPaving)
                {
                    continue;
                }
                let options =
                    self.alpha_fields[i].options(&self.alpha_inputs[i], self.dialog.closed(), cx);
                self.alpha_numbers[i]
                    .update(cx, |input, cx| input.configure(label.clone(), options, cx));
                settings = settings.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .flex_shrink_0()
                        .child(div().flex_1().text_xs().child(label.clone()))
                        .child(
                            div()
                                .w(crate::ui::compact::rem(76.))
                                .id(id)
                                .test_support()
                                .aria_label(label)
                                .child(self.alpha_numbers[i].clone()),
                        ),
                );
            }
            settings = settings.child(
                Checkbox::new("mesh-generation-suppress-diagonals")
                    .small()
                    .label(t!("mesh_generation.suppress_diagonals").to_string())
                    .checked(self.suppress_boundary_diagonals)
                    .on_click(cx.listener(|this, value: &bool, _, cx| {
                        this.suppress_boundary_diagonals = *value;
                        this.dialog.edited();
                        this.refresh(cx);
                        cx.notify();
                    })),
            );
        } else if !self.is_reconnect(cx) {
            let mut presets = div().flex().gap_1();
            for (id, label, count) in [
                ("mesh-generation-sparse", t!("mesh_generation.sparse"), 4),
                ("mesh-generation-medium", t!("mesh_generation.medium"), 8),
                ("mesh-generation-dense", t!("mesh_generation.dense"), 16),
            ] {
                let selected = self.validation.as_ref().ok()
                    == Some(&MeshGenerationOptions::Grid(MeshGridOptions {
                        columns: count,
                        rows: count,
                    }));
                presets = presets.child(
                    crate::ui::compact::button(Button::new(id))
                        .label(label)
                        .when(selected, |b| b.primary())
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.preset(count, window, cx)),
                        ),
                );
            }
            settings = settings.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().child(t!("mesh_generation.presets")))
                    .child(presets),
            );
            let mut counts = div().flex().gap_2();
            for (i, label) in [t!("mesh_generation.columns"), t!("mesh_generation.rows")]
                .into_iter()
                .enumerate()
            {
                let options = GRID_FIELD.options(&self.inputs[i], self.dialog.closed(), cx);
                self.numbers[i].update(cx, |input, cx| input.configure(label.clone(), options, cx));
                counts = counts.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .child(label.clone())
                        .child(
                            div()
                                .id(if i == 0 {
                                    "mesh-generation-columns"
                                } else {
                                    "mesh-generation-rows"
                                })
                                .test_support()
                                .aria_label(label)
                                .child(self.numbers[i].clone()),
                        ),
                );
            }
            settings = settings.child(counts).when(self.square, |v| {
                v.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("mesh_generation.square_required")),
                )
            });
            if self.preview.source.alpha_error().is_some() {
                settings = settings.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("mesh_generation.alpha_requirements")),
                );
            }
        }
        if let Some(plan) = self.preview.plan() {
            let geometry = plan.geometry();
            settings = settings.child(
                div()
                    .id("mesh-generation-counts")
                    .test_support()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!(
                        "mesh_generation.counts",
                        vertices = geometry.positions.len(),
                        triangles = geometry.triangles.len()
                    )),
            );
        }
        settings = settings.when_some(
            self.dialog
                .error
                .clone()
                .or_else(|| self.validation.as_ref().err().map(ToString::to_string)),
            |v, error| {
                v.child(
                    div()
                        .id("mesh-generation-error")
                        .test_support()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            },
        );
        div()
            .id("mesh-generation-form")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .gap_4()
            .child(settings)
            .child(
                div()
                    .id("mesh-generation-preview")
                    .test_support()
                    .w(crate::ui::compact::rem(side))
                    .h(crate::ui::compact::rem(side + 36.))
                    .flex_shrink_0()
                    .child(self.viewport.clone()),
            )
    }
}

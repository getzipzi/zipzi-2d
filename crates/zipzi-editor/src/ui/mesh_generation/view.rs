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
                let preset = AlphaMeshOptions {
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
            for (i, (id, label)) in [
                (
                    "mesh-generation-outside-spacing",
                    t!("mesh_generation.outside_spacing"),
                ),
                (
                    "mesh-generation-inside-spacing",
                    t!("mesh_generation.inside_spacing"),
                ),
                (
                    "mesh-generation-outside-margin",
                    t!("mesh_generation.outside_margin"),
                ),
                (
                    "mesh-generation-inside-margin",
                    t!("mesh_generation.inside_margin"),
                ),
                (
                    "mesh-generation-minimum-margin",
                    t!("mesh_generation.minimum_margin"),
                ),
                (
                    "mesh-generation-boundary-points",
                    t!("mesh_generation.boundary_points"),
                ),
                (
                    "mesh-generation-alpha-threshold",
                    t!("mesh_generation.alpha_threshold"),
                ),
            ]
            .into_iter()
            .enumerate()
            {
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
                Checkbox::new("mesh-generation-remove-faint-islands")
                    .small()
                    .label(t!("mesh_generation.remove_faint_islands").to_string())
                    .checked(self.remove_faint_islands)
                    .on_click(cx.listener(|this, value: &bool, _, cx| {
                        this.remove_faint_islands = *value;
                        this.dialog.edited();
                        this.refresh(cx);
                        cx.notify();
                    })),
            );
            if !self.preview.source.manual_available() {
                settings = settings.child(
                    Checkbox::new("mesh-generation-preserve-holes")
                        .small()
                        .label(t!("mesh_generation.preserve_holes").to_string())
                        .checked(self.preserve_holes)
                        .on_click(cx.listener(|this, value: &bool, _, cx| {
                            this.preserve_holes = *value;
                            this.dialog.edited();
                            this.refresh(cx);
                            cx.notify();
                        })),
                );
            }
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

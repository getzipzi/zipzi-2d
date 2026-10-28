//! Viewport-owned, two-stage generation. Drafts never create history entries.
#[path = "mesh_generation_controls.rs"]
mod controls;
use super::slide_number_input::{SlideNumberChange, SlideNumberInput, SlideNumberOptions};
use crate::{EditorView, t, ui::compact};
use controls::Controls;
use gpui_kit::base::{Tab, Tabs};
use gpui_kit::component::{
    ActiveTheme, Disableable, IndexPath, Sizable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{InputEvent, InputState},
    select::{Select, SelectEvent, SelectItem, SelectState},
};
use gpui_kit::{prelude::*, *};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use zipzi_editor_core::adaptive_mesh::{
    FillParameters, MeshEdgeMode, MeshFillAlgorithm, MeshFillParameters,
};
use zipzi_editor_core::{
    DocumentId, EditingContextToken, MeshGenerationOptions, MeshGenerationPreview,
    MeshGenerationSource, MeshSettings, ViewportId,
};

#[derive(Clone)]
struct Choice {
    id: &'static str,
    label: SharedString,
}
impl SelectItem for Choice {
    type Value = &'static str;
    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &Self::Value {
        &self.id
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Edges,
    Interior,
}
const FIELDS: [(&str, &str, f64, f64); 11] = [
    (
        "mesh-generation-edge-spacing",
        "mesh_generation.edge_spacing",
        6.,
        16384.,
    ),
    (
        "mesh-generation-interior-spacing",
        "mesh_generation.interior_spacing",
        6.,
        16384.,
    ),
    (
        "mesh-generation-outer-margin",
        "mesh_generation.outer_margin",
        0.,
        256.,
    ),
    (
        "mesh-generation-edge-width",
        "mesh_generation.edge_width",
        0.,
        256.,
    ),
    ("mesh-generation-wrap", "mesh_generation.wrap", 0., 64.),
    (
        "mesh-generation-alpha-threshold",
        "mesh_generation.alpha_threshold",
        1.,
        255.,
    ),
    (
        "mesh-generation-edge-ratio",
        "mesh_generation.edge_ratio",
        1.,
        4.,
    ),
    (
        "mesh-generation-gradation",
        "mesh_generation.gradation",
        0.3,
        4.,
    ),
    ("mesh-generation-jitter", "mesh_generation.jitter", 0., 1.),
    (
        "mesh-generation-angle",
        "mesh_generation.fill_angle",
        0.,
        90.,
    ),
    (
        "mesh-generation-max-rows",
        "mesh_generation.max_rows",
        0.,
        24.,
    ),
];
pub(crate) struct MeshGenerationPanel {
    owner: WeakEntity<EditorView>,
    pub(crate) document: DocumentId,
    pub(crate) viewport: ViewportId,
    context: EditingContextToken,
    source: Arc<MeshGenerationSource>,
    contour: Option<zipzi_sdk::MeshGeometry>,
    stage: Stage,
    columns: u16,
    edge: Entity<SelectState<Vec<Choice>>>,
    fill: Entity<SelectState<Vec<Choice>>>,
    active_fill: MeshFillAlgorithm,
    parameters: MeshFillParameters,
    steps: [f64; 11],
    suppress_diagonals: bool,
    inputs: [Entity<InputState>; 11],
    numbers: [Entity<SlideNumberInput>; 11],
    request: Arc<AtomicU64>,
    task: Option<Task<()>>,
    plan: Option<Arc<MeshGenerationPreview>>,
    error: Option<String>,
    closed: bool,
    _subscriptions: Vec<Subscription>,
}
impl MeshGenerationPanel {
    pub(crate) fn open(
        owner: WeakEntity<EditorView>,
        document: DocumentId,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        let entity = owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("Editor closed"))?;
        entity.update(cx, |owner, cx| {
            owner.cancel_mesh_generation(cx);
            owner
                .editor
                .end_editing(document, zipzi_editor_core::EditingLifecycle::BlockingUi)
        })?;
        let state = entity.read(cx);
        let doc = state
            .editor
            .document()
            .ok_or_else(|| anyhow::anyhow!("Document closed"))?;
        let source = if let Some(target) = state.editor.manual_target() {
            doc.mesh_generation_source_manual(&target)?
        } else {
            doc.mesh_generation_source(&doc.mesh_generation_target()?)?
        };
        anyhow::ensure!(
            source.alpha_error().is_none(),
            "{}",
            source.alpha_error().unwrap_or_default()
        );
        let context = state.editor.editing_context_token().unwrap();
        let viewport = state.focused_viewport.unwrap_or(state.viewport_slots[0]);
        let texture = source
            .texture
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing source texture"))?;
        let controls = Controls::for_texture(texture.width, texture.height);
        let columns = column_count(
            state
                .editor
                .viewport_metrics(viewport)
                .map_or(470., |m| m.width),
            cx,
        );
        let panel = cx.new(|cx| {
            let make = |choices: Vec<Choice>, window: &mut Window, cx: &mut Context<Self>| {
                cx.new(|cx| SelectState::new(choices, Some(IndexPath::new(0)), window, cx))
            };
            let choice = |id, label: SharedString| Choice { id, label };
            let edge = make(
                vec![
                    choice("single", t!("mesh_generation.edge_single").into()),
                    choice("double", t!("mesh_generation.edge_double").into()),
                    choice("triple", t!("mesh_generation.edge_triple").into()),
                ],
                window,
                cx,
            );
            let fill = make(
                vec![
                    choice("poisson", t!("mesh_generation.fill_poisson").into()),
                    choice("simple", t!("mesh_generation.fill_simple").into()),
                    choice("quadtree", t!("mesh_generation.fill_quadtree").into()),
                    choice("fractal", t!("mesh_generation.fill_fractal").into()),
                    choice("paving", t!("mesh_generation.fill_paving").into()),
                ],
                window,
                cx,
            );
            let defaults = controls.defaults;
            let inputs = std::array::from_fn(|i| {
                cx.new(|cx| {
                    InputState::new(window, cx).default_value(parameter_text(i, defaults[i]))
                })
            });
            let numbers = std::array::from_fn(|i| {
                cx.new(|cx| {
                    let n = SlideNumberInput::new(
                        inputs[i].clone(),
                        "",
                        number_options(i, defaults[i], controls.steps[i]),
                        window,
                        cx,
                    )
                    .xsmall()
                    .snap_to_step();
                    if matches!(i, 5 | 10) {
                        n.truncate_to_integer()
                    } else {
                        n.truncate_to_one_decimal()
                    }
                })
            });
            let mut subscriptions = vec![];
            for i in 0..11 {
                subscriptions.push(cx.subscribe_in(
                    &numbers[i],
                    window,
                    move |this: &mut Self, _, change: &SlideNumberChange, window, cx| {
                        this.inputs[i].update(cx, |input, cx| {
                            input.set_value(parameter_text(i, change.0), window, cx)
                        });
                        this.refresh(cx);
                    },
                ));
                subscriptions.push(cx.subscribe_in(
                    &inputs[i],
                    window,
                    move |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                        InputEvent::Change => this.refresh(cx),
                        InputEvent::Blur | InputEvent::PressEnter { .. } => {
                            if let Ok(value) = this.number(i, cx) {
                                let text = parameter_text(i, value);
                                if this.inputs[i].read(cx).value().as_ref() != text {
                                    this.inputs[i]
                                        .update(cx, |input, cx| input.set_value(text, window, cx));
                                }
                            }
                        }
                        _ => {}
                    },
                ));
            }
            subscriptions.push(cx.subscribe(
                &edge,
                |this: &mut Self, _, _: &SelectEvent<Vec<Choice>>, cx| this.refresh(cx),
            ));
            subscriptions.push(cx.subscribe_in(
                &fill,
                window,
                |this: &mut Self, _, _: &SelectEvent<Vec<Choice>>, window, cx| {
                    if let Ok(p) = this.read_parameters(cx) {
                        *this.parameters.get_mut(this.active_fill) = p;
                    }
                    this.active_fill = this.fill_kind(cx);
                    let p = this.parameters.get(this.active_fill);
                    for (i, value) in [
                        p.edge_ratio,
                        p.gradation,
                        p.jitter,
                        p.angle,
                        p.max_rows as f64,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        this.inputs[i + 6].update(cx, |input, cx| {
                            input.set_value(parameter_text(i + 6, value), window, cx)
                        });
                    }
                    this.refresh(cx);
                },
            ));
            subscriptions.push(cx.observe(&entity, |this: &mut Self, owner, cx| {
                if this.closed {
                    return;
                }
                let state = owner.read(cx);
                let columns = column_count(
                    state
                        .editor
                        .viewport_metrics(this.viewport)
                        .map_or(470., |m| m.width),
                    cx,
                );
                let layout_changed = columns != this.columns;
                this.columns = columns;
                if !state.editor.editing_context_token_valid(this.context)
                    || state.editor.is_busy()
                    || state
                        .editor
                        .document()
                        .is_none_or(|d| !this.source.is_current(d))
                    || state.editor.document().is_some_and(|d| {
                        d.context().selection.as_ref() != Some(this.source.target())
                    })
                {
                    this.closed = true;
                    this.request.fetch_add(1, Ordering::Relaxed);
                    let owner = owner.clone();
                    let panel_id = cx.entity_id();
                    cx.defer(move |cx| {
                        owner.update(cx, |owner, cx| {
                            if owner.owns_mesh_generation(panel_id) {
                                owner.cancel_mesh_generation(cx);
                            }
                        })
                    });
                }
                if layout_changed {
                    cx.notify();
                }
            }));
            Self {
                owner,
                document,
                viewport,
                context,
                source: Arc::new(source),
                contour: None,
                stage: Stage::Edges,
                columns,
                edge,
                fill,
                active_fill: MeshFillAlgorithm::GradedPoisson,
                parameters: controls.fill,
                steps: controls.steps,
                suppress_diagonals: false,
                inputs,
                numbers,
                request: Default::default(),
                task: None,
                plan: None,
                error: None,
                closed: false,
                _subscriptions: subscriptions,
            }
        });
        entity.update(cx, |owner, cx| {
            owner.mesh_generation = Some(panel.clone());
            owner.redraw(cx);
        });
        panel.update(cx, |this, cx| this.refresh(cx));
        Ok(())
    }
    fn edge_kind(&self, cx: &App) -> MeshEdgeMode {
        match self.edge.read(cx).selected_value().copied() {
            Some("double") => MeshEdgeMode::Double,
            Some("triple") => MeshEdgeMode::Triple,
            _ => MeshEdgeMode::Single,
        }
    }
    fn fill_kind(&self, cx: &App) -> MeshFillAlgorithm {
        match self.fill.read(cx).selected_value().copied() {
            Some("simple") => MeshFillAlgorithm::SimpleTriangles,
            Some("quadtree") => MeshFillAlgorithm::AdaptiveQuadtree,
            Some("fractal") => MeshFillAlgorithm::TriangleFractal,
            Some("paving") => MeshFillAlgorithm::ContourPaving,
            _ => MeshFillAlgorithm::GradedPoisson,
        }
    }
    fn number(&self, i: usize, cx: &App) -> anyhow::Result<f64> {
        self.inputs[i]
            .read(cx)
            .value()
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(|value| parameter_value(i, value))
            .ok_or_else(|| anyhow::anyhow!(t!("mesh_generation.invalid_alpha")))
    }
    fn read_parameters(&self, cx: &App) -> anyhow::Result<FillParameters> {
        let rows = self.number(10, cx)?;
        anyhow::ensure!(
            rows.fract() == 0. && (0. ..=24.).contains(&rows),
            t!("mesh_generation.invalid_alpha")
        );
        Ok(FillParameters {
            edge_ratio: self.number(6, cx)?,
            gradation: self.number(7, cx)?,
            jitter: self.number(8, cx)?,
            angle: self.number(9, cx)?,
            max_rows: rows as usize,
        })
    }
    fn settings(&self, cx: &App) -> anyhow::Result<MeshSettings> {
        let mut s = MeshSettings {
            clip_to_image: true,
            ..Default::default()
        };
        if self.stage == Stage::Edges {
            let threshold = self.number(5, cx)?;
            anyhow::ensure!(
                threshold.fract() == 0. && (1. ..=255.).contains(&threshold),
                t!("mesh_generation.invalid_alpha")
            );
            s.edge_mode = self.edge_kind(cx);
            s.edge_spacing = self.number(0, cx)?;
            s.outer_margin = self.number(2, cx)?;
            s.edge_width = self.number(3, cx)?;
            s.wrap = self.number(4, cx)?;
            s.alpha_threshold = threshold as u8;
        } else {
            s.interior_spacing = self.number(1, cx)?;
            s.fill_algorithm = self.fill_kind(cx);
            s.suppress_boundary_diagonals = self.suppress_diagonals;
            s.fill_parameters = self.parameters;
            if s.fill_algorithm != MeshFillAlgorithm::SimpleTriangles {
                *s.fill_parameters.get_mut(s.fill_algorithm) = self.read_parameters(cx)?;
            }
        }
        s.validate()?;
        Ok(s)
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        let request = self.request.fetch_add(1, Ordering::Relaxed) + 1;
        self.source.invalidate_previews();
        self.plan = None;
        self.error = None;
        let settings = match self.settings(cx) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(e.to_string());
                let owner = self.owner.clone();
                let document = self.document;
                let panel_id = cx.entity_id();
                cx.defer(move |cx| {
                    _ = owner.update(cx, |owner, cx| {
                        if !owner.owns_mesh_generation(panel_id) {
                            return;
                        }
                        _ = owner.editor.preview_mesh_generation(document, None);
                        owner.redraw_canvas(cx);
                    });
                });
                cx.notify();
                return;
            }
        };
        let source = if self.stage == Stage::Interior {
            self.contour
                .as_ref()
                .map(|g| self.source.with_geometry(g.clone()))
                .unwrap_or_else(|| (*self.source).clone())
        } else {
            (*self.source).clone()
        };
        let options = if self.stage == Stage::Edges {
            MeshGenerationOptions::ManualEdge(settings)
        } else {
            MeshGenerationOptions::ManualInterior(settings)
        };
        let stage = self.stage;
        let latest = self.request.clone();
        let executor = cx.background_executor().clone();
        let timer = executor.timer(std::time::Duration::from_millis(100));
        let work = executor.spawn(async move {
            timer.await;
            if latest.load(Ordering::Relaxed) != request {
                return None;
            }
            let result = source.generate(options);
            if latest.load(Ordering::Relaxed) != request {
                return None;
            }
            Some(result)
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            _ = this.update(cx, |this, cx| {
                if this.closed || this.request.load(Ordering::Relaxed) != request {
                    return;
                }
                match result.expect("current generation") {
                    Ok(plan) => {
                        let plan = Arc::new(plan);
                        let result = this.owner.update(cx, |owner, cx| {
                            anyhow::ensure!(
                                owner.editor.editing_context_token_valid(this.context),
                                "Generation context changed"
                            );
                            let result = owner
                                .editor
                                .preview_mesh_generation(this.document, Some(&plan));
                            owner.redraw_canvas(cx);
                            result
                        });
                        match result {
                            Ok(Ok(())) => {
                                if stage == Stage::Edges {
                                    this.contour = Some(plan.geometry().clone());
                                }
                                this.plan = Some(plan);
                            }
                            Ok(Err(e)) => this.error = Some(e.to_string()),
                            Err(e) => this.error = Some(e.to_string()),
                        }
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                if this.error.is_some() {
                    _ = this.owner.update(cx, |owner, cx| {
                        _ = owner.editor.preview_mesh_generation(this.document, None);
                        owner.redraw_canvas(cx);
                    });
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn apply(&mut self, cx: &mut Context<Self>) {
        let Some(plan) = self.plan.clone() else {
            return;
        };
        self.closed = true;
        self.request.fetch_add(1, Ordering::Relaxed);
        let document = self.document;
        let owner = self.owner.clone();
        let panel_id = cx.entity_id();
        cx.defer(move |cx| {
            _ = owner.update(cx, |owner, cx| {
                if !owner.owns_mesh_generation(panel_id) {
                    return;
                }
                let result = owner.editor.execute_for(
                    document,
                    zipzi_editor_core::EditorCommand::ApplyMeshGeneration(plan.clone()),
                );
                owner.cancel_mesh_generation(cx);
                owner.complete_edit(result, cx);
            });
        });
    }
    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.closed = true;
        self.request.fetch_add(1, Ordering::Relaxed);
        let owner = self.owner.clone();
        let panel_id = cx.entity_id();
        cx.defer(move |cx| {
            _ = owner.update(cx, |owner, cx| {
                if owner.owns_mesh_generation(panel_id) {
                    owner.cancel_mesh_generation(cx);
                }
            });
        });
    }
}
/// Counts remain integral; all continuous controls use the same precision for
/// preview generation, typed input and scrubbing.
fn parameter_value(i: usize, value: f64) -> f64 {
    if matches!(i, 5 | 10) {
        value
    } else {
        (value * 10.).trunc() / 10.
    }
}
fn parameter_text(i: usize, value: f64) -> String {
    if matches!(i, 5 | 10) {
        value.to_string()
    } else {
        format!("{:.1}", parameter_value(i, value))
    }
}
fn number_options(i: usize, value: f64, step: f64) -> SlideNumberOptions {
    SlideNumberOptions {
        value: parameter_value(i, value),
        min: FIELDS[i].2,
        max: FIELDS[i].3,
        step,
        drag_distance: None,
        disabled: false,
    }
}
fn column_count(viewport_width: f32, cx: &App) -> u16 {
    let available = viewport_width / compact::scale(cx) - 64.;
    if available >= 390. {
        3
    } else if available >= 250. {
        2
    } else {
        1
    }
}
const LABEL_WIDTH: f32 = 54.;
const VALUE_WIDTH: f32 = 64.;
const FIELD_GAP: f32 = 4.;
const FIELD_WIDTH: f32 = LABEL_WIDTH + FIELD_GAP + VALUE_WIDTH;

fn field_row(label: &str, control: impl IntoElement, cx: &App) -> Div {
    let tooltip = t!(label).into_owned();
    let key = format!("{label}_compact");
    let short = t!(key.as_str()).into_owned();
    div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .w(compact::rem(FIELD_WIDTH))
        .max_w_full()
        .min_h(compact::rem(18.))
        .min_w_0()
        .gap(compact::rem(FIELD_GAP))
        .text_size(compact::rem(compact::CAPTION))
        .line_height(compact::rem(14.))
        .child(
            div()
                .id(SharedString::from(format!("mesh-generation-label-{label}")))
                .w(compact::rem(LABEL_WIDTH))
                .flex_shrink_0()
                .min_w_0()
                .truncate()
                .text_color(cx.theme().muted_foreground)
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .child(short),
        )
        .child(control)
}
fn parameter_row(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .test_support()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x(compact::rem(8.))
        .gap_y(compact::rem(2.))
}
fn panel_button(button: Button) -> Button {
    compact::button(button)
        .xsmall()
        .h(compact::rem(20.))
        .min_w(compact::rem(20.))
        .px(compact::rem(4.))
}
impl Render for MeshGenerationPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let edges = self.stage == Stage::Edges;
        let columns = self.owner.upgrade().map_or(self.columns, |owner| {
            column_count(
                owner
                    .read(cx)
                    .editor
                    .viewport_metrics(self.viewport)
                    .map_or(470., |m| m.width),
                cx,
            )
        });
        self.columns = columns;
        let narrow = columns == 1;
        let select = if edges { &self.edge } else { &self.fill };
        let mut fields = parameter_row("mesh-generation-fields").child(
            div()
                .w(compact::rem(FIELD_WIDTH))
                .max_w_full()
                .flex_shrink_0()
                .child(
                    Select::new(select)
                        .xsmall()
                        .h(compact::rem(18.))
                        .accessibility_label(
                            t!(if edges {
                                "mesh_generation.edge_mode"
                            } else {
                                "mesh_generation.algorithm"
                            })
                            .into_owned(),
                        )
                        .id(if edges {
                            "mesh-generation-edge-mode"
                        } else {
                            "mesh-generation-fill-algorithm"
                        })
                        .w_full(),
                ),
        );
        let mut details = parameter_row("mesh-generation-details");
        let kind = self.fill_kind(cx);
        for (i, (id, label, _, _)) in FIELDS.iter().enumerate() {
            let visible = if edges {
                matches!(i, 0 | 4 | 5)
                    || (i == 2 && self.edge_kind(cx) == MeshEdgeMode::Single)
                    || (i == 3 && self.edge_kind(cx) != MeshEdgeMode::Single)
            } else {
                i == 1
                    || (matches!(i, 6 | 7) && kind != MeshFillAlgorithm::SimpleTriangles)
                    || (i == 8 && kind == MeshFillAlgorithm::GradedPoisson)
                    || (i == 9
                        && matches!(
                            kind,
                            MeshFillAlgorithm::AdaptiveQuadtree
                                | MeshFillAlgorithm::TriangleFractal
                        ))
                    || (i == 10 && kind == MeshFillAlgorithm::ContourPaving)
            };
            if !visible {
                continue;
            }
            let value = self.number(i, cx).unwrap_or(FIELDS[i].2);
            self.numbers[i].update(cx, |number, cx| {
                number.configure(
                    t!(*label).into_owned(),
                    number_options(i, value, self.steps[i]),
                    cx,
                )
            });
            let field = field_row(
                label,
                div()
                    .id(*id)
                    .test_support()
                    .w(compact::rem(VALUE_WIDTH))
                    .flex_shrink_0()
                    .child(self.numbers[i].clone()),
                cx,
            );
            // Keep the layout, density and boundary size together. Fine tuning
            // and preview feedback share the following row at every width.
            if matches!(i, 0 | 1 | 2 | 3 | 6) {
                fields = fields.child(field);
            } else {
                details = details.child(field);
            }
        }
        let stages = Tabs::new("mesh-generation-stage-tabs")
            .flex()
            .flex_shrink_0()
            .h(compact::rem(compact::HEADER_CONTROL))
            .gap(compact::rem(8.))
            .children(
                [
                    (
                        Stage::Edges,
                        "mesh-generation-stage-edges",
                        "mesh_generation.edges_short",
                    ),
                    (
                        Stage::Interior,
                        "mesh-generation-stage-interior",
                        "mesh_generation.interior_short",
                    ),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (stage, id, label))| {
                    let selected = self.stage == stage;
                    Tab::new(id)
                        .h_full()
                        .px(compact::rem(4.))
                        .border_b_2()
                        .border_color(if selected {
                            cx.theme().foreground
                        } else {
                            transparent_black()
                        })
                        .text_size(compact::rem(compact::BODY))
                        .text_color(if selected {
                            cx.theme().foreground
                        } else {
                            cx.theme().muted_foreground
                        })
                        .selected(selected)
                        .accessibility_label(t!(label))
                        .set_position(index + 1, 2)
                        .when(!selected, |tab| {
                            tab.cursor_pointer()
                                .hover(|style| style.text_color(cx.theme().foreground))
                        })
                        .child(t!(label))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.stage != stage {
                                this.stage = stage;
                                this.refresh(cx);
                            }
                        }))
                }),
            );
        let actions = div()
            .id("mesh-generation-actions")
            .test_support()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(compact::rem(2.))
            .child(
                panel_button(Button::new("mesh-generation-cancel"))
                    .ghost()
                    .label(t!("mesh_generation.cancel"))
                    .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
            )
            .child(
                panel_button(Button::new("mesh-generation-submit"))
                    .primary()
                    .label(t!("mesh_generation.apply"))
                    .disabled(self.plan.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.apply(cx))),
            );
        let header = if narrow {
            div()
                .flex()
                .flex_col()
                .gap(compact::rem(4.))
                .child(div().flex().justify_end().child(actions))
                .child(stages)
        } else {
            div()
                .flex()
                .items_center()
                .gap(compact::rem(4.))
                .child(stages)
                .child(div().flex_1())
                .child(actions)
        };
        let status = if let Some(error) = &self.error {
            div()
                .id("mesh-generation-error")
                .test_support()
                .child(error.clone())
        } else if let Some(plan) = &self.plan {
            let vertices = plan.geometry().vertex_ids.len();
            let triangles = plan.geometry().triangles.len();
            let full_counts = t!(
                "mesh_generation.counts",
                vertices = vertices,
                triangles = triangles
            )
            .into_owned();
            div()
                .id("mesh-generation-counts")
                .test_support()
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(full_counts.clone())
                        .build(window, cx)
                })
                .child(
                    t!(
                        "mesh_generation.counts_compact",
                        vertices = vertices,
                        triangles = triangles
                    )
                    .into_owned(),
                )
        } else {
            div()
                .id("mesh-generation-preparing")
                .test_support()
                .child(t!("mesh_generation.preparing"))
        };
        let status = status
            .min_w_0()
            .text_size(compact::rem(compact::CAPTION))
            .line_height(compact::rem(18.))
            .text_color(if self.error.is_some() {
                cx.theme().danger
            } else {
                cx.theme().muted_foreground
            });
        // Errors get the full width for readable wrapping; normal feedback
        // uses the spare space beside the fine-tuning controls.
        let error = if self.error.is_some() {
            Some(status)
        } else {
            details = details.child(status.ml_auto());
            None
        };
        let content = div()
            .relative()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(compact::rem(4.))
            .text_size(compact::rem(compact::BODY))
            .child(header)
            .child(fields)
            .child(details)
            .when(!edges, |content| {
                content.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_between()
                        .gap(compact::rem(4.))
                        .child(
                            panel_button(Button::new("mesh-generation-use-original"))
                                .ghost()
                                .label(t!("mesh_generation.use_original"))
                                .disabled(self.contour.is_none())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.contour = None;
                                    this.refresh(cx);
                                })),
                        )
                        .child(
                            Checkbox::new("mesh-generation-suppress-diagonals")
                                .xsmall()
                                .label(t!("mesh_generation.suppress_diagonals").into_owned())
                                .checked(self.suppress_diagonals)
                                .on_click(cx.listener(|this, value: &bool, _, cx| {
                                    this.suppress_diagonals = *value;
                                    this.refresh(cx);
                                })),
                        ),
                )
            })
            .children(error);
        let content = content.on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
            if event.keystroke.key == "escape" {
                this.cancel(cx);
                cx.stop_propagation();
            }
        }));
        crate::viewports::toolbar::generation_bar(content, cx)
    }
}
impl EditorView {
    fn owns_mesh_generation(&self, id: EntityId) -> bool {
        self.mesh_generation
            .as_ref()
            .is_some_and(|panel| panel.entity_id() == id)
    }
    pub(crate) fn cancel_mesh_generation(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.mesh_generation.take() else {
            return;
        };
        let document = panel.read(cx).document;
        panel.update(cx, |panel, _| {
            panel.closed = true;
            panel.request.fetch_add(1, Ordering::Relaxed);
        });
        _ = self.editor.preview_mesh_generation(document, None);
        self.redraw_canvas(cx);
        cx.notify();
    }
}

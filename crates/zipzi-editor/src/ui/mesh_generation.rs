//! Viewport-owned, two-stage generation. Drafts never create history entries.
use super::slide_number_input::{SlideNumberChange, SlideNumberInput, SlideNumberOptions};
use crate::{EditorView, t};
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
        4096.,
    ),
    (
        "mesh-generation-interior-spacing",
        "mesh_generation.interior_spacing",
        6.,
        4096.,
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
        0.25,
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
    edge: Entity<SelectState<Vec<Choice>>>,
    fill: Entity<SelectState<Vec<Choice>>>,
    active_fill: MeshFillAlgorithm,
    parameters: MeshFillParameters,
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
            let defaults = [6., 40., 1., 10., 0., 8., 2., 1., 0.45, 0., 12.];
            let inputs = std::array::from_fn(|i| {
                cx.new(|cx| InputState::new(window, cx).default_value(defaults[i].to_string()))
            });
            let numbers = std::array::from_fn(|i| {
                cx.new(|cx| {
                    let n = SlideNumberInput::new(
                        inputs[i].clone(),
                        "",
                        number_options(i, defaults[i]),
                        window,
                        cx,
                    );
                    if matches!(i, 5 | 10) {
                        n.truncate_to_integer()
                    } else {
                        n
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
                            input.set_value(change.0.to_string(), window, cx)
                        });
                    },
                ));
                subscriptions.push(cx.subscribe(
                    &inputs[i],
                    |this: &mut Self, _, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) {
                            this.refresh(cx);
                        }
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
                            input.set_value(value.to_string(), window, cx)
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
            }));
            Self {
                owner,
                document,
                viewport,
                context,
                source: Arc::new(source),
                contour: None,
                stage: Stage::Edges,
                edge,
                fill,
                active_fill: MeshFillAlgorithm::GradedPoisson,
                parameters: Default::default(),
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
fn number_options(i: usize, value: f64) -> SlideNumberOptions {
    SlideNumberOptions {
        value,
        min: FIELDS[i].2,
        max: FIELDS[i].3,
        step: if i >= 6 && i <= 8 { 0.05 } else { 1. },
        drag_distance: Some(2000.),
        disabled: false,
    }
}
impl Render for MeshGenerationPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut fields = div().flex().flex_wrap().gap_2();
        let select = if self.stage == Stage::Edges {
            &self.edge
        } else {
            &self.fill
        };
        fields = fields.child(
            div().w(crate::ui::compact::rem(180.)).child(
                crate::ui::compact::select(Select::new(select))
                    .id(if self.stage == Stage::Edges {
                        "mesh-generation-edge-mode"
                    } else {
                        "mesh-generation-fill-algorithm"
                    })
                    .w_full(),
            ),
        );
        let kind = self.fill_kind(cx);
        for (i, (id, label, _, _)) in FIELDS.iter().enumerate() {
            let visible = if self.stage == Stage::Edges {
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
                number.configure(t!(*label).into_owned(), number_options(i, value), cx)
            });
            fields = fields.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .child(t!(*label).into_owned())
                    .child(
                        div()
                            .id(*id)
                            .test_support()
                            .w(crate::ui::compact::rem(64.))
                            .child(self.numbers[i].clone()),
                    ),
            );
        }
        if self.stage == Stage::Interior {
            fields = fields.child(
                crate::ui::compact::button(Button::new("mesh-generation-use-original"))
                    .label(t!("mesh_generation.use_original"))
                    .disabled(self.contour.is_none())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.contour = None;
                        this.refresh(cx);
                    })),
            );
            fields = fields.child(
                Checkbox::new("mesh-generation-suppress-diagonals")
                    .small()
                    .label(t!("mesh_generation.suppress_diagonals").into_owned())
                    .checked(self.suppress_diagonals)
                    .on_click(cx.listener(|this, value: &bool, _, cx| {
                        this.suppress_diagonals = *value;
                        this.refresh(cx);
                    })),
            );
        }
        let stages = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                crate::ui::compact::button(Button::new("mesh-generation-stage-edges"))
                    .label(t!("mesh_generation.stage_edges"))
                    .when(self.stage == Stage::Edges, |b| b.primary())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.stage = Stage::Edges;
                        this.refresh(cx);
                    })),
            )
            .child(
                crate::ui::compact::button(Button::new("mesh-generation-stage-interior"))
                    .label(t!("mesh_generation.stage_interior"))
                    .when(self.stage == Stage::Interior, |b| b.primary())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.stage = Stage::Interior;
                        this.refresh(cx);
                    })),
            )
            .child(div().flex_1())
            .child(
                crate::ui::compact::button(Button::new("mesh-generation-submit"))
                    .label(t!("mesh_generation.apply"))
                    .disabled(self.plan.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.apply(cx))),
            )
            .child(
                crate::ui::compact::button(Button::new("mesh-generation-cancel"))
                    .label(t!("mesh_generation.cancel"))
                    .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
            );
        div()
            .id("mesh-generation-panel")
            .test_support()
            .w_full()
            .max_w(crate::ui::compact::rem(850.))
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .rounded_b_md()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.cancel(cx);
                    cx.stop_propagation();
                }
            }))
            .child(stages)
            .child(fields)
            .child(if let Some(error) = &self.error {
                div()
                    .id("mesh-generation-error")
                    .test_support()
                    .text_xs()
                    .child(error.clone())
            } else if let Some(plan) = &self.plan {
                div()
                    .id("mesh-generation-counts")
                    .test_support()
                    .text_xs()
                    .child(format!(
                        "{}: {} · {}: {}",
                        t!("mesh_generation.vertices"),
                        plan.geometry().vertex_ids.len(),
                        t!("mesh_generation.triangles"),
                        plan.geometry().triangles.len()
                    ))
            } else {
                div()
                    .id("mesh-generation-preparing")
                    .test_support()
                    .text_xs()
                    .child(t!("mesh_generation.preparing"))
            })
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

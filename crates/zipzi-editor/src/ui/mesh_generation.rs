use super::form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView};
use super::mesh_generation_preview::{
    MeshGenerationPreview, PreviewContent, PreviewPaintStatus, PreviewPainter,
};
use super::slide_number_input::{SlideNumberChange, SlideNumberInput, SlideNumberOptions};
use crate::{EditorView, t};
use gpui_kit::component::{
    ActiveTheme, IndexPath, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{InputEvent, InputState},
    select::{Select, SelectEvent, SelectItem, SelectState},
};
use gpui_kit::{prelude::*, *};
use std::sync::Arc;
use zipzi_editor_core::adaptive_mesh::{
    FillParameters, MeshEdgeMode, MeshFillAlgorithm, MeshFillParameters,
};
use zipzi_editor_core::{
    CommandId, DocumentId, EditorCommand, MeshGenerationOptions,
    MeshGenerationPreview as MeshGenerationPlan, MeshGenerationSource, MeshGridOptions,
    MeshSettings, ObjectRef,
};
mod generation;
mod validation;
mod view;

#[derive(Clone)]
struct AlgorithmChoice {
    label: SharedString,
    id: &'static str,
    available: bool,
}

#[derive(Clone, Copy)]
struct NumberField {
    min: f64,
    max: f64,
}

const MESH_SLIDE_DISTANCE_PX: f64 = 2000.;

impl NumberField {
    fn options(self, input: &Entity<InputState>, disabled: bool, cx: &App) -> SlideNumberOptions {
        let value = parsed_number(&input.read(cx).value()).unwrap_or(self.min);
        SlideNumberOptions {
            value,
            min: self.min,
            max: self.max,
            step: if self.max <= 4. { 0.05 } else { 1. },
            drag_distance: Some(MESH_SLIDE_DISTANCE_PX),
            disabled,
        }
    }
}

const GRID_FIELD: NumberField = NumberField { min: 1., max: 255. };
impl SelectItem for AlgorithmChoice {
    type Value = &'static str;
    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &Self::Value {
        &self.id
    }
    fn disabled(&self) -> bool {
        !self.available
    }
}

/// Presentation data is built on a worker, never by mutating the live document.
pub(crate) struct MeshGenerationForm {
    context: zipzi_editor_core::EditingContextToken,
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    revision: u64,
    target: ObjectRef,
    name: String,
    square: bool,
    algorithm: Entity<SelectState<Vec<AlgorithmChoice>>>,
    inputs: [Entity<InputState>; 2],
    numbers: [Entity<SlideNumberInput>; 2],
    alpha_inputs: [Entity<InputState>; 11],
    alpha_numbers: [Entity<SlideNumberInput>; 11],
    alpha_fields: [NumberField; 11],
    edge_mode: Entity<SelectState<Vec<AlgorithmChoice>>>,
    fill_algorithm: Entity<SelectState<Vec<AlgorithmChoice>>>,
    active_fill: MeshFillAlgorithm,
    fill_parameters: MeshFillParameters,
    suppress_boundary_diagonals: bool,
    dialog: FormDialogState,
    validation: anyhow::Result<MeshGenerationOptions>,
    preview: generation::PreviewGeneration,
    viewport: Entity<MeshGenerationPreview>,
    _subscriptions: Vec<Subscription>,
}

impl MeshGenerationForm {
    pub fn open(
        owner: WeakEntity<EditorView>,
        document: DocumentId,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        let entity = owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("mesh_generation.stale")))?;
        let dialog_open = window.has_active_dialog(cx);
        let state = entity.read(cx);
        anyhow::ensure!(
            state.editor.active_id() == Some(document)
                && !state.modal
                && !dialog_open
                && state
                    .editor
                    .command_available(state.editor.command(CommandId::GenerateMesh)),
            t!("mesh_generation.stale")
        );
        entity.update(cx, |view, cx| {
            view.editor
                .end_editing(document, zipzi_editor_core::EditingLifecycle::BlockingUi)?;
            view.redraw(cx);
            Ok::<_, anyhow::Error>(())
        })?;
        let state = entity.read(cx);
        let context = state.editor.editing_context_token().unwrap();
        let doc = state.editor.document().unwrap();
        let manual_target = state.editor.manual_target();
        let (target, source) = if let Some(manual_target) = &manual_target {
            (
                manual_target.object.clone(),
                doc.mesh_generation_source_manual(manual_target)?,
            )
        } else {
            let target = doc.mesh_generation_target()?;
            let source = doc.mesh_generation_source(&target)?;
            (target, source)
        };
        let source = Arc::new(source);
        let name = doc.object_index().name(&target).to_owned();
        let revision = doc.mesh_generation_revision();
        let square = doc.mesh_grid_requires_square(&target);
        let alpha_available = source.alpha_error().is_none();
        let form = cx.new(|cx| {
            let defaults = MeshGridOptions::default();
            let inputs = [defaults.columns, defaults.rows].map(|n| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .step(1.)
                        .default_value(n.to_string())
                })
            });
            // Pixel fields use a useful image-relative scrub range; typed values
            // continue to use the generator's existing validation limits.
            let image_side = source
                .texture
                .as_ref()
                .map_or(256., |texture| texture.width.max(texture.height) as f64);
            let spacing = NumberField {
                min: 6.,
                max: image_side.max(256.),
            };
            let margin = NumberField {
                min: 0.,
                max: (image_side / 4.).max(64.),
            };
            let alpha_fields = [
                spacing,
                spacing,
                margin,
                margin,
                NumberField { min: 0., max: 64. },
                NumberField { min: 1., max: 255. },
                NumberField { min: 1., max: 4. },
                NumberField { min: 0.25, max: 4. },
                NumberField { min: 0., max: 1. },
                NumberField { min: 0., max: 90. },
                NumberField { min: 0., max: 24. },
            ];
            let alpha_defaults = alpha_input_values(alpha_presets()[1]);
            let alpha_inputs: [Entity<InputState>; 11] = std::array::from_fn(|i| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .step(1.)
                        .default_value(alpha_defaults[i].clone())
                })
            });
            let numbers = std::array::from_fn(|i| {
                cx.new(|cx| {
                    SlideNumberInput::new(
                        inputs[i].clone(),
                        "",
                        GRID_FIELD.options(&inputs[i], false, cx),
                        window,
                        cx,
                    )
                    .truncate_to_integer()
                })
            });
            let alpha_numbers = std::array::from_fn(|i| {
                cx.new(|cx| {
                    let number = SlideNumberInput::new(
                        alpha_inputs[i].clone(),
                        "",
                        alpha_fields[i].options(&alpha_inputs[i], false, cx),
                        window,
                        cx,
                    );
                    if matches!(i, 5 | 10) {
                        number.truncate_to_integer()
                    } else {
                        number
                    }
                })
            });
            let choices = if source.manual_available() {
                vec![
                    AlgorithmChoice {
                        label: t!("mesh_generation.manual_edge").into(),
                        id: "manual-edge",
                        available: true,
                    },
                    AlgorithmChoice {
                        label: t!("mesh_generation.manual_interior").into(),
                        id: "manual-interior",
                        available: true,
                    },
                    AlgorithmChoice {
                        label: t!("mesh_generation.manual_reconnect").into(),
                        id: "manual-reconnect",
                        available: true,
                    },
                    AlgorithmChoice {
                        label: t!("mesh_generation.manual_full").into(),
                        id: "manual-full",
                        available: true,
                    },
                ]
            } else {
                vec![
                    AlgorithmChoice {
                        label: if source.replaces_modeling_topology() {
                            t!("mesh_generation.alpha_replaces").into()
                        } else {
                            t!("mesh_generation.alpha").into()
                        },
                        id: "alpha-cdt",
                        available: source.alpha_error().is_none(),
                    },
                    AlgorithmChoice {
                        label: t!("mesh_generation.grid").into(),
                        id: "grid",
                        available: source.grid_available(),
                    },
                ]
            };
            let algorithm = cx.new(|cx| {
                SelectState::new(
                    choices,
                    Some(IndexPath::new(
                        if source.manual_available() || alpha_available {
                            0
                        } else {
                            1
                        },
                    )),
                    window,
                    cx,
                )
            });
            let edge_mode = cx.new(|cx| {
                SelectState::new(
                    vec![
                        AlgorithmChoice {
                            label: t!("mesh_generation.edge_single").into(),
                            id: "single",
                            available: true,
                        },
                        AlgorithmChoice {
                            label: t!("mesh_generation.edge_double").into(),
                            id: "double",
                            available: true,
                        },
                        AlgorithmChoice {
                            label: t!("mesh_generation.edge_triple").into(),
                            id: "triple",
                            available: true,
                        },
                    ],
                    Some(IndexPath::new(0)),
                    window,
                    cx,
                )
            });
            let fill_algorithm = cx.new(|cx| {
                SelectState::new(
                    vec![
                        AlgorithmChoice {
                            label: t!("mesh_generation.fill_poisson").into(),
                            id: "poisson",
                            available: true,
                        },
                        AlgorithmChoice {
                            label: t!("mesh_generation.fill_simple").into(),
                            id: "simple",
                            available: true,
                        },
                        AlgorithmChoice {
                            label: t!("mesh_generation.fill_quadtree").into(),
                            id: "quadtree",
                            available: true,
                        },
                        AlgorithmChoice {
                            label: t!("mesh_generation.fill_fractal").into(),
                            id: "fractal",
                            available: true,
                        },
                        AlgorithmChoice {
                            label: t!("mesh_generation.fill_paving").into(),
                            id: "paving",
                            available: true,
                        },
                    ],
                    Some(IndexPath::new(0)),
                    window,
                    cx,
                )
            });
            let mut subscriptions = Vec::new();
            for (number, input) in numbers
                .iter()
                .zip(&inputs)
                .chain(alpha_numbers.iter().zip(&alpha_inputs))
            {
                let input = input.clone();
                subscriptions.push(cx.subscribe_in(
                    number,
                    window,
                    move |this: &mut Self, _, event: &SlideNumberChange, window, cx| {
                        if this.dialog.closed() {
                            return;
                        }
                        input.update(cx, |input, cx| {
                            input.set_value(event.0.to_string(), window, cx)
                        });
                        this.dialog.edited();
                        this.refresh(cx);
                        cx.notify();
                    },
                ));
            }
            for input in inputs.iter().chain(&alpha_inputs) {
                subscriptions.push(cx.subscribe_in(
                    input,
                    window,
                    |this: &mut Self, input, event: &InputEvent, window, cx| {
                        if this.dialog.closed() {
                            return;
                        }
                        if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                            let text = input.read(cx).value();
                            if let Some(value) = parsed_number(&text) {
                                let value = value.to_string();
                                if text.as_ref() != value {
                                    input
                                        .update(cx, |input, cx| input.set_value(value, window, cx));
                                    cx.notify();
                                }
                            }
                        }
                        if matches!(event, InputEvent::Change) {
                            this.dialog.edited();
                            this.refresh(cx);
                            cx.notify();
                        }
                        form_dialog::input_enter(event, window, cx);
                    },
                ));
            }
            subscriptions.push(cx.subscribe(
                &algorithm,
                |this: &mut Self, _, _: &SelectEvent<Vec<AlgorithmChoice>>, cx| {
                    this.dialog.edited();
                    this.refresh(cx);
                    cx.notify();
                },
            ));
            subscriptions.push(cx.subscribe(
                &edge_mode,
                |this: &mut Self, _, _: &SelectEvent<Vec<AlgorithmChoice>>, cx| {
                    this.dialog.edited();
                    this.refresh(cx);
                    cx.notify();
                },
            ));
            subscriptions.push(cx.subscribe_in(
                &fill_algorithm,
                window,
                |this: &mut Self, _, _: &SelectEvent<Vec<AlgorithmChoice>>, window, cx| {
                    if let Some(parameters) = this.read_fill_parameters(cx) {
                        *this.fill_parameters.get_mut(this.active_fill) = parameters;
                    }
                    this.active_fill = this.fill_kind(cx);
                    let p = this.fill_parameters.get(this.active_fill);
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
                        this.alpha_inputs[i + 6].update(cx, |input, cx| {
                            input.set_value(value.to_string(), window, cx)
                        });
                    }
                    this.dialog.edited();
                    this.refresh(cx);
                    cx.notify();
                },
            ));
            subscriptions.push(cx.observe(&entity, |this: &mut Self, _, cx| {
                if !this.dialog.closed() {
                    this.refresh(cx);
                    cx.notify();
                }
            }));
            let viewport = cx.new(MeshGenerationPreview::new);
            subscriptions.push(cx.subscribe(
                &viewport,
                |this: &mut Self, _, status: &PreviewPaintStatus, cx| {
                    if !this.preview.accepts_paint(status.0) {
                        return;
                    }
                    match &status.1 {
                        Ok(()) => this.preview.set_painted(true),
                        Err(error) => {
                            this.preview.set_painted(false);
                            this.dialog.error = Some(error.clone());
                        }
                    }
                    this.refresh(cx);
                    cx.notify();
                },
            ));
            Self {
                context,
                owner,
                document,
                revision,
                target,
                preview: generation::PreviewGeneration::new(source.clone(), window.metal_context()),
                name,
                square,
                algorithm,
                inputs,
                numbers,
                alpha_inputs,
                alpha_numbers,
                alpha_fields,
                edge_mode,
                fill_algorithm,
                active_fill: MeshFillAlgorithm::GradedPoisson,
                fill_parameters: MeshFillParameters::default(),
                suppress_boundary_diagonals: false,
                dialog: FormDialogState::new(cx),
                validation: Ok(if alpha_available {
                    if source.manual_available() {
                        MeshGenerationOptions::ManualEdge(MeshSettings {
                            clip_to_image: true,
                            ..alpha_presets()[1]
                        })
                    } else {
                        MeshGenerationOptions::Alpha(alpha_presets()[1])
                    }
                } else {
                    MeshGenerationOptions::Grid(defaults)
                }),
                viewport,
                _subscriptions: subscriptions,
            }
        });
        form.update(cx, |form, cx| {
            form.refresh(cx);
        });
        let options = FormDialogOptions {
            title: t!("mesh_generation.title").into_owned(),
            submit_label: t!("mesh_generation.generate").into(),
            footer_id: "mesh-generation-footer",
            cancel_id: "mesh-generation-cancel",
            submit_id: "mesh-generation-submit",
            initial_focus: form.read(cx).dialog.focus.clone(),
        };
        form_dialog::open(form, options, window, cx);
        Ok(())
    }

    fn is_alpha(&self, cx: &App) -> bool {
        matches!(
            self.algorithm.read(cx).selected_value().copied(),
            Some("alpha-cdt" | "manual-edge" | "manual-interior" | "manual-full")
        )
    }

    fn is_reconnect(&self, cx: &App) -> bool {
        self.algorithm.read(cx).selected_value() == Some(&"manual-reconnect")
    }

    fn wrap_alpha(&self, options: MeshSettings, cx: &App) -> MeshGenerationOptions {
        match self.algorithm.read(cx).selected_value().copied() {
            Some("manual-edge") => MeshGenerationOptions::ManualEdge(options),
            Some("manual-interior") => MeshGenerationOptions::ManualInterior(options),
            Some("manual-full") => MeshGenerationOptions::ManualFull(options),
            _ => MeshGenerationOptions::Alpha(options),
        }
    }

    fn alpha_preset(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let mut options = alpha_presets()[index];
        options.fill_algorithm = self.fill_kind(cx);
        options.fill_parameters = self.fill_parameters;
        let values = alpha_input_values(options);
        for (input, value) in self.alpha_inputs.iter().zip(values) {
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        self.suppress_boundary_diagonals = options.suppress_boundary_diagonals;
        self.dialog.edited();
        self.refresh(cx);
        cx.notify();
    }

    fn preset(&mut self, count: usize, window: &mut Window, cx: &mut Context<Self>) {
        for input in &self.inputs {
            input.update(cx, |input, cx| {
                input.set_value(count.to_string(), window, cx)
            });
        }
        self.dialog.edited();
        self.refresh(cx);
        cx.notify();
    }
}

impl FormDialogView for MeshGenerationForm {
    fn width(window: &Window, cx: &App) -> Pixels {
        // Settings, column gap and dialog padding scale; its 1px border does not.
        crate::ui::compact::scaled(preview_side(window, cx) + 288., cx) + px(2.)
    }
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn error_message(error: &anyhow::Error) -> String {
        error.to_string()
    }
    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let options = self.validate(cx)?;
        anyhow::ensure!(self.preview.painted(), t!("mesh_generation.preparing"));
        let plan = self
            .preview
            .plan()
            .filter(|plan| plan.options() == options)
            .ok_or_else(|| anyhow::anyhow!(t!("mesh_generation.preparing")))?;
        let plan = plan.clone();
        self.owner.upgrade().unwrap().update(cx, |owner, cx| {
            owner
                .editor
                .execute_for(self.document, EditorCommand::ApplyMeshGeneration(plan))?;
            owner.edit_error = None;
            owner.redraw(cx);
            Ok(())
        })
    }
}

fn preview_side(window: &Window, cx: &App) -> f32 {
    let size = window.viewport_size();
    let scale = crate::ui::compact::scale(cx);
    // Work in the same unscaled units as the rem-based form, before fitting it
    // to the viewport. Scaling an already fitted side overflows small windows.
    (size.height.as_f32() / scale - 192.)
        .min(size.width.as_f32() / scale - 336.)
        .max(120.)
}

fn truncated_number(text: &str) -> Option<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map(|value| value.trunc() + 0.)
}

fn alpha_input_values(options: MeshSettings) -> [String; 11] {
    let p = options.fill_parameters.get(options.fill_algorithm);
    [
        options.edge_spacing,
        options.interior_spacing,
        options.outer_margin,
        options.edge_width,
        options.wrap,
        options.alpha_threshold as f64,
        p.edge_ratio,
        p.gradation,
        p.jitter,
        p.angle,
        p.max_rows as f64,
    ]
    .map(|v| v.to_string())
}

fn alpha_presets() -> [MeshSettings; 3] {
    [
        MeshSettings {
            edge_spacing: 12.,
            interior_spacing: 64.,
            ..MeshSettings::default()
        },
        MeshSettings::default(),
        MeshSettings {
            edge_spacing: 6.,
            interior_spacing: 20.,
            ..MeshSettings::default()
        },
    ]
}

impl MeshGenerationForm {
    fn fill_kind(&self, cx: &App) -> MeshFillAlgorithm {
        match self.fill_algorithm.read(cx).selected_value().copied() {
            Some("simple") => MeshFillAlgorithm::SimpleTriangles,
            Some("quadtree") => MeshFillAlgorithm::AdaptiveQuadtree,
            Some("fractal") => MeshFillAlgorithm::TriangleFractal,
            Some("paving") => MeshFillAlgorithm::ContourPaving,
            _ => MeshFillAlgorithm::GradedPoisson,
        }
    }
    fn edge_kind(&self, cx: &App) -> MeshEdgeMode {
        match self.edge_mode.read(cx).selected_value().copied() {
            Some("double") => MeshEdgeMode::Double,
            Some("triple") => MeshEdgeMode::Triple,
            _ => MeshEdgeMode::Single,
        }
    }
    fn read_fill_parameters(&self, cx: &App) -> Option<FillParameters> {
        let get = |i: usize| parsed_number(&self.alpha_inputs[i].read(cx).value());
        let rows = get(10)?;
        if rows.fract() != 0. || !(0. ..=24.).contains(&rows) {
            return None;
        }
        Some(FillParameters {
            edge_ratio: get(6)?,
            gradation: get(7)?,
            jitter: get(8)?,
            angle: get(9)?,
            max_rows: rows as usize,
        })
    }
}
fn parsed_number(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

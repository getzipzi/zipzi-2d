use super::form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView};
use super::mesh_generation_preview::{
    MeshGenerationPreview, PreviewContent, PreviewPaintStatus, PreviewPainter,
};
use super::slide_number_input::{SlideNumberChange, SlideNumberInput, SlideNumberOptions};
use crate::{EditorView, t};
use editor_core::{
    AlphaMeshOptions, CommandId, DocumentId, EditorCommand, MeshGenerationOptions,
    MeshGenerationPreview as MeshGenerationPlan, MeshGenerationSource, MeshGridOptions, ObjectRef,
};
use gpui_kit::component::{
    ActiveTheme, IndexPath, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{InputEvent, InputState},
    select::{Select, SelectEvent, SelectItem, SelectState},
};
use gpui_kit::{prelude::*, *};
use std::sync::Arc;
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
        let value = truncated_number(&input.read(cx).value()).unwrap_or(self.min);
        SlideNumberOptions {
            value,
            min: self.min,
            max: self.max,
            step: 1.,
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
    context: editor_core::EditingContextToken,
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    revision: u64,
    target: ObjectRef,
    name: String,
    square: bool,
    algorithm: Entity<SelectState<Vec<AlgorithmChoice>>>,
    inputs: [Entity<InputState>; 2],
    numbers: [Entity<SlideNumberInput>; 2],
    alpha_inputs: [Entity<InputState>; 7],
    alpha_numbers: [Entity<SlideNumberInput>; 7],
    alpha_fields: [NumberField; 7],
    preserve_holes: bool,
    remove_faint_islands: bool,
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
                .end_editing(document, editor_core::EditingLifecycle::BlockingUi)?;
            view.redraw(cx);
            Ok::<_, anyhow::Error>(())
        })?;
        let state = entity.read(cx);
        let context = state.editor.editing_context_token().unwrap();
        let doc = state.editor.document().unwrap();
        let target = doc.mesh_generation_target()?;
        let source = Arc::new(doc.mesh_generation_source(&target)?);
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
                min: 1.,
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
                margin,
                NumberField { min: 3., max: 256. },
                NumberField { min: 0., max: 255. },
            ];
            let alpha_defaults = alpha_input_values(alpha_presets()[0]);
            let alpha_inputs: [Entity<InputState>; 7] = std::array::from_fn(|i| {
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
                    SlideNumberInput::new(
                        alpha_inputs[i].clone(),
                        "",
                        alpha_fields[i].options(&alpha_inputs[i], false, cx),
                        window,
                        cx,
                    )
                    .truncate_to_integer()
                })
            });
            let algorithm = cx.new(|cx| {
                SelectState::new(
                    vec![
                        AlgorithmChoice {
                            label: t!("mesh_generation.alpha").into(),
                            id: "alpha-cdt",
                            available: source.alpha_error().is_none(),
                        },
                        AlgorithmChoice {
                            label: t!("mesh_generation.grid").into(),
                            id: "grid",
                            available: source.grid_available(),
                        },
                    ],
                    Some(IndexPath::new(if alpha_available { 0 } else { 1 })),
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
                            if let Some(value) = truncated_number(&text) {
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
                preview: generation::PreviewGeneration::new(source, window.metal_context()),
                name,
                square,
                algorithm,
                inputs,
                numbers,
                alpha_inputs,
                alpha_numbers,
                alpha_fields,
                preserve_holes: false,
                remove_faint_islands: true,
                dialog: FormDialogState::new(cx),
                validation: Ok(if alpha_available {
                    MeshGenerationOptions::Alpha(alpha_presets()[0])
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
        self.algorithm.read(cx).selected_value() == Some(&"alpha-cdt")
    }

    fn alpha_preset(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let options = alpha_presets()[index];
        let values = alpha_input_values(options);
        for (input, value) in self.alpha_inputs.iter().zip(values) {
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        self.preserve_holes = options.preserve_holes;
        self.remove_faint_islands = options.remove_faint_islands;
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

fn alpha_input_values(options: AlphaMeshOptions) -> [String; 7] {
    [
        options.outside_spacing.trunc().to_string(),
        options.inside_spacing.trunc().to_string(),
        options.outside_margin.trunc().to_string(),
        options.inside_margin.trunc().to_string(),
        options.minimum_margin.trunc().to_string(),
        options.minimum_boundary_points.to_string(),
        options.alpha_threshold.to_string(),
    ]
}

fn alpha_presets() -> [AlphaMeshOptions; 3] {
    [
        AlphaMeshOptions {
            minimum_boundary_points: 15,
            ..AlphaMeshOptions::deformation_small()
        },
        AlphaMeshOptions::standard(),
        AlphaMeshOptions::deformation_large(),
    ]
    .map(|options| AlphaMeshOptions {
        clip_to_image: false,
        remove_faint_islands: true,
        ..options
    })
}

use super::form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView};
use super::preview_viewport::{
    PreviewContent, PreviewPaintStatus, PreviewPainter, PreviewViewport,
};
use crate::{EditorView, t};
use editor_core::{
    CommandId, DocumentId, EditorCommand, MeshGenerationOptions, MeshGenerationPreview,
    MeshGenerationSource, MeshGridOptions, ObjectRef,
};
use gpui_kit::component::{
    ActiveTheme, IndexPath, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    select::{Select, SelectEvent, SelectItem, SelectState},
};
use gpui_kit::{prelude::*, *};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

#[derive(Clone)]
struct AlgorithmChoice {
    label: SharedString,
    id: &'static str,
    available: bool,
}
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
struct Candidate {
    plan: Arc<MeshGenerationPreview>,
    edges: Arc<[([f32; 2], [f32; 2])]>,
    aspect: f32,
    renderer: Option<super::mesh_preview::MeshPreview>,
}
impl Candidate {
    fn new(
        plan: MeshGenerationPreview,
        texture: Option<editor_core::SceneTexture>,
        gpu: Option<SharedMetalContext>,
    ) -> anyhow::Result<Self> {
        let geometry = plan.geometry();
        let [a, c] = plan.source_bounds();
        let (min_x, min_y, width, height) = (a.x, a.y, c.x - a.x, c.y - a.y);
        let points: HashMap<_, _> = geometry
            .vertex_ids
            .iter()
            .copied()
            .zip(
                geometry
                    .positions
                    .iter()
                    .map(|p| [(p.x - min_x) / width, (p.y - min_y) / height]),
            )
            .collect();
        let mut seen = HashSet::new();
        let mut edges = Vec::new();
        for triangle in &geometry.triangles {
            for i in 0..3 {
                let (a, b) = (triangle[i], triangle[(i + 1) % 3]);
                if seen.insert((a.min(b), a.max(b))) {
                    edges.push((points[&a], points[&b]));
                }
            }
        }
        let scene = texture.map(|texture| plan.scene(texture));
        let renderer = scene
            .map(|scene| super::mesh_preview::MeshPreview::prepare(scene, gpu))
            .transpose()?;
        Ok(Self {
            renderer,
            plan: Arc::new(plan),
            edges: edges.into(),
            aspect: width / height,
        })
    }
}

pub(crate) struct MeshGenerationForm {
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    revision: u64,
    target: ObjectRef,
    source: Arc<MeshGenerationSource>,
    gpu: Option<SharedMetalContext>,
    name: String,
    square: bool,
    algorithm: Entity<SelectState<Vec<AlgorithmChoice>>>,
    inputs: [Entity<InputState>; 2],
    dialog: FormDialogState,
    validation: anyhow::Result<MeshGenerationOptions>,
    requested: Option<MeshGenerationOptions>,
    candidate: Option<Arc<Candidate>>,
    task: Option<Task<()>>,
    request_id: u64,
    painter: Option<PreviewPainter>,
    painted: bool,
    viewport: Entity<PreviewViewport>,
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
        let doc = state.editor.document().unwrap();
        let target = doc.mesh_generation_target()?;
        let source = Arc::new(doc.mesh_generation_source(&target)?);
        let name = doc.object_index().name(&target).to_owned();
        let revision = doc.mesh_generation_revision();
        let square = doc.mesh_grid_requires_square(&target);
        let form = cx.new(|cx| {
            let defaults = MeshGridOptions::default();
            let inputs = [defaults.columns, defaults.rows]
                .map(|n| cx.new(|cx| InputState::new(window, cx).default_value(n.to_string())));
            let algorithm = cx.new(|cx| {
                SelectState::new(
                    vec![
                        AlgorithmChoice {
                            label: t!("mesh_generation.grid").into(),
                            id: "grid",
                            available: true,
                        },
                        AlgorithmChoice {
                            label: t!("mesh_generation.alpha_soon").into(),
                            id: "alpha-cdt",
                            available: false,
                        },
                    ],
                    Some(IndexPath::new(0)),
                    window,
                    cx,
                )
            });
            let mut subscriptions = Vec::new();
            for input in &inputs {
                subscriptions.push(cx.subscribe_in(
                    input,
                    window,
                    |this: &mut Self, _, event: &InputEvent, window, cx| {
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
            let viewport = cx.new(PreviewViewport::new);
            subscriptions.push(cx.subscribe(
                &viewport,
                |this: &mut Self, _, status: &PreviewPaintStatus, cx| {
                    if status.0 != this.request_id || this.candidate.is_none() {
                        return;
                    }
                    match &status.1 {
                        Ok(()) => this.painted = true,
                        Err(error) => {
                            this.painted = false;
                            this.dialog.error = Some(error.clone());
                        }
                    }
                    this.refresh(cx);
                    cx.notify();
                },
            ));
            Self {
                owner,
                document,
                revision,
                target,
                source,
                gpu: window.metal_context(),
                name,
                square,
                algorithm,
                inputs,
                dialog: FormDialogState::new(cx),
                validation: Ok(MeshGenerationOptions::Grid(defaults)),
                requested: None,
                candidate: None,
                task: None,
                request_id: 0,
                painter: None,
                painted: false,
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
            initial_focus: form.read(cx).inputs[0].read(cx).focus_handle(cx),
        };
        form_dialog::open(form, options, window, cx);
        Ok(())
    }

    fn validate(&self, cx: &App) -> anyhow::Result<MeshGenerationOptions> {
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("mesh_generation.stale")))?;
        let state = owner.read(cx);
        anyhow::ensure!(
            state.editor.active_id() == Some(self.document)
                && !state.modal
                && !state.editor.is_busy()
                && !state.editor.exiting(),
            t!("mesh_generation.stale")
        );
        let doc = state.editor.document().unwrap();
        anyhow::ensure!(
            doc.mesh_generation_revision() == self.revision && !doc.has_pending_edit(),
            t!("mesh_generation.stale")
        );
        anyhow::ensure!(
            !doc.object_state(&self.target).locked(),
            t!("mesh_generation.locked")
        );
        anyhow::ensure!(
            self.algorithm.read(cx).selected_value() == Some(&"grid"),
            t!("mesh_generation.unavailable")
        );
        let parse = |i: usize| {
            self.inputs[i]
                .read(cx)
                .value()
                .trim()
                .parse::<usize>()
                .map_err(|_| anyhow::anyhow!(t!("mesh_generation.invalid_counts")))
        };
        let options = MeshGridOptions {
            columns: parse(0)?,
            rows: parse(1)?,
        };
        options
            .validate()
            .map_err(|_| anyhow::anyhow!(t!("mesh_generation.invalid_counts")))?;
        anyhow::ensure!(
            !self.square || options.columns == options.rows,
            t!("mesh_generation.square_required")
        );
        doc.validate_mesh_grid(&self.target, options)?;
        Ok(MeshGenerationOptions::Grid(options))
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.validation = self.validate(cx);
        let options = self.validation.as_ref().ok().copied();
        if self.requested != options {
            self.request_id += 1;
            self.candidate = None;
            self.painter = None;
            self.painted = false;
            self.requested = options;
        }
        if self.task.is_none() && self.candidate.is_none() && self.dialog.error.is_none() {
            self.start_generation(cx);
        }
        self.dialog.set_validation(
            self.validation.is_ok() && self.candidate.is_some() && self.painted,
            true,
        );
        let content = self.candidate.as_ref().map(|c| PreviewContent {
            aspect: c.aspect,
            painter: self.painter.clone(),
            request: self.request_id,
            edges: c.edges.clone(),
        });
        let message = if self.validation.is_ok() && self.dialog.error.is_none() {
            t!("mesh_generation.preparing")
        } else {
            t!("mesh_generation.no_preview")
        };
        self.viewport.update(cx, |viewport, cx| {
            viewport.set_content(content, message.into(), cx)
        });
    }

    fn start_generation(&mut self, cx: &mut Context<Self>) {
        let Some(options) = self.requested else {
            return;
        };
        if self.dialog.closed() {
            return;
        }
        let request = self.request_id;
        let source = self.source.clone();
        let gpu = self.gpu.clone();
        let executor = cx.background_executor().clone();
        let timer = executor.timer(std::time::Duration::from_millis(100));
        let background = executor.spawn(async move {
            timer.await;
            source.generate(options).and_then(|plan| {
                objc2::rc::autoreleasepool(|_| Candidate::new(plan, source.texture.clone(), gpu))
            })
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = background.await;
            _ = this.update(cx, |this, cx| {
                this.task = None;
                if this.dialog.closed() {
                    return;
                }
                if this.request_id == request {
                    match result {
                        Ok(mut candidate) => {
                            this.dialog.edited();
                            this.painter = candidate.renderer.take().map(|mut renderer| {
                                Rc::new(RefCell::new(
                                    move |bounds, content, window: &mut Window| {
                                        renderer.paint(bounds, content, window)
                                    },
                                )) as PreviewPainter
                            });
                            this.candidate = Some(Arc::new(candidate));
                        }
                        Err(error) => this.dialog.error = Some(error.to_string()),
                    }
                }
                this.refresh(cx);
                cx.notify();
            });
        }));
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
    fn width(window: &Window) -> f32 {
        preview_side(window) + 288.
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
        anyhow::ensure!(self.painted, t!("mesh_generation.preparing"));
        let candidate = self
            .candidate
            .as_ref()
            .filter(|c| c.plan.options() == options)
            .ok_or_else(|| anyhow::anyhow!(t!("mesh_generation.preparing")))?;
        let plan = candidate.plan.clone();
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

fn preview_side(window: &Window) -> f32 {
    let size = window.viewport_size();
    (size.height.as_f32() - 192.)
        .min(size.width.as_f32() - 336.)
        .max(120.)
}

impl Render for MeshGenerationForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let side = preview_side(window);
        let mut settings = div()
            .id("mesh-generation-settings")
            .test_support()
            .flex()
            .flex_col()
            .gap_3()
            .w(px(240.))
            .flex_shrink_0()
            .child(
                div()
                    .text_sm()
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
                            Select::new(&self.algorithm)
                                .id("mesh-generation-algorithm")
                                .accessibility_label(t!("mesh_generation.algorithm"))
                                .small()
                                .w_full(),
                        ),
                    ),
            );
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
                Button::new(id)
                    .small()
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
            counts = counts.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .child(label.clone())
                    .child(
                        Input::new(&self.inputs[i])
                            .id(if i == 0 {
                                "mesh-generation-columns"
                            } else {
                                "mesh-generation-rows"
                            })
                            .small()
                            .aria_label(label),
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
        if let Some(candidate) = &self.candidate {
            let geometry = candidate.plan.geometry();
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
                    .w(px(side))
                    .h(px(side + 36.))
                    .flex_shrink_0()
                    .child(self.viewport.clone()),
            )
    }
}

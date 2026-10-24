//! Debounced generation, stale-request filtering and preview readiness.
use super::*;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

struct Candidate {
    plan: Arc<MeshGenerationPlan>,
    edges: Arc<[([f32; 2], [f32; 2])]>,
    source_size: [f32; 2],
    renderer: Option<crate::ui::mesh_preview::MeshPreview>,
}
impl Candidate {
    fn new(
        plan: MeshGenerationPlan,
        texture: Option<zipzi_editor_core::SceneTexture>,
        gpu: Option<SharedMetalContext>,
        preview_cache: crate::ui::mesh_preview::MeshPreviewCache,
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
                    .map(|p| [(p.x - min_x) / width, 1. - (p.y - min_y) / height]),
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
            .map(|scene| {
                crate::ui::mesh_preview::MeshPreview::prepare(
                    scene,
                    [width, height],
                    gpu,
                    preview_cache,
                )
            })
            .transpose()?;
        Ok(Self {
            renderer,
            plan: Arc::new(plan),
            edges: edges.into(),
            source_size: [width, height],
        })
    }
}

struct GeneratedPreview {
    candidate: Candidate,
    painter: Option<PreviewPainter>,
    painted: bool,
}
enum GenerationState {
    Empty,
    Preparing,
    Ready(GeneratedPreview),
    Failed,
}
pub(super) struct PreviewGeneration {
    pub(super) source: Arc<MeshGenerationSource>,
    gpu: Option<SharedMetalContext>,
    preview_cache: crate::ui::mesh_preview::MeshPreviewCache,
    latest_request: Arc<AtomicU64>,
    requested: Option<MeshGenerationOptions>,
    request_id: u64,
    task: Option<Task<()>>,
    state: GenerationState,
}
impl PreviewGeneration {
    pub(super) fn new(source: Arc<MeshGenerationSource>, gpu: Option<SharedMetalContext>) -> Self {
        Self {
            source,
            gpu,
            preview_cache: Default::default(),
            latest_request: Default::default(),
            requested: None,
            request_id: 0,
            task: None,
            state: GenerationState::Empty,
        }
    }
    fn request(&mut self, options: Option<MeshGenerationOptions>) {
        if self.requested != options {
            self.request_id += 1;
            self.latest_request
                .store(self.request_id, Ordering::Relaxed);
            self.requested = options;
            self.state = if options.is_some() {
                GenerationState::Preparing
            } else {
                GenerationState::Empty
            };
        }
    }
    fn ready(&mut self, mut candidate: Candidate) {
        let painter = candidate.renderer.take().map(|mut renderer| {
            Rc::new(RefCell::new(move |bounds, content, window: &mut Window| {
                renderer.paint(bounds, content, window)
            })) as PreviewPainter
        });
        self.state = GenerationState::Ready(GeneratedPreview {
            candidate,
            painter,
            painted: false,
        });
    }
    pub(super) fn accepts_paint(&self, request: u64) -> bool {
        request == self.request_id && matches!(self.state, GenerationState::Ready(_))
    }
    pub(super) fn set_painted(&mut self, painted: bool) {
        if let GenerationState::Ready(preview) = &mut self.state {
            preview.painted = painted;
        }
    }
    pub(super) fn painted(&self) -> bool {
        matches!(&self.state, GenerationState::Ready(preview) if preview.painted)
    }
    pub(super) fn plan(&self) -> Option<&Arc<MeshGenerationPlan>> {
        match &self.state {
            GenerationState::Ready(preview) => Some(&preview.candidate.plan),
            _ => None,
        }
    }
    fn content(&self) -> Option<PreviewContent> {
        match &self.state {
            GenerationState::Ready(preview) => Some(PreviewContent {
                source_size: preview.candidate.source_size,
                painter: preview.painter.clone(),
                request: self.request_id,
                edges: preview.candidate.edges.clone(),
            }),
            _ => None,
        }
    }
}

impl MeshGenerationForm {
    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.validation = self.validate(cx);
        let options = self.validation.as_ref().ok().copied();
        self.preview.request(options);
        if self.preview.task.is_none()
            && self.preview.plan().is_none()
            && self.dialog.error.is_none()
        {
            self.start_generation(cx);
        }
        self.dialog
            .set_validation(self.validation.is_ok() && self.preview.painted(), true);
        let content = self.preview.content();
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
        let Some(options) = self.preview.requested else {
            return;
        };
        if self.dialog.closed() {
            return;
        }
        self.preview.state = GenerationState::Preparing;
        let request = self.preview.request_id;
        let source = self.preview.source.clone();
        let gpu = self.preview.gpu.clone();
        let preview_cache = self.preview.preview_cache.clone();
        let latest_request = self.preview.latest_request.clone();
        let executor = cx.background_executor().clone();
        let timer = executor.timer(std::time::Duration::from_millis(100));
        let background = executor.spawn(async move {
            timer.await;
            // Coalesce changes during the debounce and skip GPU work for results
            // superseded while the CPU generator was running.
            if latest_request.load(Ordering::Relaxed) != request {
                return None;
            }
            let plan = source.generate(options);
            if latest_request.load(Ordering::Relaxed) != request {
                return None;
            }
            Some(plan.and_then(|plan| {
                objc2::rc::autoreleasepool(|_| {
                    Candidate::new(plan, source.texture.clone(), gpu, preview_cache)
                })
            }))
        });
        self.preview.task = Some(cx.spawn(async move |this, cx| {
            let result = background.await;
            _ = this.update(cx, |this, cx| {
                this.preview.task = None;
                if this.dialog.closed() {
                    return;
                }
                if this.preview.request_id == request {
                    match result.expect("Current generation must produce a result") {
                        Ok(candidate) => {
                            this.dialog.edited();
                            this.preview.ready(candidate);
                        }
                        Err(error) => {
                            this.preview.state = GenerationState::Failed;
                            this.dialog.error = Some(error.to_string());
                        }
                    }
                }
                this.refresh(cx);
                cx.notify();
            });
        }));
    }
}

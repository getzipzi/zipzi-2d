//! Glue painting, relationship labels, and coincident component picking.
use super::hints::*;
use super::{
    BrushSelectionSettings, FrameVersion, ToolController, brush_select::StrokeSamples,
    gesture::Press,
};
use crate::{
    CanvasFrame, CommandId, ComponentRef, Document, HitTarget, ObjectKind, ObjectRef, ViewportPoint,
};
use anyhow::{Result, ensure};

pub(super) const HINTS: &[InteractionHint] = &[
    InteractionHint::Drag(HintAction::GluePaint),
    InteractionHint::Shift(HintAction::GlueReverse),
    InteractionHint::Command(CommandId::SelectTool),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlueBrushMode {
    #[default]
    Ratio,
    FollowB,
    A,
    B,
    Reattach,
    Release,
}

#[cfg(test)]
mod tests {
    use super::*;
    use zipzi_core::{Vec2, evaluation::GlueOperation};

    #[test]
    fn glue_brush_modes_follow_evaluation_direction() {
        let evaluate = |[weight_a, weight_b]: [f32; 2], intensity: f32| {
            let (a, b) = GlueOperation {
                mesh_a: "a".into(),
                vertex_a: 0,
                mesh_b: "b".into(),
                vertex_b: 0,
                amount_a: intensity * weight_a,
                amount_b: intensity * weight_b,
            }
            .apply(Vec2::new(0., 0.), Vec2::new(10., 0.));
            [a.x, b.x]
        };
        let weights = [0.5, 0.5];
        assert_eq!(evaluate(weights, 1.), [5., 5.]);
        assert_eq!(evaluate(weights, 0.5), [2.5, 7.5]);
        assert_eq!(evaluate(weights, 0.), [0., 10.]);
        for (mode, reverse, result, positions) in [
            (GlueBrushMode::Ratio, false, [0., 1.], [0., 0.]),
            (GlueBrushMode::Ratio, true, [1., 0.], [10., 10.]),
            (GlueBrushMode::FollowB, false, [1., 0.], [10., 10.]),
            (GlueBrushMode::FollowB, true, [0., 1.], [0., 0.]),
            (GlueBrushMode::Release, false, [0., 0.], [0., 10.]),
            (GlueBrushMode::Release, true, [0.5, 0.5], [5., 5.]),
            (GlueBrushMode::A, false, [0., 0.5], [0., 5.]),
            (GlueBrushMode::B, false, [0.5, 0.], [5., 10.]),
        ] {
            assert_eq!(mode.paint(weights, 1., reverse), result);
            assert_eq!(evaluate(result, 1.), positions);
        }
        assert_eq!(GlueBrushMode::A.paint([0., 0.5], 1., true), weights);
        assert_eq!(GlueBrushMode::B.paint([0.5, 0.], 1., true), weights);
        let mode = GlueBrushMode::Reattach;
        assert_eq!(mode.paint([0.125, 0.375], 1., false), [0.25, 0.75]);
        assert_eq!(mode.paint([0., 0.], 1., false), weights);
        assert_eq!(mode.paint([0.25, 0.75], 0.5, true), [0.125, 0.375]);
        assert_eq!(
            GlueBrushMode::Release.paint([0.125, 0.375], 1., true),
            [0.25, 0.75]
        );
    }

    #[test]
    fn glue_pair_dabs_require_both_points_and_keep_distance_phase() {
        let p = |x| ViewportPoint { x, y: 0. };
        let settings = BrushSelectionSettings::default();
        let sweep = |points: &[f32], pair: [f32; 2]| {
            let spacing = settings.sample_spacing();
            let mut distance = spacing;
            let mut weights = [0.5, 0.5];
            for window in points.windows(2) {
                let sample =
                    StrokeSamples::between(p(window[0]), p(window[1]), distance, spacing).unwrap();
                let influence = sample.influence_at(settings, p(pair[0]), Some(p(pair[1])));
                weights = GlueBrushMode::Ratio.paint(weights, influence, false);
                distance = sample.next_distance();
            }
            weights
        };
        let sparse = sweep(&[-200., 200.], [-5., 5.]);
        let dense = sweep(&[-200., -65., -6., 0., 4., 23., 39., 100., 200.], [-5., 5.]);
        assert!((sparse[0] - dense[0]).abs() < 1e-6 && sparse[0] < 0.5);
        assert_eq!(sweep(&[-200., 200.], [-40., 40.]), [0.5, 0.5]);
    }

    #[test]
    fn glue_dabs_accept_imported_roundoff_in_all_modes_without_negative_weights() {
        for weights in [
            [0.5723187, 0.42768192],
            [0.8380362, 0.16196397],
            [0.6087419, 0.39125824],
            [1. + 4. * f32::EPSILON, 0.],
            [0., 1. + 4. * f32::EPSILON],
        ] {
            for mode in [
                GlueBrushMode::Ratio,
                GlueBrushMode::FollowB,
                GlueBrushMode::A,
                GlueBrushMode::B,
                GlueBrushMode::Reattach,
                GlueBrushMode::Release,
            ] {
                for reverse in [false, true] {
                    for influence in [0.001, 0.2, 1.] {
                        let [a, b] = mode.paint(weights, influence, reverse);
                        assert!(
                            a.is_finite() && b.is_finite() && a >= 0. && b >= 0. && a + b <= 1.,
                            "{mode:?} {weights:?} reverse={reverse} influence={influence}: {a}, {b}"
                        );
                    }
                }
            }
        }
    }
}

impl GlueBrushMode {
    fn paint(self, weights: [f32; 2], influence: f32, reverse: bool) -> [f32; 2] {
        // A tolerable imported rounding excess may also sit on an endpoint
        // (1 + epsilon, 0). Single-side dabs must never emit a negative peer.
        let [a, b] = weights.map(|weight| weight.clamp(0., 1.));
        let mix = |from: f32, to: f32| (to - from).mul_add(influence, from);
        match self {
            Self::FollowB => Self::Ratio.paint(weights, influence, !reverse),
            Self::Release => Self::Reattach.paint(weights, influence, !reverse),
            Self::A => [mix(a, if reverse { 1. - b } else { 0. }).min(1. - b), b],
            Self::B => [a, mix(b, if reverse { 1. - a } else { 0. }).min(1. - a)],
            Self::Ratio | Self::Reattach => {
                let sum = a + b;
                let target = match self {
                    Self::Ratio if reverse => [1., 0.],
                    Self::Ratio => [0., 1.],
                    Self::Reattach if reverse => [0., 0.],
                    Self::Reattach if sum > 0. => [a / sum, b / sum],
                    Self::Reattach => [0.5, 0.5],
                    _ => unreachable!(),
                };
                let a = mix(a, target[0]).clamp(0., 1.);
                [a, mix(b, target[1]).clamp(0., 1. - a)]
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OverlayGlueLabel {
    pub object: ObjectRef,
    pub name: String,
    pub position: ViewportPoint,
    pub selected: bool,
    pub locked: bool,
}

impl OverlayGlueLabel {
    pub const WIDTH: f32 = 130.;
    pub const HEIGHT: f32 = 20.;

    pub(super) fn hit(&self, p: ViewportPoint) -> bool {
        (self.position.x..=self.position.x + Self::WIDTH).contains(&p.x)
            && (self.position.y..=self.position.y + Self::HEIGHT).contains(&p.y)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OverlayGlueWeight {
    pub a: ViewportPoint,
    pub b: ViewportPoint,
    pub weights: [f32; 2],
}

/// A selected Glue's influence over one of its meshes in the presented pose.
/// The scalar is A's (red) influence; B's (green) influence is its complement.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayGlueWeightMesh {
    pub object: ObjectRef,
    pub triangles: Vec<([ViewportPoint; 3], [f32; 3])>,
}

#[derive(Clone, Debug)]
pub struct GluePaintControls {
    pub name: String,
    pub meshes: [String; 2],
    pub pair_count: usize,
    pub uniform_weights: Option<[f32; 2]>,
    pub diagnostic: Option<zipzi_core::Status>,
}

pub(super) fn active_glue(document: &Document) -> Option<&ObjectRef> {
    if document.context().selections.len() != 1 {
        return None;
    }
    document
        .context()
        .selection
        .as_ref()
        .filter(|o| o.kind == ObjectKind::Glue)
}

fn check_paint(
    document: &Document,
    model: &zipzi_core::state::DocumentState,
    frame: &CanvasFrame,
    object: &ObjectRef,
    glue: &zipzi_core::Glue,
) -> Result<(), zipzi_core::Status> {
    use zipzi_core::Status;
    let meshes =
        [&glue.mesh_a_id, &glue.mesh_b_id].map(|id| ObjectRef::new(ObjectKind::Mesh, id.as_str()));
    document
        .ensure_editable(&[object.clone(), meshes[0].clone(), meshes[1].clone()])
        .map_err(|error| Status::error("GLUE_LOCKED", error.to_string()))?;
    zipzi_sdk::validate_glue_weight_edit(model, glue)?;
    if !frame
        .geometry
        .control(&object.id)
        .is_some_and(|c| c.enabled && c.visible)
        || !frame.geometry.in_isolation_scope(object)
        || !meshes
            .iter()
            .all(|o| frame.geometry.in_isolation_scope(o) && frame.geometry.mesh_visible(&o.id))
    {
        return Err(Status::error(
            "GLUE_HIDDEN",
            "Glue and both meshes must be visible in this viewport",
        ));
    }
    if glue.pairs.is_empty() {
        return Err(Status::error("GLUE_EMPTY", "Glue has no bound vertices"));
    }
    Ok(())
}

struct PaintPair {
    id: String,
    points: [ViewportPoint; 2],
    weights: [f32; 2],
}

pub(super) struct GlueGesture {
    pub press: Press,
    version: zipzi_sdk::Version,
    object: ObjectRef,
    pairs: Vec<PaintPair>,
    previous: ViewportPoint,
    started: bool,
    next: f64,
    settings: BrushSelectionSettings,
    mode: GlueBrushMode,
}

impl GlueGesture {
    pub fn new(
        document: &Document,
        frame: &CanvasFrame,
        press: Press,
        settings: BrushSelectionSettings,
        mode: GlueBrushMode,
    ) -> Option<Self> {
        let version = document.session.observed_version();
        if version != document.session.version() {
            return None;
        }
        let object = active_glue(document)?.clone();
        let model = document.model();
        let glue = model.glue(&object.id)?;
        check_paint(document, &model, frame, &object, glue).ok()?;
        let pairs = glue
            .pairs
            .iter()
            .map(|pair| {
                Some(PaintPair {
                    id: pair.id.clone(),
                    points: [
                        endpoint(frame, glue.mesh_a_id.as_str(), pair.vertex_a?)?,
                        endpoint(frame, glue.mesh_b_id.as_str(), pair.vertex_b?)?,
                    ],
                    weights: [pair.weight_a, pair.weight_b],
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            previous: press.point,
            press,
            version,
            object,
            pairs,
            started: false,
            next: settings.sample_spacing(),
            settings,
            mode,
        })
    }

    pub fn accepts_frame(&self, version: FrameVersion) -> bool {
        version.binding == self.press.version.binding
            && version.viewport == self.press.version.viewport
            && version.scene >= self.press.version.scene
    }

    pub fn current(&self, document: &Document) -> bool {
        document.session.version() == self.version
    }

    pub fn update(&mut self, document: &mut Document) -> Result<bool> {
        ensure!(self.current(document), "Glue changed during the stroke");
        let settings = self.settings;
        let point = self.press.current;
        let segment =
            StrokeSamples::between(self.previous, point, self.next, settings.sample_spacing());
        if self.started && segment.is_none() {
            return Ok(false);
        }
        let mut updates = Vec::new();
        for pair in &mut self.pairs {
            let [a, b] = pair.points;
            let (p, other) = match self.mode {
                GlueBrushMode::A => (a, None),
                GlueBrushMode::B => (b, None),
                _ => (a, Some(b)),
            };
            let influence = if let Some(segment) = &segment {
                segment.influence_at(settings, p, other)
            } else {
                let influence = settings.influence(p, point, point);
                other.map_or(influence, |q| {
                    influence.min(settings.influence(q, point, point))
                })
            };
            if influence > 0. {
                let weights = self.mode.paint(pair.weights, influence, self.press.shift);
                if pair.weights != weights {
                    pair.weights = weights;
                    updates.push((pair.id.as_str(), weights));
                }
            }
        }
        self.started = true;
        self.previous = point;
        if let Some(segment) = &segment {
            self.next = segment.next_distance();
        }
        if updates.is_empty() {
            return Ok(false);
        }
        document.preview_glue_weights(self.version, &self.object.id, &updates)?;
        Ok(true)
    }
}

pub(super) fn endpoint(
    frame: &CanvasFrame,
    mesh: &str,
    id: zipzi_core::VertexId,
) -> Option<ViewportPoint> {
    let slot = *frame.geometry.topology.meshes.get(mesh)?.slots.get(&id)?;
    let &(vertex, position) = frame.geometry.mesh(mesh)?.vertices.get(slot)?;
    (vertex == id).then(|| frame.transform.to_viewport(position))
}

impl ToolController {
    pub(super) fn glue_weight_meshes(
        &self,
        glue: &zipzi_core::Glue,
        frame: &CanvasFrame,
    ) -> Vec<OverlayGlueWeightMesh> {
        // Follow the displayed mesh order so overlapping faces retain their
        // painter order. Influence is interpolated across authored triangles,
        // never between nearby disconnected vertices or unrelated meshes.
        frame
            .geometry
            .meshes
            .iter()
            .filter_map(|mesh| {
                let side_a = mesh.id == glue.mesh_a_id.as_str();
                if !side_a && mesh.id != glue.mesh_b_id.as_str() {
                    return None;
                }
                let topology = frame.geometry.topology.meshes.get(&mesh.id)?;
                let mut influence = vec![if side_a { 1. } else { 0. }; mesh.vertices.len()];
                for pair in &glue.pairs {
                    let (vertex, red) = if side_a {
                        (pair.vertex_a, 1. - pair.weight_a)
                    } else {
                        (pair.vertex_b, pair.weight_b)
                    };
                    if let Some(slot) = vertex.and_then(|id| topology.slots.get(&id))
                        && red.is_finite()
                    {
                        influence[*slot] = red.clamp(0., 1.);
                    }
                }
                Some(OverlayGlueWeightMesh {
                    object: ObjectRef::new(ObjectKind::Mesh, &mesh.id),
                    triangles: mesh
                        .triangles
                        .iter()
                        .map(|ids| {
                            let slots = ids.map(|id| topology.slots[&id]);
                            (
                                slots
                                    .map(|slot| frame.transform.to_viewport(mesh.vertices[slot].1)),
                                slots.map(|slot| influence[slot]),
                            )
                        })
                        .collect(),
                })
            })
            .collect()
    }

    pub(crate) fn glue_paint_controls(
        &self,
        document: &Document,
        frame: Option<&CanvasFrame>,
    ) -> Option<GluePaintControls> {
        let object = active_glue(document)?;
        let model = document.model();
        let glue = model.glue(&object.id)?;
        let meshes = [&glue.mesh_a_id, &glue.mesh_b_id].map(|id| {
            model
                .mesh(id.as_str())
                .map_or_else(|| "—".into(), |m| m.name.clone())
        });
        Some(GluePaintControls {
            name: glue.name.clone(),
            meshes,
            pair_count: glue.pairs.len(),
            uniform_weights: glue.pairs.first().and_then(|first| {
                let weights = [first.weight_a, first.weight_b];
                glue.pairs
                    .iter()
                    .all(|p| [p.weight_a, p.weight_b] == weights)
                    .then_some(weights)
            }),
            diagnostic: frame
                .and_then(|frame| check_paint(document, &model, frame, object, glue).err()),
        })
    }

    pub(super) fn glue_labels(
        &self,
        document: &Document,
        frame: &CanvasFrame,
    ) -> Vec<OverlayGlueLabel> {
        let selection = &document.context().selections;
        let scene = &frame.geometry;
        let locks = document.object_state_index();
        let mut labels: Vec<OverlayGlueLabel> = Vec::new();
        for glue in scene.topology.glues.iter() {
            let Some(control) = scene.control(&glue.object.id) else {
                continue;
            };
            if !scene.control_visible(
                control,
                selection,
                self.active,
                self.deformer_visibility_for(frame),
            ) {
                continue;
            }
            let mesh_a = ObjectRef::new(ObjectKind::Mesh, &glue.mesh_a);
            let mesh_b = ObjectRef::new(ObjectKind::Mesh, &glue.mesh_b);
            if [&glue.object, &mesh_a, &mesh_b]
                .iter()
                .any(|o| !scene.in_isolation_scope(o))
                || !scene.mesh_visible(&glue.mesh_a)
                || !scene.mesh_visible(&glue.mesh_b)
            {
                continue;
            }
            let mut bounds = [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ];
            let mut include = |position| {
                let p = frame.transform.to_viewport(position);
                bounds[0] = bounds[0].min(p.x);
                bounds[1] = bounds[1].min(p.y);
                bounds[2] = bounds[2].max(p.x);
                bounds[3] = bounds[3].max(p.y);
            };
            if control.points.is_empty() {
                for mesh in [&glue.mesh_a, &glue.mesh_b]
                    .into_iter()
                    .filter_map(|id| scene.mesh(id))
                {
                    mesh.vertices.iter().for_each(|&(_, p)| include(p));
                }
            } else {
                control.points.iter().copied().for_each(&mut include);
            }
            let mut position = ViewportPoint {
                x: (bounds[0] + (bounds[2] - bounds[0]) / 2. - OverlayGlueLabel::WIDTH / 2.).clamp(
                    0.,
                    (frame.transform.metrics.width - OverlayGlueLabel::WIDTH).max(0.),
                ),
                y: (bounds[1] - OverlayGlueLabel::HEIGHT - 8.).max(32.),
            };
            if !bounds[0].is_finite()
                || bounds[2] < 0.
                || bounds[0] > frame.transform.metrics.width
                || bounds[3] < 0.
                || bounds[1] > frame.transform.metrics.height
            {
                continue;
            }
            for label in &labels {
                if (label.position.x - position.x).abs() < OverlayGlueLabel::WIDTH
                    && (label.position.y - position.y).abs() < OverlayGlueLabel::HEIGHT + 2.
                {
                    position.y = label.position.y + OverlayGlueLabel::HEIGHT + 2.;
                }
            }
            labels.push(OverlayGlueLabel {
                name: scene.index.name(&glue.object).to_owned(),
                object: glue.object.clone(),
                position,
                selected: selection.contains(&glue.object),
                locked: [&glue.object, &mesh_a, &mesh_b]
                    .iter()
                    .any(|o| locks.is_locked(o)),
            });
        }
        labels
    }

    pub(super) fn pick_glue_label(
        &self,
        document: &Document,
        frame: &CanvasFrame,
        point: ViewportPoint,
    ) -> Option<ObjectRef> {
        self.glue_labels(document, frame)
            .into_iter()
            .rev()
            .find(|label| label.hit(point))
            .map(|label| label.object)
    }
}

impl super::ToolController {
    /// A single visible handle must hit the editable selected source at an
    /// overlapping seam. It never redirects writes into an unselected mesh.
    pub(crate) fn canvas_hit(
        &self,
        document: &Document,
        frame: &CanvasFrame,
        point: ViewportPoint,
    ) -> Option<HitTarget> {
        if self.active == super::ToolId::BrushSelect {
            return None;
        }
        let scene = &frame.geometry;
        let selection: Vec<_> = document
            .context()
            .selections
            .iter()
            .filter(|r| scene.in_isolation_scope(r))
            .cloned()
            .collect();
        let locks = document.object_state_index();
        let hit = scene.hit_with_state(
            point,
            frame.transform,
            &selection,
            self.active,
            self.deformer_visibility_for(frame),
            &locks,
            self.warp_selection_gizmo(document, frame).as_ref(),
        );
        let Some(HitTarget::Component(component)) = &hit else {
            return hit;
        };
        if component.object.kind != ObjectKind::Mesh {
            return hit;
        }
        let position = scene
            .mesh(&component.object.id)?
            .vertices
            .iter()
            .find(|(id, _)| *id == component.id)?
            .1;
        let position = frame.transform.to_viewport(position);
        let mut candidates = Vec::new();
        for g in scene.topology.glues.iter() {
            if !scene.control(&g.object.id).is_some_and(|c| {
                scene.control_visible(
                    c,
                    &selection,
                    self.active,
                    self.deformer_visibility_for(frame),
                )
            }) {
                continue;
            }
            for &(a, b) in &g.pairs {
                for (mesh, slot) in [(&g.mesh_a, a), (&g.mesh_b, b)] {
                    let object = ObjectRef::new(ObjectKind::Mesh, mesh);
                    if !selection.contains(&object) || locks.is_locked(&object) {
                        continue;
                    }
                    let Some((id, p)) = scene.mesh(mesh).and_then(|m| m.vertices.get(slot)) else {
                        continue;
                    };
                    let p = frame.transform.to_viewport(*p);
                    let candidate = ComponentRef { object, id: *id };
                    if (p.x - position.x).hypot(p.y - position.y) < 0.5
                        && !candidates.contains(&candidate)
                    {
                        candidates.push(candidate);
                    }
                }
            }
        }
        if candidates.len() < 2 || !candidates.contains(component) {
            return hit;
        }
        let access = self.edit_access(document, scene, &selection);
        if matches!(access.probe(component), Some(Ok(()))) {
            return hit;
        }
        candidates.sort_by(|a, b| a.object.id.cmp(&b.object.id).then(a.id.cmp(&b.id)));
        candidates
            .into_iter()
            .find(|c| matches!(access.probe(c), Some(Ok(()))))
            .map(HitTarget::Component)
            .or(hit)
    }
}

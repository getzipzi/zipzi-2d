//! Independent canvas baselines. Native Warp evaluation first maps its stored
//! lattice through its parent, so a captured canvas lattice retains that Warp's
//! complete runtime interpolation, including below nonlinear parents.
use super::*;
use crate::deformers::{rotation_parent_angle, warp_points, PsmVec2};
use crate::evaluation::PreviewValues;
use crate::{
    DeformerOriginal, DeformerOriginalShape, DeformerOriginalSnapshot, DeformerOriginalSource,
    PreciseVec2, SceneTrack, TransformData, Vec2, WarpTransform,
};

pub(super) struct OriginalParentSpace {
    origin: Vec2,
    basis: [Vec2; 2],
    warp: Option<WarpTransform>,
    scale: f32,
    identity: bool,
    flat: Vec<f32>,
    affine: bool,
}
impl OriginalParentSpace {
    pub(super) fn new(
        doc: &Document,
        parent: &str,
        context: &PreviewValues,
    ) -> Result<Self, Status> {
        if parent.is_empty() {
            return Ok(Self {
                origin: Vec2::default(),
                basis: [Vec2::new(1., 0.), Vec2::new(0., 1.)],
                warp: None,
                scale: 1.,
                identity: true,
                flat: Vec::new(),
                affine: true,
            });
        }
        let mut frame = crate::DrawableFrame::default();
        require_ok(crate::evaluation::evaluate_frame_including_hidden(
            doc, context, &mut frame,
        ))?;
        let control = frame
            .deformers
            .iter()
            .find(|d| d.id == parent)
            .ok_or_else(|| Status::error("MISSING_TRANSFORM", parent))?;
        let transform = doc.get_transform(parent).unwrap();
        let origin = control.points[0];
        let (basis, warp) = if let Some(warp) = transform.warp() {
            let mut warp = warp.clone();
            warp.points = control.points.clone();
            warp.bezier = None;
            let x = warp.points[warp.columns as usize];
            let y = warp.points[(warp.rows * (warp.columns + 1)) as usize];
            (
                [
                    Vec2::new(x.x - origin.x, x.y - origin.y),
                    Vec2::new(y.x - origin.x, y.y - origin.y),
                ],
                Some(warp),
            )
        } else {
            (control.rotation_basis.unwrap(), None)
        };
        let mut rotation = parent;
        while !rotation.is_empty() && doc.get_transform(rotation).unwrap().rotation().is_none() {
            rotation = doc.get_transform(rotation).unwrap().parent();
        }
        let scale = if rotation.is_empty() {
            1.
        } else {
            let [x, _] = frame
                .deformers
                .iter()
                .find(|d| d.id == rotation)
                .unwrap()
                .rotation_basis
                .unwrap();
            x.x.hypot(x.y)
        };
        let flat = warp.as_ref().map_or_else(Vec::new, |w| {
            w.points.iter().flat_map(|p| [p.x, p.y]).collect()
        });
        let mut space = Self {
            origin,
            basis,
            warp,
            scale,
            identity: false,
            flat,
            affine: true,
        };
        space.affine = space.test_affine();
        Ok(space)
    }
    pub(super) fn point(&self, p: Vec2) -> Vec2 {
        if let Some(w) = &self.warp {
            let mut output = [0.; 2];
            warp_points(
                w.rows as i32,
                w.columns as i32,
                w.quad,
                &self.flat,
                &[p.x, p.y],
                &mut output,
                1,
            );
            Vec2::new(output[0], output[1])
        } else {
            Vec2::new(
                self.basis[0].x * p.x + self.basis[1].x * p.y + self.origin.x,
                self.basis[0].y * p.x + self.basis[1].y * p.y + self.origin.y,
            )
        }
    }
    fn affine(&self) -> bool {
        self.affine
    }
    fn test_affine(&self) -> bool {
        self.warp.as_ref().is_none_or(|w| {
            let magnitude = w
                .points
                .iter()
                .fold(1f32, |v, p| v.max(p.x.abs()).max(p.y.abs()));
            w.points.iter().enumerate().all(|(i, p)| {
                let u = (i % (w.columns as usize + 1)) as f32 / w.columns as f32;
                let v = (i / (w.columns as usize + 1)) as f32 / w.rows as f32;
                let x = self.origin.x + self.basis[0].x * u + self.basis[1].x * v;
                let y = self.origin.y + self.basis[0].y * u + self.basis[1].y * v;
                (p.x - x).abs().max((p.y - y).abs()) <= (8. * f32::EPSILON * magnitude).min(0.0001)
            })
        })
    }
    fn inverse(&self, p: PreciseVec2) -> Result<PreciseVec2, Status> {
        if self.identity {
            return Ok(p);
        }
        if !self.affine() {
            return Err(Status::error(
                "ORIGINAL_PARENT_NONLINEAR",
                "Restoring through a nonlinear parent requires a proven unique inverse",
            ));
        }
        let [x, y] = self.basis;
        let (a, b, c, d) = (
            f64::from(x.x),
            f64::from(y.x),
            f64::from(x.y),
            f64::from(y.y),
        );
        let det = a * d - b * c;
        if !det.is_finite() || det == 0. {
            return Err(Status::error(
                "SINGULAR_PARENT",
                "The current parent has no unique inverse",
            ));
        }
        let (x, y) = (
            p.x - f64::from(self.origin.x),
            p.y - f64::from(self.origin.y),
        );
        let result = PreciseVec2::new((d * x - b * y) / det, (a * y - c * x) / det);
        let check = self.point(Vec2::new(result.x as f32, result.y as f32));
        let error = (f64::from(check.x) - p.x).hypot(f64::from(check.y) - p.y);
        if !error.is_finite() || error > 0.01 {
            return Err(Status::error(
                "ORIGINAL_RESTORE_TOLERANCE",
                format!("Inverse error {error} exceeds 0.01 canvas units"),
            ));
        }
        Ok(result)
    }
    fn direction(&self, origin: Vec2) -> f32 {
        if self.identity {
            return 0.;
        }
        let mut point = if self.warp.is_none() {
            PsmVec2::default()
        } else {
            PsmVec2::new(origin.x, origin.y)
        };
        rotation_parent_angle(
            self.warp.is_none(),
            |p| {
                let p = if self.warp.is_some() {
                    self.point(Vec2::new(p.x, p.y))
                } else {
                    Vec2::new(
                        self.basis[0].x * p.x + self.basis[1].x * p.y,
                        self.basis[0].y * p.x + self.basis[1].y * p.y,
                    )
                };
                PsmVec2::new(p.x, p.y)
            },
            &mut point,
        )
    }
}

fn require_ok(status: Status) -> Result<(), Status> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(status)
    }
}
impl Document {
    /// Set frozen inputs for automatic creation/first-key baselines in an owned
    /// authoring candidate or planning snapshot. This is not persistent content.
    pub fn set_original_capture_context(&mut self, context: PreviewValues) {
        self.original_capture_context = context;
    }
    pub(super) fn capture_deformer_original(
        &self,
        t: &Transform,
        data: &TransformData,
        source: DeformerOriginalSource,
        context: &PreviewValues,
    ) -> Result<DeformerOriginalSnapshot, Status> {
        if source == DeformerOriginalSource::FirstKey
            && self.iter_blend_bindings_for_target(&t.id).next().is_some()
        {
            let mut frame = crate::DrawableFrame::default();
            require_ok(crate::evaluation::evaluate_frame_including_hidden(
                self, context, &mut frame,
            ))?;
            let values = frame
                .parameters
                .iter()
                .map(|p| (p.id.clone(), p.value))
                .collect();
            if self
                .iter_blend_bindings_for_target(&t.id)
                .any(|b| !crate::evaluation::evaluate_blend_binding(self, &values, b).is_empty())
            {
                return Err(Status::error(
                    "ORIGINAL_BLEND_UNSUPPORTED",
                    "Automatic original capture is unavailable while deformer BlendShapes contribute",
                ));
            }
        }
        let parent = OriginalParentSpace::new(self, t.parent(), context)?;
        let shape = match data {
            TransformData::Warp(warp) => {
                let mut warp = warp.clone();
                for p in &mut warp.points {
                    *p = parent.point(*p);
                }
                if let Some(controller) = &mut warp.bezier {
                    for n in &mut controller.nodes {
                        n.position = parent.point(n.position);
                        for h in &mut n.handles {
                            *h = parent.point(*h);
                        }
                    }
                }
                DeformerOriginalShape::Warp {
                    warp,
                    controller_canvas_exact: parent.affine(),
                }
            }
            TransformData::Rotation(rotation) => {
                let mut pose = rotation.pose;
                let local = Vec2::new(pose.origin.x as f32, pose.origin.y as f32);
                if !parent.identity {
                    let p = parent.point(local);
                    pose.origin = PreciseVec2::new(f64::from(p.x), f64::from(p.y));
                }
                pose.angle = rotation.base_angle + (pose.angle + parent.direction(local));
                pose.scale *= parent.scale;
                DeformerOriginalShape::Rotation { pose }
            }
        };
        let snapshot = DeformerOriginalSnapshot { source, shape };
        self.validate_deformer_original(&DeformerOriginal {
            snapshot: Some(snapshot.clone()),
            ..Default::default()
        })?;
        Ok(snapshot)
    }
    pub(super) fn initialize_deformer_original(&mut self, id: &str) {
        let Some(transform) = self
            .get_transform(id)
            .filter(|t| t.original_shape.is_none())
            .cloned()
        else {
            return;
        };
        let mut record = DeformerOriginal::default();
        match self.capture_deformer_original(
            &transform,
            &transform.data,
            DeformerOriginalSource::Created,
            &self.original_capture_context,
        ) {
            Ok(snapshot) => record.snapshot = Some(snapshot),
            Err(error) => record.capture_error = Some(error.code),
        }
        self.content.transforms.get_mut(id).unwrap().original_shape = Some(record);
    }
    pub(super) fn update_first_key_originals(&mut self, ids: &[String]) {
        let targets: std::collections::HashSet<_> = ids
            .iter()
            .filter_map(|id| {
                self.get_scene_binding(id)
                    .map(|b| b.target_id().to_owned())
                    .or_else(|| self.get_transform(id).map(|_| id.clone()))
            })
            .filter(|id| {
                self.binding_for_scene(id).is_some_and(|b| {
                    !b.track.is_empty() && b.axes.iter().any(|a| !a.parameter_id.is_empty())
                }) && self
                    .get_transform(id)
                    .is_some_and(|t| t.original_shape.as_ref().is_some_and(|r| !r.first_key_seen))
            })
            .collect();
        for id in targets {
            let transform = self.get_transform(&id).unwrap().clone();
            let mut record = transform.original_shape.clone().unwrap_or_default();
            record.first_key_seen = true;
            if record.auto_update {
                match self.capture_deformer_original(
                    &transform,
                    &transform.data,
                    DeformerOriginalSource::FirstKey,
                    &self.original_capture_context,
                ) {
                    Ok(snapshot) => {
                        record.snapshot = Some(snapshot);
                        record.capture_error = None;
                    }
                    Err(error) => record.capture_error = Some(error.code),
                }
            }
            self.content.transforms.get_mut(&id).unwrap().original_shape = Some(record);
        }
    }
    pub(super) fn original_shape_data(
        &self,
        t: &Transform,
        target: &ShapeTarget,
    ) -> Result<TransformData, Status> {
        match target {
            ShapeTarget::Base => Ok(t.data.clone()),
            ShapeTarget::Ordinary {
                binding_id,
                samples,
            } => {
                let binding = self
                    .get_scene_binding(binding_id)
                    .filter(|b| b.target_id() == t.id)
                    .ok_or_else(|| Status::error("WRONG_SHAPE_TARGET", binding_id))?;
                self.authoring_unlocked(binding_id)?;
                match &binding.track {
                    SceneTrack::Warp { keyforms, .. } => {
                        let form = keyforms
                            .iter()
                            .find(|f| same_samples(&f.samples, samples))
                            .ok_or_else(|| Status::error("MISSING_KEYFORM", binding_id))?;
                        let mut warp = t
                            .warp()
                            .ok_or_else(|| Status::error("NOT_WARP", &t.id))?
                            .clone();
                        warp.points = form.positions.clone();
                        warp.bezier = form.bezier.clone();
                        Ok(TransformData::Warp(warp))
                    }
                    SceneTrack::Rotation { keyforms, .. } => {
                        let form = keyforms
                            .iter()
                            .find(|f| same_samples(&f.samples, samples))
                            .ok_or_else(|| Status::error("MISSING_KEYFORM", binding_id))?;
                        let mut rotation = t
                            .rotation()
                            .ok_or_else(|| Status::error("NOT_ROTATION", &t.id))?
                            .clone();
                        rotation.pose = form.rotation;
                        Ok(TransformData::Rotation(rotation))
                    }
                    _ => Err(Status::error("WRONG_SHAPE_TARGET", binding_id)),
                }
            }
            ShapeTarget::Blend { .. } => Err(Status::error(
                "ORIGINAL_BLEND_UNSUPPORTED",
                "Original updates and restoration require Base or an ordinary stored shape",
            )),
        }
    }
    pub(super) fn restored_original_data(
        &self,
        t: &Transform,
        context: &PreviewValues,
    ) -> Result<TransformData, Status> {
        let snapshot = t
            .original_shape
            .as_ref()
            .and_then(|r| r.snapshot.as_ref())
            .ok_or_else(|| {
                Status::error(
                    "MISSING_DEFORMER_ORIGINAL",
                    "Record an original shape first",
                )
            })?;
        let parent = OriginalParentSpace::new(self, t.parent(), context)?;
        match (&snapshot.shape, &t.data) {
            (
                DeformerOriginalShape::Warp {
                    warp: saved,
                    controller_canvas_exact,
                },
                TransformData::Warp(_),
            ) => {
                if saved.bezier.is_some() && !controller_canvas_exact {
                    return Err(Status::error(
                        "ORIGINAL_CONTROLLER_NONLINEAR",
                        "The recorded controller passed through a nonlinear parent and cannot be restored as equivalent cubics",
                    ));
                }
                let mut warp = saved.clone();
                let convert = |p: &mut Vec2| -> Result<(), Status> {
                    let local = parent.inverse(PreciseVec2::new(f64::from(p.x), f64::from(p.y)))?;
                    *p = Vec2::new(local.x as f32, local.y as f32);
                    Ok(())
                };
                for p in &mut warp.points {
                    convert(p)?;
                }
                if let Some(controller) = &mut warp.bezier {
                    for n in &mut controller.nodes {
                        convert(&mut n.position)?;
                        for h in &mut n.handles {
                            convert(h)?;
                        }
                    }
                }
                if let (Some(local), Some(canvas)) = (&warp.bezier, &saved.bezier) {
                    use crate::WarpBezierEvaluation;
                    for y in 0..17 {
                        for x in 0..17 {
                            let (u, v) = ((x as f32 + 0.5) / 17., (y as f32 + 0.5) / 17.);
                            let a = parent.point(local.sample(u, v));
                            let b = canvas.sample(u, v);
                            let error = (f64::from(a.x) - f64::from(b.x))
                                .hypot(f64::from(a.y) - f64::from(b.y));
                            if !error.is_finite() || error > 0.01 {
                                return Err(Status::error(
                                    "ORIGINAL_CONTROLLER_TOLERANCE",
                                    "The restored controller exceeds 0.01 canvas units at independent curve samples",
                                ));
                            }
                        }
                    }
                }
                Ok(TransformData::Warp(warp))
            }
            (DeformerOriginalShape::Rotation { pose }, TransformData::Rotation(rotation)) => {
                let mut pose = *pose;
                pose.origin = parent.inverse(pose.origin)?;
                if parent.scale == 0. {
                    return Err(Status::error(
                        "SINGULAR_PARENT",
                        "The current inherited scale has no unique inverse",
                    ));
                }
                pose.angle -= rotation.base_angle
                    + parent.direction(Vec2::new(pose.origin.x as f32, pose.origin.y as f32));
                pose.scale /= parent.scale;
                let mut rotation = rotation.clone();
                rotation.pose = pose;
                Ok(TransformData::Rotation(rotation))
            }
            _ => Err(Status::error(
                "ORIGINAL_DEFORMER_TYPE_CHANGED",
                "The saved original has a different deformer type",
            )),
        }
    }
    pub(super) fn validate_deformer_original(
        &self,
        record: &DeformerOriginal,
    ) -> Result<(), Status> {
        if let Some(snapshot) = &record.snapshot {
            match &snapshot.shape {
                DeformerOriginalShape::Warp { warp, .. } => {
                    if warp.rows == 0
                        || warp.columns == 0
                        || warp.rows > 1024
                        || warp.columns > 1024
                        || warp.points.len() != ((warp.rows + 1) * (warp.columns + 1)) as usize
                    {
                        return Err(Status::error(
                            "INVALID_DEFORMER_ORIGINAL",
                            "Invalid saved lattice",
                        ));
                    }
                    require_ok(crate::geometry::validate_positions(&warp.points))?;
                    if let Some(controller) = &warp.bezier {
                        require_ok(controller.validate())?;
                    }
                }
                DeformerOriginalShape::Rotation { pose } => {
                    require_ok(pose_valid(pose, "original shape"))?
                }
            }
        }
        Ok(())
    }
}
fn same_samples(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().all(|id| b.contains(id))
}

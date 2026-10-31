use super::*;
use crate::{DeformerOriginalAction as Action, DeformerOriginalSource, SceneTrack, TransformData};

impl Document {
    pub(super) fn plan_deformer_original(
        &self,
        plan: &mut SceneContentPlan,
        id: &str,
        target: &ShapeTarget,
        action: Action,
        context: &crate::evaluation::PreviewValues,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        self.content_unlocked(id)?;
        let transform = self
            .get_transform(id)
            .ok_or_else(|| Status::error("MISSING_TRANSFORM", id))?;
        let mut record =
            transform
                .original_shape
                .clone()
                .unwrap_or_else(|| crate::DeformerOriginal {
                    first_key_seen: self.binding_for_scene(id).is_some_and(|b| {
                        !b.track.is_empty() && b.axes.iter().any(|a| !a.parameter_id.is_empty())
                    }),
                    ..Default::default()
                });
        if let Action::AutoUpdate { enabled } = action {
            record.auto_update = enabled;
            let mut t = transform.clone();
            t.original_shape = Some(record);
            plan.domain.delta.transforms.push(t);
            plan.domain.affected.push(id.into());
            plan.kind = ChangeKind::Metadata;
            return Ok(());
        }
        for (id, value) in context {
            if self.get_parameter(id).is_none() || !value.is_finite() {
                return Err(Status::error("INVALID_ORIGINAL_CONTEXT", id));
            }
        }
        let mut values = self
            .parameter_order()
            .iter()
            .map(|id| {
                let p = self.get_parameter(id).unwrap();
                (
                    id.clone(),
                    context.get(id).copied().unwrap_or(p.default_value),
                )
            })
            .collect();
        if self.iter_blend_bindings_for_target(id).next().is_some() {
            let mut frame = crate::DrawableFrame::default();
            check(crate::evaluation::evaluate_frame_including_hidden(
                self, context, &mut frame,
            ))?;
            values = frame
                .parameters
                .iter()
                .map(|p| (p.id.clone(), p.value))
                .collect();
        }
        if self
            .iter_blend_bindings_for_target(id)
            .any(|b| !crate::evaluation::evaluate_blend_binding(self, &values, b).is_empty())
        {
            return Err(Status::error(
                "ORIGINAL_BLEND_UNSUPPORTED",
                "Return deformer BlendShapes to their base values before updating or restoring an original",
            ));
        }
        let data = self.original_shape_data(transform, target)?;
        if action == Action::Update {
            record.snapshot = Some(self.capture_deformer_original(
                transform,
                &data,
                DeformerOriginalSource::Manual,
                context,
            )?);
            record.auto_update = false;
            record.capture_error = None;
            let mut t = transform.clone();
            t.original_shape = Some(record);
            plan.domain.delta.transforms.push(t);
            plan.domain.affected.push(id.into());
            plan.kind = ChangeKind::Metadata;
            return Ok(());
        }
        let Action::Restore { allow_resize } = action else {
            unreachable!()
        };
        let restored = self.restored_original_data(transform, context)?;
        let mut candidate = self.clone();
        if let TransformData::Warp(saved) = &restored {
            let current = transform.warp().unwrap();
            if (saved.rows, saved.columns, saved.quad)
                != (current.rows, current.columns, current.quad)
            {
                if !allow_resize {
                    return Err(Status::error(
                        "ORIGINAL_RESTORE_REQUIRES_RESIZE",
                        "Restoration resamples every stored Warp shape; review and explicitly allow density/interpolation conversion",
                    ));
                }
                let resize = candidate.plan_scene_content_with_control(
                    SceneContentEdit::ResizeWarp {
                        transform_id: id.into(),
                        rows: saved.rows,
                        columns: saved.columns,
                        quad: saved.quad,
                    },
                    limits,
                    control,
                )?;
                plan.report.resampled_shapes += resize.report.resampled_shapes;
                plan.report.losses.extend(resize.report.losses.clone());
                check(candidate.apply_scene_content_plan(&resize).status)?;
            }
            if let Some(controller) = &saved.bezier {
                let grid = crate::WarpBezierGrid {
                    rows: controller.rows,
                    columns: controller.columns,
                };
                let mut settings = transform.warp_authoring.clone().unwrap_or_default();
                if settings.bezier_grid.is_some_and(|g| g != grid) {
                    if !allow_resize {
                        return Err(Status::error(
                            "ORIGINAL_RESTORE_REQUIRES_RESIZE",
                            "Restoration refits every stored controller to the original density; explicit permission is required",
                        ));
                    }
                    settings.bezier_grid = Some(grid);
                    let resize = candidate.plan_scene_content_with_control(
                        SceneContentEdit::WarpAuthoring {
                            transform_id: id.into(),
                            settings,
                        },
                        limits,
                        control,
                    )?;
                    plan.report.resampled_shapes += resize.report.resampled_shapes;
                    check(candidate.apply_scene_content_plan(&resize).status)?;
                    plan.report.losses.push("All existing controllers were refitted to the original controller density; baked lattices remain unchanged by that refit".into());
                }
            }
        }
        match target {
            ShapeTarget::Base => candidate.content.transforms.get_mut(id).unwrap().data = restored,
            ShapeTarget::Ordinary {
                binding_id,
                samples,
            } => {
                let binding = candidate
                    .content
                    .scene_bindings
                    .get_mut(binding_id)
                    .unwrap();
                match (&mut binding.track, restored) {
                    (SceneTrack::Warp { keyforms, .. }, TransformData::Warp(w)) => {
                        let form = keyforms
                            .iter_mut()
                            .find(|f| same_samples(&f.samples, samples))
                            .unwrap();
                        form.positions = w.points;
                        form.bezier = w.bezier;
                    }
                    (SceneTrack::Rotation { keyforms, .. }, TransformData::Rotation(r)) => {
                        let form = keyforms
                            .iter_mut()
                            .find(|f| same_samples(&f.samples, samples))
                            .unwrap();
                        form.rotation = r.pose;
                    }
                    _ => return Err(Status::error("WRONG_SHAPE_TARGET", binding_id)),
                }
            }
            ShapeTarget::Blend { .. } => unreachable!(),
        }
        let t = candidate.get_transform(id).unwrap().clone();
        plan.domain.delta.transforms.push(t);
        plan.domain.affected.push(id.into());
        if let Some(binding) = candidate.binding_for_scene(id) {
            if self.get_scene_binding(&binding.id) != Some(binding) {
                plan.domain.delta.scene_bindings.push(binding.clone());
                plan.domain.affected.push(binding.id.clone());
            }
        }
        for binding in candidate.iter_blend_bindings_for_target(id) {
            if self.get_blend_binding(&binding.id) != Some(binding) {
                plan.domain.delta.blends.push(binding.clone());
                plan.domain.affected.push(binding.id.clone());
            }
        }
        Ok(())
    }
}
fn same_samples(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().all(|id| b.contains(id))
}

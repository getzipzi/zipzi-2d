//! Native content edits on retained owners. These run only in the private
//! candidate; native converters retain sample/key interpolation semantics.
use super::*;
impl Document {
    pub(super) fn install_leaf_deletion(
        &self,
        candidate: &mut Document,
        request: &DeletionRequest,
        report: &mut DeletionReport,
        limits: AuthoringLimits,
        control: &ConversionControl,
        cost: &mut ConversionCost,
    ) -> Result<bool, Status> {
        let removed: BTreeSet<_> = report.removed.iter().map(|r| r.object.clone()).collect();
        let index = self.deletion_index(control)?;
        let mut affected = BTreeSet::new();
        for key in removed.iter().filter(|k| k.kind == DeletionKind::Sample) {
            control.checkpoint()?;
            let child = self
                .binding_identity(&key.id)
                .ok_or_else(|| Status::error("MISSING_SAMPLE", &key.id))?;
            let owner = if self.get_binding(&child.owner).is_some() {
                DeletionKind::MeshBinding
            } else if self.get_scene_binding(&child.owner).is_some() {
                DeletionKind::SceneBinding
            } else if self.get_glue(&child.owner).is_some() {
                DeletionKind::GlueBinding
            } else if self.get_blend_key_table(&child.owner).is_some() {
                DeletionKind::BlendKeyTable
            } else {
                DeletionKind::BlendConstraint
            };
            let parent = object(owner, &child.owner);
            if removed.contains(&parent)
                || child
                    .axis
                    .as_ref()
                    .is_some_and(|id| removed.contains(&object(DeletionKind::Axis, id)))
            {
                continue;
            }
            let result: Result<(Vec<String>, ConversionCost), Status> = (|| {
                if child.axis.is_some() {
                    let plan = candidate.plan_deletion_grid(
                        BindingGridEdit::Edit {
                            owner: child.owner.clone(),
                            operation: GridOperation::DeleteSample {
                                sample_id: key.id.clone(),
                            },
                        },
                        limits,
                        control,
                    )?;
                    let result = (plan.affected_ids().to_vec(), plan.cost());
                    let applied = candidate.apply_binding_grid_plan(&plan);
                    if !applied.status.is_ok() {
                        return Err(applied.status);
                    }
                    Ok(result)
                } else {
                    let edit = if owner == DeletionKind::BlendKeyTable {
                        BlendAuthoringEdit::Table {
                            id: child.owner.clone(),
                            operation: BlendTableOperation::Delete {
                                sample_id: key.id.clone(),
                                replacement_base: request
                                    .blend_base_replacements
                                    .get(&child.owner)
                                    .cloned(),
                            },
                        }
                    } else {
                        let c = candidate.get_blend_constraint(&child.owner).unwrap();
                        BlendAuthoringEdit::Constraint {
                            id: c.id.clone(),
                            parameter_id: c.parameter_id.clone(),
                            samples: c
                                .samples
                                .iter()
                                .filter(|s| s.id != key.id)
                                .cloned()
                                .collect(),
                        }
                    };
                    let plan = candidate.plan_deletion_blend(edit, limits, control)?;
                    let result = (plan.affected_ids().to_vec(), plan.cost());
                    let applied = candidate.apply_blend_authoring_plan(&plan);
                    if !applied.status.is_ok() {
                        return Err(applied.status);
                    }
                    Ok(result)
                }
            })();
            match result {
                Ok((ids, converted)) => {
                    affected.extend(ids);
                    *cost = limits.check_conversion(
                        cost.sample_combinations.max(converted.sample_combinations),
                        cost.payload_bytes
                            .checked_add(converted.payload_bytes)
                            .ok_or_else(|| {
                                Status::error("AUTHORING_CAPACITY_OVERFLOW", "Deletion samples")
                            })?,
                        self.estimated_content_bytes(),
                    )?;
                }
                Err(status) if status.code.starts_with("AUTHORING_") => return Err(status),
                Err(status) => {
                    if status.code == "OBJECT_LOCKED" {
                        report.blockers.push(DeletionLock {
                            object: parent,
                            source_id: status.message,
                        });
                    } else {
                        let base = status.code == "BASE_REPLACEMENT_REQUIRED";
                        report.decisions.push(DeletionDecision {
                            object: if base {
                                parent
                            } else {
                                child
                                    .axis
                                    .as_ref()
                                    .map(|id| object(DeletionKind::Axis, id))
                                    .unwrap_or(parent)
                            },
                            reason: if base {
                                DeletionDecisionReason::BlendBaseReplacementRequired
                            } else {
                                DeletionDecisionReason::InvalidContainer { code: status.code }
                            },
                            choices: if base {
                                vec![
                                    DeletionChoice::ChooseBlendBase,
                                    DeletionChoice::DeleteSubtree,
                                    DeletionChoice::Cancel,
                                ]
                            } else {
                                vec![DeletionChoice::DeleteSubtree, DeletionChoice::Cancel]
                            },
                        });
                    }
                    return Ok(false);
                }
            }
        }
        // Stable owner lookup is indexed once, rather than searching all tracks
        // for every selected key. Scoped source scenes use the same algorithm.
        for (owner, children) in &index.owned {
            if removed.contains(owner) {
                continue;
            }
            let keys: BTreeSet<_> = children
                .iter()
                .filter(|k| {
                    matches!(
                        k.kind,
                        DeletionKind::MotionKey | DeletionKind::ExpressionKey
                    ) && removed.contains(*k)
                })
                .map(|k| k.id.as_str())
                .collect();
            if keys.is_empty() {
                continue;
            }
            if let Some(expression) = &owner.owner_id {
                let e = candidate.content.expressions.get_mut(expression).unwrap();
                let mut clip = e.source_scene().unwrap();
                let restricted = clip.restricted_beziers;
                let track = clip.tracks.iter_mut().find(|t| t.id == owner.id).unwrap();
                if !track.retain_keys(|id| !keys.contains(id), restricted) {
                    return Err(Status::error("DELETION_EMPTY_TRACK", &owner.id));
                }
                e.preserve_source_scene(&clip);
            } else if owner.kind == DeletionKind::MotionTrack {
                let clip_id = self.get_motion_track(&owner.id).unwrap().0.id.clone();
                let clip = Arc::make_mut(candidate.content.motions.get_mut(&clip_id).unwrap());
                let restricted = clip.restricted_beziers;
                let track = clip.tracks.iter_mut().find(|t| t.id == owner.id).unwrap();
                if !track.retain_keys(|id| !keys.contains(id), restricted) {
                    return Err(Status::error("DELETION_EMPTY_TRACK", &owner.id));
                }
            }
            report.rewritten.push(DeletionRelation {
                owner: owner.clone(),
                field: DeletionField::ConvertedData,
                target: owner.clone(),
            });
            report
                .effects
                .push(DeletionEffect::ContentChanged(owner.clone()));
        }
        for glue in candidate.content.glues.values_mut() {
            if removed.contains(&object(DeletionKind::Glue, &glue.id)) {
                continue;
            }
            let before = glue.pairs.len();
            glue.pairs
                .retain(|p| !removed.contains(&object(DeletionKind::GluePair, &p.id)));
            if before != glue.pairs.len() {
                let key = object(DeletionKind::Glue, &glue.id);
                report.rewritten.push(DeletionRelation {
                    owner: key.clone(),
                    field: DeletionField::ConvertedData,
                    target: key.clone(),
                });
                report.effects.push(DeletionEffect::ContentChanged(key));
            }
        }
        for id in affected {
            let Some(key) = self.changed_deletion_object(candidate, &id) else {
                continue;
            };
            if let Err(status) = self.deletion_unlocked(&key) {
                report.blockers.push(DeletionLock {
                    object: key,
                    source_id: status.message,
                });
                return Ok(false);
            }
            report.rewritten.push(DeletionRelation {
                owner: key.clone(),
                field: DeletionField::ConvertedData,
                target: key.clone(),
            });
            report.effects.push(DeletionEffect::ContentChanged(key));
        }
        Ok(true)
    }
}

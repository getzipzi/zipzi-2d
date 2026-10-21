//! Immutable resource feedback, separate from transient playback and authoring projections.
use crate::{Document, ExpressionResource, MotionResource};
use kasane_core::document::{ExpressionTarget, MotionTrackTarget, PosePartRef};
use kasane_sdk::MotionSnapshot;
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq)]
pub enum AnimationFailure {
    MotionRemoved,
    ExpressionUnavailable,
    TriggersRemoved,
    RendererStopped,
    Runtime(kasane_sdk::AnimationError),
    Other(String),
}
impl AnimationFailure {
    pub fn from_error(error: &anyhow::Error) -> Self {
        error
            .downcast_ref::<kasane_sdk::AnimationError>()
            .map_or_else(
                || Self::Other(error.to_string()),
                |error| Self::Runtime(error.clone()),
            )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnimationDiagnostic {
    MissingAttachment {
        path: String,
    },
    UnresolvedMotion {
        motion: String,
        category: String,
        target: String,
    },
    UnsupportedChannel {
        motion: String,
        channel: String,
    },
    PartOpacityWithoutPose {
        motion: String,
        part: String,
    },
    UnconfiguredExpressionEntry {
        expression: String,
        entry_id: String,
    },
    UnresolvedExpression {
        expression: String,
        target: String,
    },
    UnresolvedPhysics {
        target: String,
    },
    UnresolvedPose {
        target: String,
    },
    SoundUnsupported {
        motion: String,
        file: String,
    },
    PhysicsStepLimit,
    RuntimeCoverage {
        detail: String,
    },
}

pub struct AnimationParameter {
    pub id: String,
    pub name: String,
    pub runtime_id: String,
}

pub struct AnimationCatalog {
    pub motions: Vec<MotionResource>,
    pub expressions: Vec<ExpressionResource>,
    pub parameters: Vec<AnimationParameter>,
    pub physics: bool,
    pub pose: bool,
    pub diagnostics: Vec<AnimationDiagnostic>,
    // Match SDK coverage to the same typed resource issue, so feedback is localized
    // and deduplicated. Unknown future SDK messages remain visible as details.
    coverage: HashMap<String, AnimationDiagnostic>,
}
impl AnimationCatalog {
    pub fn diagnostics_for(&self, snapshot: Option<&MotionSnapshot>) -> Vec<AnimationDiagnostic> {
        let mut result: BTreeSet<_> = self.diagnostics.iter().cloned().collect();
        if let Some(snapshot) = snapshot {
            for detail in &snapshot.coverage {
                result.insert(self.coverage.get(detail).cloned().unwrap_or_else(|| {
                    if detail == "Physics: step limit reached" {
                        AnimationDiagnostic::PhysicsStepLimit
                    } else {
                        AnimationDiagnostic::RuntimeCoverage {
                            detail: detail.clone(),
                        }
                    }
                }));
            }
        }
        result.into_iter().collect()
    }
    fn from_model(model: &kasane_core::Document) -> Self {
        let mut issues = BTreeSet::new();
        let mut coverage = HashMap::new();
        for path in model.missing_attachments() {
            issues.insert(AnimationDiagnostic::MissingAttachment { path: path.clone() });
        }
        for id in model.motion_order() {
            let Some(motion) = model.get_motion(id) else {
                continue;
            };
            for track in &motion.tracks {
                let (message, diagnostic) = match &track.target {
                    MotionTrackTarget::Unresolved {
                        category,
                        runtime_id,
                    } => (
                        format!("{id}: virtual {category} {runtime_id} has no drawable target"),
                        AnimationDiagnostic::UnresolvedMotion {
                            motion: motion.name.clone(),
                            category: category.clone(),
                            target: runtime_id.clone(),
                        },
                    ),
                    MotionTrackTarget::Model { runtime_id } if runtime_id != "Opacity" => (
                        format!("{id}: Model {runtime_id} mapping requires model3 Groups"),
                        AnimationDiagnostic::UnsupportedChannel {
                            motion: motion.name.clone(),
                            channel: runtime_id.clone(),
                        },
                    ),
                    MotionTrackTarget::PartOpacity { part_id } if model.pose().is_none() => (
                        format!("{id}: PartOpacity {part_id} has no Pose asset"),
                        AnimationDiagnostic::PartOpacityWithoutPose {
                            motion: motion.name.clone(),
                            part: model
                                .get_part(part_id)
                                .map_or_else(|| part_id.clone(), |p| p.name.clone()),
                        },
                    ),
                    _ => continue,
                };
                coverage.insert(message, diagnostic.clone());
                issues.insert(diagnostic);
            }
        }
        for group in model.motion_groups() {
            for (index, entry) in group.entries.iter().enumerate() {
                if let Some(file) = &entry.sound {
                    issues.insert(AnimationDiagnostic::SoundUnsupported {
                        motion: format!("{}[{}]", group.name, index + 1),
                        file: file.clone(),
                    });
                }
            }
        }
        for id in model.expression_order() {
            let Some(expression) = model.get_expression(id) else {
                continue;
            };
            for entry in &expression.entries {
                if entry.target == ExpressionTarget::Unspecified {
                    issues.insert(AnimationDiagnostic::UnconfiguredExpressionEntry {
                        expression: expression.name.clone(),
                        entry_id: entry.id.clone(),
                    });
                }
                if let ExpressionTarget::Unresolved { runtime_id } = &entry.target {
                    issues.insert(AnimationDiagnostic::UnresolvedExpression {
                        expression: expression.name.clone(),
                        target: runtime_id.clone(),
                    });
                }
            }
        }
        if let Some(physics) = model.physics() {
            for target in physics.referenced_runtime_ids() {
                if !physics.parameter_bindings.contains_key(target) {
                    let issue = AnimationDiagnostic::UnresolvedPhysics {
                        target: target.into(),
                    };
                    coverage.insert(
                        format!("Physics: unresolved parameter {target}"),
                        issue.clone(),
                    );
                    issues.insert(issue);
                }
            }
        }
        if let Some(pose) = model.pose() {
            for (group, entries) in pose.groups.iter().enumerate() {
                for (index, entry) in entries.iter().enumerate() {
                    for part in std::iter::once(&entry.part).chain(entry.links.iter().flatten()) {
                        if let PosePartRef::Unresolved { runtime_id } = part {
                            let issue = AnimationDiagnostic::UnresolvedPose {
                                target: runtime_id.clone(),
                            };
                            if part == &entry.part {
                                coverage.insert(
                                    format!("pose group {group} entry {index} has unresolved Part"),
                                    issue.clone(),
                                );
                            }
                            issues.insert(issue);
                        }
                    }
                }
            }
        }
        Self {
            motions: Vec::new(),
            expressions: Vec::new(),
            parameters: model
                .parameter_order()
                .iter()
                .filter_map(|id| model.get_parameter(id))
                .map(|p| AnimationParameter {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    runtime_id: p.runtime_id.clone(),
                })
                .collect(),
            physics: model.physics().is_some(),
            pose: model.pose().is_some(),
            diagnostics: issues.into_iter().collect(),
            coverage,
        }
    }
}
impl Document {
    /// Borrow resource definitions for read-only inspection without exposing the
    /// authoring model or copying motion curves on each preview frame.
    pub fn motion_definition(&self, id: &str) -> Option<&kasane_core::document::MotionClip> {
        self.model().get_motion(id)
    }

    pub fn expression_definition(
        &self,
        id: &str,
    ) -> Option<&kasane_core::document::ExpressionAsset> {
        self.model().get_expression(id)
    }

    pub fn physics_definition(&self) -> Option<&kasane_core::document::PhysicsAsset> {
        self.model().physics()
    }

    pub fn pose_definition(&self) -> Option<&kasane_core::document::PoseAsset> {
        self.model().pose()
    }

    pub fn animation_catalog(&self) -> Arc<AnimationCatalog> {
        let generation = self.model_generation();
        let mut cache = self.animation_catalog.lock().unwrap();
        if let Some((revision, catalog)) = &*cache {
            if *revision == generation {
                return catalog.clone();
            }
        }
        let mut catalog = AnimationCatalog::from_model(self.model());
        catalog.motions = self.motion_resources();
        catalog.expressions = self.expression_resources();
        let catalog = Arc::new(catalog);
        *cache = Some((generation, catalog.clone()));
        catalog
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_files_are_visible_without_starting_a_preview() {
        let mut model = kasane_core::Document::new();
        model.set_missing_attachments(vec![
            "motions/missing.motion3.json".into(),
            "textures/missing.png".into(),
        ]);
        let catalog = AnimationCatalog::from_model(&model);
        assert_eq!(
            catalog.diagnostics_for(None),
            vec![
                AnimationDiagnostic::MissingAttachment {
                    path: "motions/missing.motion3.json".into()
                },
                AnimationDiagnostic::MissingAttachment {
                    path: "textures/missing.png".into()
                },
            ]
        );
    }
}

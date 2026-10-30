//! Human-readable deletion report labels.
use super::*;

pub(super) fn section_label(section: S) -> String {
    t!(match section {
        S::Removed => "deletion.removed",
        S::Owned => "deletion.owned",
        S::Rewritten => "deletion.rewritten",
        S::Effects => "deletion.effects",
        S::Decisions => "deletion.decisions",
        S::Locks => "deletion.locks",
        S::Structure => "deletion.structure",
        S::Deformation => "deletion.deformation",
        S::Organization => "deletion.organization",
        S::GuaranteeLimits => "deletion.limits",
        S::ConversionDomain => "deletion.domain",
        S::Exports => "deletion.exports",
        S::Relations => "deletion.relations",
    })
    .into_owned()
}
pub(super) fn choice_label(choice: &C) -> String {
    t!(match choice {
        C::PreserveEditState => "deletion.edit_state",
        C::KeepLocalData => "deletion.local",
        C::ChoosePart => "deletion.choose_part",
        C::SupplyOffscreenMapping => "deletion.mapping",
        C::DeleteSubtree => "deletion.subtree",
        C::ChooseTexture => "deletion.use_texture",
        C::ClearTexture => "deletion.clear_texture",
        C::ChoosePhysicsNode => "deletion.use_node",
        C::ChooseBlendBase => "deletion.use_base",
        C::Cancel => "project.cancel",
    })
    .into_owned()
}
pub(super) fn strategy_label(s: &zipzi_sdk::ReparentStrategy) -> String {
    t!(match s {
        zipzi_sdk::ReparentStrategy::KeepLocalData => "deletion.local",
        zipzi_sdk::ReparentStrategy::PreserveEditState => "deletion.edit_state",
        zipzi_sdk::ReparentStrategy::PreserveParameterDomain => "deletion.parameter_domain",
    })
    .into_owned()
}
pub(super) fn field_label(f: &zipzi_sdk::DeletionField) -> String {
    use zipzi_sdk::DeletionField::*;
    t!(match f {
        Owned => "deletion.owned",
        MeshMask { .. } | OffscreenMask { .. } | MeshInvertedMask => "deletion.mask",
        GlueMeshA | GlueMeshB => "deletion.glue",
        HitAreaMesh => "deletion.hit_area",
        DrawingItem { .. } => "deletion.drawing",
        OrganizationParent => "deletion.organization",
        DeformationParent | ConvertedData => "deletion.deformation",
        PosePart | PoseLinkPart | MotionPart | ExpressionSourcePart => "deletion.animation",
        CdiPart { .. } => "deletion.display",
        ParameterAxis | BlendTableParameter | BlendConstraintParameter =>
            "deletion.parameter_dimension",
        BlendTable | BlendConstraint { .. } => "deletion.binding_loss",
        MotionParameter | ExpressionParameter | ExpressionSourceParameter | MotionClip =>
            "deletion.animation",
        MotionRegistrationContainer => "deletion.move_to_root",
        PhysicsParameter | PhysicsNode => "deletion.physics",
        Texture | AttachmentSound | AttachmentUserData => "deletion.resource",
        ParameterFolder | ModelParameter | CdiParameter { .. } | CdiCombinedParameter =>
            "deletion.display",
        RequestedScope => "deletion.scope",
    })
    .into_owned()
}
pub(super) fn decision_label(r: &zipzi_sdk::DeletionDecisionReason, details: bool) -> String {
    use zipzi_sdk::DeletionDecisionReason::*;
    match r {
        ConversionUnproven { code } => crate::authoring::availability::reason_message(code)
            .unwrap_or_else(|| {
                if details {
                    code.clone()
                } else {
                    t!("deletion.conversion_unavailable").into_owned()
                }
            }),
        OffscreenOwnerUnavailable => t!("deletion.owner_required").into_owned(),
        OffscreenMappingRequired => t!("deletion.mapping_required").into_owned(),
        PartStateUnpreserved => t!("deletion.part_state").into_owned(),
        TextureReplacementRequired => t!("deletion.texture_required").into_owned(),
        BlendBaseReplacementRequired => t!("deletion.base_required").into_owned(),
        PhysicsNodeReplacementRequired => t!("deletion.node_required").into_owned(),
        InvalidContainer { code } if code == "LAST_MOTION_REGISTRATION" => {
            t!("deletion.last_registration").into_owned()
        }
        InvalidContainer { code } => {
            if details {
                format!("{}: {code}", t!("deletion.invalid_container"))
            } else {
                t!("deletion.invalid_structure").into_owned()
            }
        }
    }
}
pub(super) fn effect_label(
    e: &zipzi_sdk::DeletionEffect,
    name: impl Fn(&DeletionObject) -> String,
) -> String {
    use zipzi_sdk::DeletionEffect::*;
    let (key, object) = match e {
        MotionRegistrationMoved {
            registration,
            group,
        } => {
            return format!(
                "{} · {}: {}",
                name(registration),
                t!("deletion.move_to_root"),
                name(group)
            );
        }
        ContentChanged(o) => ("deletion.content_changed", o),
        ParameterAnimationRemoved(o) => ("deletion.animation_effect", o),
        ParameterDimensionRemoved {
            object,
            parameter,
            value,
            effective_value,
            disabled,
        } => {
            return format!(
                "{} · {}: {} = {value} → {effective_value} · {}: {disabled}",
                name(object),
                t!("deletion.parameter_dimension"),
                name(parameter),
                t!("deletion.disabled")
            );
        }
        BindingContributionRemoved { object, binding } => {
            return format!(
                "{} · {}: {}",
                name(object),
                t!("deletion.binding_loss"),
                name(binding)
            );
        }
        PhysicsChannelRemoved { object, group } => {
            return format!(
                "{} · {}: {}",
                name(group),
                t!("deletion.physics"),
                name(object)
            );
        }
        ResourceReplaced {
            object,
            source,
            replacement,
        } => {
            return format!(
                "{} · {}: {} → {}",
                name(object),
                t!("deletion.resource"),
                name(source),
                replacement
                    .as_ref()
                    .map(&name)
                    .unwrap_or_else(|| t!("deletion.unconfigured").into_owned())
            );
        }
        MaskChanged(o) => ("deletion.mask_effect", o),
        GlueRemoved(o) => ("deletion.glue_effect", o),
        HitAreaRemoved(o) => ("deletion.hit_area", o),
        CompositionChanged(o) => ("deletion.composition_effect", o),
        PartAnimationRemoved(o) => ("deletion.animation_effect", o),
        OrganizationChanged(o) => ("deletion.organization", o),
        DeformationChanged(o) => ("deletion.deformation", o),
        DrawingGroupFlattened {
            group,
            destination,
            source_range,
            destination_range,
            dynamic_order,
            offscreen,
        } => {
            return format!(
                "{} · {} → {} · {:?} → {:?} · {}: {dynamic_order} · {}",
                t!("deletion.drawing_effect"),
                name(group),
                name(destination),
                source_range,
                destination_range,
                t!("deletion.dynamic_order"),
                offscreen.as_ref().map(&name).unwrap_or_default()
            );
        }
        UnpreservedChannels { object, channels } => {
            return format!(
                "{} · {}: {}",
                name(object),
                t!("deletion.unpreserved"),
                channels
                    .iter()
                    .map(|c| t!(match c {
                        zipzi_sdk::DeletionChannel::Appearance => "deletion.appearance",
                        zipzi_sdk::DeletionChannel::Enabled => "deletion.enabled",
                        zipzi_sdk::DeletionChannel::GluePostprocessing => "deletion.glue",
                        zipzi_sdk::DeletionChannel::DrawingOrder => "deletion.drawing",
                    })
                    .into_owned())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        PartStateRemoved {
            part,
            enabled,
            animated_order,
        } => {
            return format!(
                "{} · {} · {}: {enabled} · {}: {animated_order}",
                name(part),
                t!("deletion.part_state"),
                t!("deletion.enabled"),
                t!("deletion.dynamic_order")
            );
        }
    };
    format!("{} · {}", name(object), t!(key))
}
pub(super) fn export_label(e: &zipzi_sdk::DeletionExportFailure) -> String {
    use zipzi_sdk::DeletionExportFailure::*;
    match e {
        Model3NamespaceChanged => "model3 · OPAQUE_NAMESPACE_CHANGED".into(),
        Cdi { problem } => format!("CDI · {problem:?}"),
        Motion { id, problem } => format!("Motion {id} · {problem:?}"),
        Expression { id, problem } => format!("Expression {id} · {problem:?}"),
        Pose { id, problem } => format!("Pose {id} · {problem:?}"),
        Physics { id, problem } => format!("Physics {id} · {problem:?}"),
    }
}

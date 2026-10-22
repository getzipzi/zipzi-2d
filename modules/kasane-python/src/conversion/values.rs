use super::*;

pub(crate) fn blend_mode_from_name(name: &str) -> PyResult<BlendMode> {
    match name {
        "normal" => Ok(BlendMode::Normal),
        "additive" => Ok(BlendMode::Additive),
        "multiplicative" => Ok(BlendMode::Multiplicative),
        _ => Err(PyValueError::new_err("Unknown mesh blend mode")),
    }
}

pub(crate) fn parameter_kind_from_name(name: &str) -> PyResult<ParameterKind> {
    match name {
        "normal" => Ok(ParameterKind::Normal),
        "blend_shape" => Ok(ParameterKind::BlendShape),
        _ => Err(PyValueError::new_err("Unknown parameter kind")),
    }
}

pub(crate) fn parameter_kind_name(kind: ParameterKind) -> &'static str {
    match kind {
        ParameterKind::Normal => "normal",
        ParameterKind::BlendShape => "blend_shape",
    }
}

pub(crate) fn blend_mode_name(mode: BlendMode) -> &'static str {
    match mode {
        BlendMode::Normal => "normal",
        BlendMode::Additive => "additive",
        BlendMode::Multiplicative => "multiplicative",
    }
}

pub(super) fn pose_from_tuple(value: PoseTuple) -> RotationPose {
    RotationPose {
        origin: PreciseVec2::new(value.0, value.1),
        angle: value.2,
        scale: value.3,
        reflect_x: value.4,
        reflect_y: value.5,
    }
}

pub(super) fn pose_tuple(pose: RotationPose) -> PoseTuple {
    (
        pose.origin.x,
        pose.origin.y,
        pose.angle,
        pose.scale,
        pose.reflect_x,
        pose.reflect_y,
    )
}

pub(crate) fn appearance_from_tuple(value: AppearanceTuple) -> Appearance {
    let (opacity, multiply, screen) = value;
    Appearance {
        opacity,
        multiply: [multiply.0, multiply.1, multiply.2],
        screen: [screen.0, screen.1, screen.2],
    }
}

pub(crate) fn appearance_tuple(value: Appearance) -> AppearanceTuple {
    (
        value.opacity,
        (value.multiply[0], value.multiply[1], value.multiply[2]),
        (value.screen[0], value.screen[1], value.screen[2]),
    )
}

pub(crate) fn object_kind(value: &str) -> PyResult<ObjectKind> {
    match value {
        "binding_axis" => Ok(ObjectKind::BindingAxis),
        "binding_sample" => Ok(ObjectKind::BindingSample),
        "asset" => Ok(ObjectKind::Asset),
        "mesh" => Ok(ObjectKind::Mesh),
        "parameter" => Ok(ObjectKind::Parameter),
        "mesh_binding" => Ok(ObjectKind::MeshBinding),
        "part" => Ok(ObjectKind::Part),
        "transform" => Ok(ObjectKind::Transform),
        "scene_binding" => Ok(ObjectKind::SceneBinding),
        "blend_key_table" => Ok(ObjectKind::BlendKeyTable),
        "blend_constraint" => Ok(ObjectKind::BlendConstraint),
        "blend_binding" => Ok(ObjectKind::BlendBinding),
        "glue" => Ok(ObjectKind::Glue),
        "offscreen" => Ok(ObjectKind::Offscreen),
        "draw_order_group" => Ok(ObjectKind::DrawOrderGroup),
        "cdi_parameter_group" => Ok(ObjectKind::CdiParameterGroup),
        "cdi_combined_set" => Ok(ObjectKind::CdiCombinedSet),
        "expression" => Ok(ObjectKind::Expression),
        "expression_entry" => Ok(ObjectKind::ExpressionEntry),
        "motion_group" => Ok(ObjectKind::MotionGroup),
        "motion_registration" => Ok(ObjectKind::MotionRegistration),
        "motion_track" => Ok(ObjectKind::MotionTrack),
        "motion_key" => Ok(ObjectKind::MotionKey),
        "motion_event" => Ok(ObjectKind::MotionEvent),
        "attachment" => Ok(ObjectKind::Attachment),
        "motion" => Ok(ObjectKind::Motion),
        "pose" => Ok(ObjectKind::Pose),
        "pose_group" => Ok(ObjectKind::PoseGroup),
        "pose_entry" => Ok(ObjectKind::PoseEntry),
        "pose_link" => Ok(ObjectKind::PoseLink),
        "physics" => Ok(ObjectKind::Physics),
        "physics_group" => Ok(ObjectKind::PhysicsGroup),
        "physics_input" => Ok(ObjectKind::PhysicsInput),
        "physics_output" => Ok(ObjectKind::PhysicsOutput),
        "physics_node" => Ok(ObjectKind::PhysicsNode),
        _ => Err(PyValueError::new_err(format!(
            "Unknown object kind: {value}"
        ))),
    }
}

pub(crate) fn object_kind_name(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::BindingAxis => "binding_axis",
        ObjectKind::BindingSample => "binding_sample",
        ObjectKind::Asset => "asset",
        ObjectKind::Mesh => "mesh",
        ObjectKind::Parameter => "parameter",
        ObjectKind::MeshBinding => "mesh_binding",
        ObjectKind::Part => "part",
        ObjectKind::Transform => "transform",
        ObjectKind::SceneBinding => "scene_binding",
        ObjectKind::BlendKeyTable => "blend_key_table",
        ObjectKind::BlendConstraint => "blend_constraint",
        ObjectKind::BlendBinding => "blend_binding",
        ObjectKind::Glue => "glue",
        ObjectKind::Offscreen => "offscreen",
        ObjectKind::DrawOrderGroup => "draw_order_group",
        ObjectKind::CdiParameterGroup => "cdi_parameter_group",
        ObjectKind::CdiCombinedSet => "cdi_combined_set",
        ObjectKind::Expression => "expression",
        ObjectKind::ExpressionEntry => "expression_entry",
        ObjectKind::MotionGroup => "motion_group",
        ObjectKind::MotionRegistration => "motion_registration",
        ObjectKind::MotionTrack => "motion_track",
        ObjectKind::MotionKey => "motion_key",
        ObjectKind::MotionEvent => "motion_event",
        ObjectKind::Attachment => "attachment",
        ObjectKind::Motion => "motion",
        ObjectKind::Pose => "pose",
        ObjectKind::PoseGroup => "pose_group",
        ObjectKind::PoseEntry => "pose_entry",
        ObjectKind::PoseLink => "pose_link",
        ObjectKind::Physics => "physics",
        ObjectKind::PhysicsGroup => "physics_group",
        ObjectKind::PhysicsInput => "physics_input",
        ObjectKind::PhysicsOutput => "physics_output",
        ObjectKind::PhysicsNode => "physics_node",
    }
}

pub(crate) fn rotation_data(value: RotationTuple) -> RotationTransform {
    let (base_angle, (x, y, angle, scale, reflect_x, reflect_y)) = value;
    RotationTransform {
        base_angle,
        pose: RotationPose {
            origin: PreciseVec2::new(x, y),
            angle,
            scale,
            reflect_x,
            reflect_y,
        },
    }
}

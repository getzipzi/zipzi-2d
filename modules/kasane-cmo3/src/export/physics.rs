use super::*;

impl Exporter<'_> {
    pub(super) fn physics(&mut self) -> N {
        use kasane_core::physics::PhysicsAxis;
        let Some(asset) = self.doc.physics() else {
            return N::new("CPhysicsSettingsSourceSet")
                .field("_sourceCubismPhysics", N::empty_list())
                .field("selectedCubismPhysics", N::null())
                .field("settingFPS", i(60));
        };
        let mut settings = Vec::new();
        let mut selected = N::null();
        for rig in &asset.data.settings {
            let g = self.new_guid("CPhysicsSettingsGuid");
            if settings.is_empty() {
                selected = g.clone();
            }
            let mut inputs = Vec::new();
            let mut outputs = Vec::new();
            let mut vertices = Vec::new();
            for v in rig.inputs.iter().filter(|v| v.kind != PhysicsAxis::Y) {
                let data_guid = self.new_guid("CPhysicsDataGuid");
                inputs.push(
                    N::new("CPhysicsInput")
                        .field("guid", data_guid)
                        .field(
                            "source",
                            guid("CParameterGuid", &asset.parameter_bindings[&v.source.id]),
                        )
                        .field("angleScale", f(0))
                        .field(
                            "translationScale",
                            N::new("GVector2").field("x", f(0)).field("y", f(0)),
                        )
                        .field("weight", f(v.weight))
                        .field(
                            "type",
                            N::enumeration(
                                "CPhysicsSourceType",
                                if v.kind == PhysicsAxis::X {
                                    "SRC_TO_X"
                                } else {
                                    "SRC_TO_G_ANGLE"
                                },
                            ),
                        )
                        .field("isReverse", b(v.reflect)),
                );
            }
            for v in rig.outputs.iter().filter(|v| v.kind != PhysicsAxis::Y) {
                let data_guid = self.new_guid("CPhysicsDataGuid");
                outputs.push(
                    N::new("CPhysicsOutput")
                        .field("guid", data_guid)
                        .field(
                            "destination",
                            guid(
                                "CParameterGuid",
                                &asset.parameter_bindings[&v.destination.id],
                            ),
                        )
                        .field("vertexIndex", i(v.vertex_index))
                        .field(
                            "translationScale",
                            N::new("GVector2")
                                .field(
                                    "x",
                                    f(if v.kind == PhysicsAxis::X {
                                        v.scale
                                    } else {
                                        0.
                                    }),
                                )
                                .field("y", f(0)),
                        )
                        .field(
                            "angleScale",
                            f(if v.kind == PhysicsAxis::Angle {
                                v.scale
                            } else {
                                0.
                            }),
                        )
                        .field("weight", f(v.weight))
                        .field(
                            "type",
                            N::enumeration(
                                "CPhysicsSourceType",
                                if v.kind == PhysicsAxis::X {
                                    "SRC_TO_X"
                                } else {
                                    "SRC_TO_G_ANGLE"
                                },
                            ),
                        )
                        .field("isReverse", b(v.reflect)),
                );
            }
            for v in &rig.vertices {
                let data_guid = self.new_guid("CPhysicsDataGuid");
                vertices.push(
                    N::new("CPhysicsVertex")
                        .field("guid", data_guid)
                        .field(
                            "position",
                            N::new("GVector2")
                                .field("x", f(v.position.x))
                                .field("y", f(v.position.y)),
                        )
                        .field("mobility", f(v.mobility))
                        .field("delay", f(v.delay))
                        .field("acceleration", f(v.acceleration))
                        .field("radius", f(v.radius)),
                );
            }
            let n = &rig.normalization;
            let name = asset
                .data
                .meta
                .dictionary
                .iter()
                .find(|d| d.id == rig.id)
                .map_or(rig.id.as_str(), |d| d.name.as_str());
            settings.push(
                N::new("CPhysicsSettingsSource")
                    .field("name", s(name))
                    .field("guid", g)
                    .field("id", ident("CPhysicsSettingId", &rig.id))
                    .field("inputs", N::list("carray_list", inputs))
                    .field("outputs", N::list("carray_list", outputs))
                    .field("vertices", N::list("carray_list", vertices))
                    .field("normalizedPositionValueMax", f(n.position.maximum))
                    .field("normalizedPositionValueMin", f(n.position.minimum))
                    .field("normalizedPositionDefaultValue", f(n.position.default))
                    .field("normalizedAngleValueMax", f(n.angle.maximum))
                    .field("normalizedAngleValueMin", f(n.angle.minimum))
                    .field("normalizedAngleDefaultValue", f(n.angle.default)),
            );
        }
        N::new("CPhysicsSettingsSourceSet")
            .field("_sourceCubismPhysics", N::list("carray_list", settings))
            .field("selectedCubismPhysics", selected)
            .field(
                "settingFPS",
                asset
                    .data
                    .meta
                    .fps
                    .map_or(N::null(), |fps| i(fps.round() as i32)),
            )
    }
}

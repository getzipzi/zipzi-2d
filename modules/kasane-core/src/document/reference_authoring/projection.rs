use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReferenceDisabledReason {
    Unspecified,
    Unresolved { runtime_id: String },
    EmptyGeometry { mesh_id: String },
    EmptyMembers,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisabledReferenceItem {
    pub id: String,
    pub owner: Option<String>,
    pub reason: ReferenceDisabledReason,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedParameterReferences {
    pub name: Option<String>,
    pub id: String,
    pub parameters: Vec<String>,
    pub members: Vec<String>,
    pub member_slots: HashMap<String, usize>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedHitArea {
    pub name: String,
    pub id: String,
    pub mesh_id: String,
    pub source_slot: usize,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceProjection {
    pub group_slots: HashMap<String, usize>,
    pub combined_set_slots: HashMap<String, usize>,
    pub groups: Vec<ProjectedParameterReferences>,
    pub combined_sets: Vec<ProjectedParameterReferences>,
    pub hit_areas: Vec<ProjectedHitArea>,
    pub hit_area_slots: HashMap<String, usize>,
    pub disabled: Vec<DisabledReferenceItem>,
}
impl Document {
    pub fn reference_projection(&self) -> Arc<ReferenceProjection> {
        self.reference_projection
            .get_or_init(|| {
                let mut out = ReferenceProjection::default();
                for g in self.model3_settings().groups.iter().flatten() {
                    let mut p = ProjectedParameterReferences {
                        id: g.id.clone(),
                        name: Some(g.name.clone()),
                        parameters: vec![],
                        members: vec![],
                        member_slots: HashMap::new(),
                    };
                    for m in &g.parameters {
                        match &m.target {
                            ModelTargetRef::Resolved { object_id } => {
                                p.member_slots.insert(m.id.clone(), p.parameters.len());
                                p.parameters.push(object_id.clone());
                                p.members.push(m.id.clone());
                            }
                            target => out.disabled.push(DisabledReferenceItem {
                                id: m.id.clone(),
                                owner: Some(g.id.clone()),
                                reason: match target {
                                    ModelTargetRef::Unresolved { runtime_id } => {
                                        ReferenceDisabledReason::Unresolved {
                                            runtime_id: runtime_id.clone(),
                                        }
                                    }
                                    _ => ReferenceDisabledReason::Unspecified,
                                },
                            }),
                        }
                    }
                    if p.parameters.is_empty() {
                        out.disabled.push(DisabledReferenceItem {
                            id: g.id.clone(),
                            owner: None,
                            reason: ReferenceDisabledReason::EmptyMembers,
                        });
                    }
                    out.group_slots.insert(p.id.clone(), out.groups.len());
                    out.groups.push(p);
                }
                for s in self.display_info().combined_parameters.iter().flatten() {
                    let mut p = ProjectedParameterReferences {
                        id: s.id.clone(),
                        name: None,
                        parameters: vec![],
                        members: vec![],
                        member_slots: HashMap::new(),
                    };
                    for m in &s.members {
                        match &m.target {
                            CdiParameterRef::Resolved { parameter_id } => {
                                p.member_slots.insert(m.id.clone(), p.parameters.len());
                                p.parameters.push(parameter_id.clone());
                                p.members.push(m.id.clone());
                            }
                            target => out.disabled.push(DisabledReferenceItem {
                                id: m.id.clone(),
                                owner: Some(s.id.clone()),
                                reason: match target {
                                    CdiParameterRef::Unresolved { runtime_id } => {
                                        ReferenceDisabledReason::Unresolved {
                                            runtime_id: runtime_id.clone(),
                                        }
                                    }
                                    _ => ReferenceDisabledReason::Unspecified,
                                },
                            }),
                        }
                    }
                    if p.parameters.is_empty() {
                        out.disabled.push(DisabledReferenceItem {
                            id: s.id.clone(),
                            owner: None,
                            reason: ReferenceDisabledReason::EmptyMembers,
                        });
                    }
                    out.combined_set_slots
                        .insert(p.id.clone(), out.combined_sets.len());
                    out.combined_sets.push(p);
                }
                for (source_slot, a) in self
                    .model3_settings()
                    .hit_areas
                    .iter()
                    .flatten()
                    .enumerate()
                {
                    let reason = match &a.mesh {
                        ModelTargetRef::Unspecified => Some(ReferenceDisabledReason::Unspecified),
                        ModelTargetRef::Unresolved { runtime_id } => {
                            Some(ReferenceDisabledReason::Unresolved {
                                runtime_id: runtime_id.clone(),
                            })
                        }
                        ModelTargetRef::Resolved { object_id } => {
                            if self
                                .get_mesh(object_id)
                                .is_none_or(|m| m.vertex_ids.is_empty())
                            {
                                Some(ReferenceDisabledReason::EmptyGeometry {
                                    mesh_id: object_id.clone(),
                                })
                            } else {
                                None
                            }
                        }
                    };
                    if let Some(reason) = reason {
                        out.disabled.push(DisabledReferenceItem {
                            id: a.id.clone(),
                            owner: None,
                            reason,
                        });
                    } else if let ModelTargetRef::Resolved { object_id } = &a.mesh {
                        out.hit_area_slots.insert(a.id.clone(), out.hit_areas.len());
                        out.hit_areas.push(ProjectedHitArea {
                            name: a.name.clone(),
                            id: a.id.clone(),
                            mesh_id: object_id.clone(),
                            source_slot,
                        });
                    }
                }
                Arc::new(out)
            })
            .clone()
    }
}

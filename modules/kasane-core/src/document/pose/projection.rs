//! Compact execution slots retain stable source identities in both directions.
use super::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoseDisabledReason {
    NeedsConfiguration,
    Unresolved { runtime_id: String },
    OwnerUnavailable { entry_id: String },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisabledPoseItem {
    pub id: String,
    pub reason: PoseDisabledReason,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PoseProjection {
    pub revision: u64,
    pub groups: Vec<PoseGroup>,
    /// Group / entry / link ID -> compact group / entry / link slots.
    pub slots: BTreeMap<String, Vec<usize>>,
    pub disabled: Vec<DisabledPoseItem>,
}
impl Document {
    pub fn pose_projection(&self) -> Result<Option<PoseProjection>, Status> {
        let Some(p) = self.pose() else {
            return Ok(None);
        };
        let status = self.validate_pose(p);
        if !status.is_ok() {
            return Err(status);
        }
        let mut out = PoseProjection {
            revision: self.revision(),
            groups: vec![],
            slots: BTreeMap::new(),
            disabled: vec![],
        };
        let ready =
            |disabled: &mut Vec<DisabledPoseItem>, id: &str, target: &PosePartRef| match target {
                PosePartRef::Resolved { .. } => true,
                target => {
                    disabled.push(DisabledPoseItem {
                        id: id.into(),
                        reason: match target {
                            PosePartRef::Unspecified => PoseDisabledReason::NeedsConfiguration,
                            PosePartRef::Unresolved { runtime_id } => {
                                PoseDisabledReason::Unresolved {
                                    runtime_id: runtime_id.clone(),
                                }
                            }
                            _ => unreachable!(),
                        },
                    });
                    false
                }
            };
        for (gi, g) in p.groups.iter().enumerate() {
            let mut group = PoseGroup {
                id: g.id.clone(),
                name: g.name.clone(),
                entries: Default::default(),
            };
            out.slots.insert(g.id.clone(), vec![gi]);
            for e in &g.entries {
                let entry_ready = ready(&mut out.disabled, &e.id, &e.part);
                let mut entry = e.clone();
                if let Some(links) = &mut entry.links {
                    links.retain(|l| ready(&mut out.disabled, &l.id, &l.target));
                }
                if !entry_ready {
                    for l in entry.links.iter().flatten() {
                        out.disabled.push(DisabledPoseItem {
                            id: l.id.clone(),
                            reason: PoseDisabledReason::OwnerUnavailable {
                                entry_id: e.id.clone(),
                            },
                        });
                    }
                    continue;
                }
                let ei = group.entries.len();
                out.slots.insert(e.id.clone(), vec![gi, ei]);
                for (li, l) in entry.links.iter().flatten().enumerate() {
                    out.slots.insert(l.id.clone(), vec![gi, ei, li]);
                }
                group.entries.push(entry);
            }
            out.groups.push(group);
        }
        Ok(Some(out))
    }
}

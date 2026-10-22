use super::*;
impl Document {
    pub(super) fn plan_effect_content(
        &self,
        plan: &mut SceneContentPlan,
        edit: SceneContentEdit,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        use SceneContentEdit as E;
        let glue_id = match &edit {
            E::InsertGluePair { glue_id, .. }
            | E::PatchGluePair { glue_id, .. }
            | E::MoveGluePairs { glue_id, .. }
            | E::TransferGluePairs { glue_id, .. }
            | E::DuplicateGluePairs { glue_id, .. }
            | E::DeleteGluePairs { glue_id, .. } => Some(glue_id.clone()),
            _ => None,
        };
        if let Some(id) = glue_id {
            self.content_unlocked(&id)?;
            let mut glue = self
                .get_glue(&id)
                .ok_or_else(|| Status::error("MISSING_GLUE", &id))?
                .clone();
            plan.domain.affected.push(id.clone());
            match edit {
                E::InsertGluePair { pair, before, .. } => {
                    if self.contains_id(&pair.id) {
                        return Err(Status::error("DUPLICATE_ID", &pair.id));
                    }
                    let at = anchor(&glue.pairs, &before)?;
                    glue.pairs.insert(at, pair);
                }
                E::PatchGluePair { pair, .. } => {
                    let p = glue
                        .pairs
                        .iter_mut()
                        .find(|p| p.id == pair.id)
                        .ok_or_else(|| Status::error("MISSING_GLUE_PAIR", &pair.id))?;
                    *p = pair;
                }
                E::MoveGluePairs { ids, before, .. } => {
                    selected(&glue, &ids)?;
                    if before.as_ref().is_some_and(|id| ids.contains(id)) {
                        return Ok(());
                    }
                    let moving: Vec<_> = glue
                        .pairs
                        .iter()
                        .filter(|p| ids.contains(&p.id))
                        .cloned()
                        .collect();
                    glue.pairs.retain(|p| !ids.contains(&p.id));
                    let at = anchor(&glue.pairs, &before)?;
                    glue.pairs.splice(at..at, moving);
                }
                E::TransferGluePairs {
                    destination,
                    pairs,
                    before,
                    ..
                } => {
                    if destination == id {
                        return Err(Status::error(
                            "SAME_GLUE_OWNER",
                            "Use move_glue_pairs for same-owner ordering",
                        ));
                    }
                    let ids = pairs.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
                    selected(&glue, &ids)?;
                    self.content_unlocked(&destination)?;
                    let mut target = self
                        .get_glue(&destination)
                        .ok_or_else(|| Status::error("MISSING_GLUE", &destination))?
                        .clone();
                    let ordered = glue
                        .pairs
                        .iter()
                        .filter(|p| ids.contains(&p.id))
                        .map(|p| pairs.iter().find(|v| v.id == p.id).unwrap().clone())
                        .collect::<Vec<_>>();
                    glue.pairs.retain(|p| !ids.contains(&p.id));
                    let at = anchor(&target.pairs, &before)?;
                    target.pairs.splice(at..at, ordered);
                    plan.domain.affected.extend(ids);
                    plan.domain.affected.push(destination);
                    plan.domain.delta.glues.push(target);
                }
                E::DuplicateGluePairs { ids, before, .. } => {
                    selected(&glue, &ids)?;
                    let mut copies = vec![];
                    for source in glue.pairs.iter().filter(|p| ids.contains(&p.id)) {
                        control.check()?;
                        let mut p = source.clone();
                        p.id = uuid::Uuid::new_v4().to_string();
                        plan.report
                            .generated_pair_ids
                            .insert(source.id.clone(), p.id.clone());
                        copies.push(p);
                    }
                    let at = anchor(&glue.pairs, &before)?;
                    glue.pairs.splice(at..at, copies);
                }
                E::DeleteGluePairs { ids, .. } => {
                    selected(&glue, &ids)?;
                    glue.pairs.retain(|p| !ids.contains(&p.id));
                    plan.report.removed_pair_ids = ids;
                }
                _ => unreachable!(),
            }
            plan.domain
                .affected
                .extend(glue.pairs.iter().map(|p| p.id.clone()));
            plan.domain
                .affected
                .extend(plan.report.removed_pair_ids.clone());
            plan.domain.delta.glues.push(glue);
            return Ok(());
        }
        let id = match &edit {
            E::InsertOffscreenKeyform { offscreen_id, .. }
            | E::PatchOffscreenKeyform { offscreen_id, .. }
            | E::DuplicateOffscreenKeyforms { offscreen_id, .. }
            | E::ReorderOffscreenKeyforms { offscreen_id, .. }
            | E::DeleteOffscreenKeyforms { offscreen_id, .. }
            | E::SetOffscreenMapping { offscreen_id, .. } => offscreen_id.clone(),
            _ => unreachable!(),
        };
        self.content_unlocked(&id)?;
        let mut os = self
            .get_offscreen(&id)
            .ok_or_else(|| Status::error("MISSING_OFFSCREEN", &id))?
            .clone();
        let bound = self
            .binding_for_scene(&os.part_id)
            .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()));
        let count = bound.map(|b| b.track.len());
        if let Some(binding) = bound {
            self.content_unlocked(&binding.id)?;
        }
        let previous = os.keyforms.len();
        let mut remap: Vec<_> = (0..previous).map(Some).collect();
        let mut mapping = os.part_keyform_indices.clone();
        if mapping.is_empty() {
            mapping = (0..count.unwrap_or(1))
                .map(|slot| {
                    os.keyform_index(slot, count)
                        .map(|v| v.map_or(-1, |i| i as i32))
                })
                .collect::<Result<_, _>>()?;
        }
        match edit {
            E::InsertOffscreenKeyform { value, before, .. } => {
                let at = insertion(before, previous)?;
                os.keyforms.insert(at, value);
                for i in &mut remap {
                    if i.unwrap() >= at {
                        *i = Some(i.unwrap() + 1);
                    }
                }
            }
            E::PatchOffscreenKeyform { slot, value, .. } => {
                *os.keyforms
                    .get_mut(slot)
                    .ok_or_else(|| Status::error("INVALID_KEYFORM_SLOT", &id))? = value;
            }
            E::DuplicateOffscreenKeyforms { slots, before, .. } => {
                distinct(&slots)?;
                if slots.iter().any(|&i| i >= previous) {
                    return Err(Status::error("INVALID_KEYFORM_SLOT", &id));
                }
                let copies: Vec<_> = os
                    .keyforms
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| slots.contains(i))
                    .map(|(_, v)| v.clone())
                    .collect();
                let at = insertion(before, previous)?;
                for i in &mut remap {
                    if i.unwrap() >= at {
                        *i = Some(i.unwrap() + copies.len());
                    }
                }
                os.keyforms.splice(at..at, copies);
            }
            E::ReorderOffscreenKeyforms { order, .. } => {
                permutation(&order, previous)?;
                os.keyforms = order.iter().map(|&i| os.keyforms[i].clone()).collect();
                for (to, &from) in order.iter().enumerate() {
                    remap[from] = Some(to);
                }
            }
            E::DeleteOffscreenKeyforms { slots, policy, .. } => {
                distinct(&slots)?;
                if slots.iter().any(|&i| i >= previous) {
                    return Err(Status::error("INVALID_KEYFORM_SLOT", &id));
                }
                let mut next = 0;
                for (i, v) in remap.iter_mut().enumerate() {
                    *v = if slots.contains(&i) {
                        None
                    } else {
                        let at = next;
                        next += 1;
                        Some(at)
                    };
                }
                if let OffscreenDeletePolicy::Remap { replacements } = &policy {
                    if replacements.keys().copied().collect::<HashSet<_>>()
                        != slots.iter().copied().collect()
                        || replacements
                            .values()
                            .flatten()
                            .any(|&i| i >= previous || slots.contains(&i))
                    {
                        return Err(Status::error("INVALID_KEYFORM_REMAP", &id));
                    }
                }
                for m in &mut mapping {
                    if *m >= 0 && slots.contains(&(*m as usize)) {
                        let old = *m;
                        *m = match &policy {
                            OffscreenDeletePolicy::RejectReferences => {
                                return Err(Status::error(
                                    "KEYFORM_REFERENCED",
                                    format!("{}: keyform {}", id, old),
                                ))
                            }
                            OffscreenDeletePolicy::Neutral => -1,
                            OffscreenDeletePolicy::Remap { replacements } => {
                                replacements[&(old as usize)].map_or(-1, |i| i as i32)
                            }
                        };
                        plan.report.losses.push(format!(
                            "Part slot referencing Offscreen keyform {old} now uses {}",
                            *m
                        ));
                    }
                }
                os.keyforms = os
                    .keyforms
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| !slots.contains(i))
                    .map(|(_, v)| v)
                    .collect();
            }
            E::SetOffscreenMapping { updates, .. } => {
                let mut seen = HashSet::new();
                for u in updates {
                    let slot = self.part_keyform_slot(&os.part_id, &u.slot)?;
                    if !seen.insert(slot) {
                        return Err(Status::error("DUPLICATE_SELECTION", &id));
                    }
                    if u.keyform >= 0 && u.keyform as usize >= previous {
                        return Err(Status::error("INVALID_KEYFORM_SLOT", &id));
                    }
                    mapping[slot] = u.keyform;
                }
                os.part_keyform_indices = mapping;
                plan.domain.affected.push(id);
                plan.domain.delta.offscreens.push(os);
                return Ok(());
            }
            _ => unreachable!(),
        }
        // Values are still in the original keyform coordinate system, including
        // explicit replacement targets selected by deletion policy.
        for m in &mut mapping {
            if *m >= 0 {
                *m = remap[*m as usize].map_or(-1, |i| i as i32);
            }
        }
        // Do not materialize an identical implicit mapping for a pure value edit.
        if os.keyforms.len() != previous || remap.iter().enumerate().any(|(i, v)| *v != Some(i)) {
            os.part_keyform_indices = mapping;
        }
        plan.report.keyform_remap = remap;
        plan.domain.affected.push(id);
        plan.domain.delta.offscreens.push(os);
        Ok(())
    }
    pub fn part_keyform_slot(
        &self,
        part_id: &str,
        slot: &PartKeyformSlot,
    ) -> Result<usize, Status> {
        if self.get_part(part_id).is_none() {
            return Err(Status::error("MISSING_PART", part_id));
        }
        let bound = self
            .binding_for_scene(part_id)
            .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()));
        match (bound, slot) {
            (None, PartKeyformSlot::Static) => Ok(0),
            (Some(b), PartKeyformSlot::Samples { samples }) => {
                let crate::SceneTrack::Part { keyforms, .. } = &b.track else {
                    return Err(Status::error("WRONG_PART_SLOT", part_id));
                };
                keyforms
                    .iter()
                    .position(|f| super::mesh::same_samples(&f.samples, samples))
                    .ok_or_else(|| Status::error("MISSING_PART_KEYFORM", part_id))
            }
            _ => Err(Status::error(
                "WRONG_PART_SLOT",
                "Use Static for an unbound Part, otherwise canonical sample IDs",
            )),
        }
    }
}
fn selected(glue: &Glue, ids: &[String]) -> Result<(), Status> {
    distinct(ids)?;
    if ids.iter().any(|id| !glue.pairs.iter().any(|p| p.id == *id)) {
        Err(Status::error("MISSING_GLUE_PAIR", &glue.id))
    } else {
        Ok(())
    }
}
fn anchor(pairs: &[GlueVertexPair], before: &Option<String>) -> Result<usize, Status> {
    match before {
        Some(id) => pairs
            .iter()
            .position(|p| p.id == *id)
            .ok_or_else(|| Status::error("INVALID_PLACEMENT", id)),
        None => Ok(pairs.len()),
    }
}
fn insertion(before: Option<usize>, len: usize) -> Result<usize, Status> {
    let at = before.unwrap_or(len);
    if at > len {
        Err(Status::error(
            "INVALID_KEYFORM_SLOT",
            "Insertion outside keyform array",
        ))
    } else {
        Ok(at)
    }
}

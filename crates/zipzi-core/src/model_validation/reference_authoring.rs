use super::*;
impl Model<'_> {
    pub(crate) fn validate_reference_storage(
        &self,
        model: &Model3Settings,
        info: &DisplayInfo,
    ) -> Result<(), Status> {
        let mut seen = HashSet::new();
        let ids = model
            .groups
            .iter()
            .flatten()
            .flat_map(|g| std::iter::once(&g.id).chain(g.parameters.iter().map(|p| &p.id)))
            .chain(model.hit_areas.iter().flatten().map(|a| &a.id))
            .chain(
                info.combined_parameters
                    .iter()
                    .flatten()
                    .flat_map(|s| std::iter::once(&s.id).chain(s.members.iter().map(|p| &p.id))),
            );
        for id in ids {
            if !valid_uuid(id)
                || !seen.insert(id)
                || (self.contains_id(id) && self.reference_identity(id).is_none())
            {
                return Err(Status::error("INVALID_REFERENCE_IDENTITY", id));
            }
        }
        Ok(())
    }
}

use super::*;

// Replacements guard both the existing object's inherited locks and every new
// owning endpoint. Builder remains responsible for structural validation/tracking.
impl Commands<'_> {
    pub fn replace_mesh(&mut self, value: zipzi_core::Mesh) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(&value.id)?;
            this.unlocked(&value.part_id)?;
            this.unlocked(&value.deformer_id)?;
            this.builder.replace_mesh(value)
        })
    }
    pub fn replace_part(&mut self, value: zipzi_core::Part) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(&value.id)?;
            this.unlocked(&value.parent_id)?;
            this.builder.replace_part(value)
        })
    }
    pub fn replace_transform(&mut self, value: zipzi_core::Transform) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(&value.id)?;
            if let Some(id) = &value.part_id {
                this.unlocked(id.as_str())?;
            }
            if let Some(id) = &value.parent_id {
                this.unlocked(id.as_str())?;
            }
            this.builder.replace_transform(value)
        })
    }
    pub fn replace_offscreen(&mut self, value: zipzi_core::Offscreen) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(&value.id)?;
            this.unlocked(&value.part_id)?;
            this.builder.replace_offscreen(value)
        })
    }
    pub fn replace_glue(&mut self, value: zipzi_core::Glue) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(&value.id)?;
            for id in [value.mesh_a_id.resolved_id(), value.mesh_b_id.resolved_id()]
                .into_iter()
                .flatten()
            {
                this.unlocked(id)?;
            }
            this.builder.replace_glue(value)
        })
    }
    pub fn replace_asset(&mut self, value: zipzi_core::state::AssetRecord) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(&value.id)?;
            this.builder.replace_asset(value)
        })
    }
}

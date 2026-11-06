use super::*;

pub struct Session(zipzi_sdk::AuthoringSession);
impl Session {
    fn empty() -> Result<Self> {
        Ok(Self(zipzi_sdk::AuthoringSession::new(
            "378bcc50-94a5-464f-a210-000000000000",
            zipzi_core::ModelSetup::default(),
        )?))
    }
    pub fn import(path: &Path, _: &Path) -> Result<Self> {
        let mut session = Self::empty()?;
        session.0.import_model3(path, None)?;
        session.0.drain_events();
        Ok(session)
    }
    pub fn open(path: &Path, _: &Path) -> Result<Self> {
        let mut session = Self::empty()?;
        session.0.open_project(path, None)?;
        session.0.drain_events();
        Ok(session)
    }
    pub fn meshes(&self) -> Vec<String> {
        self.0.mesh_ids().to_vec()
    }
    pub fn mesh(&self, id: &str) -> zipzi_core::Mesh {
        self.0.mesh(id).unwrap()
    }
    pub fn assets(&self) -> Vec<String> {
        self.0.asset_ids().to_vec()
    }
    pub fn pixels(&self, id: &str) -> Result<std::sync::Arc<[u8]>> {
        Ok(self.0.read_texture(id)?.rgba)
    }
    pub fn parameter_count(&self) -> usize {
        self.0.parameter_ids().len()
    }
    pub fn motion_count(&self) -> usize {
        self.0.motion_ids().len()
    }
    pub fn dirty(&self) -> bool {
        self.0.modified()
    }
    pub fn history(&self) -> serde_json::Value {
        let h = self.0.history_state();
        json!({"undo":h.undo_steps,"redo":h.redo_steps,"estimated_bytes":h.estimated_bytes,
            "max_steps":h.max_steps,"target_bytes":h.max_bytes})
    }
    pub fn rename(&mut self, id: &str, name: &str) -> Result<()> {
        self.0
            .edit("Rename", None, |edit| edit.rename_mesh(id, name))?;
        self.0.drain_events();
        Ok(())
    }
    pub fn move_vertex(&mut self, id: &str, vertex: u32, position: Vec2) -> Result<()> {
        self.0.edit("Vertex", None, |edit| {
            edit.update_positions(id, &[vertex], &[position])
        })?;
        self.0.drain_events();
        Ok(())
    }
    pub fn create_parameters(&mut self, parameters: Vec<Parameter>) -> Result<()> {
        self.0.edit("Create parameters", None, |edit| {
            for p in parameters {
                edit.create_parameter(p)?;
            }
            Ok(())
        })?;
        self.0.drain_events();
        Ok(())
    }
    pub fn undo(&mut self) -> Result<()> {
        self.0.undo()?;
        self.0.drain_events();
        Ok(())
    }
    pub fn redo(&mut self) -> Result<()> {
        self.0.redo()?;
        self.0.drain_events();
        Ok(())
    }
    pub fn save(&mut self, path: &Path) -> Result<()> {
        self.save_as(path)
    }
    pub fn save_as(&mut self, path: &Path) -> Result<()> {
        assert!(self.0.save_project(path, None)?.durable);
        self.0.drain_events();
        Ok(())
    }
}

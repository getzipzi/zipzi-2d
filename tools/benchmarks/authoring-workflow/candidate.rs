use super::*;
use zipzi_project::{ProjectImporter, ProjectIo, ResourceStore};
use zipzi_sdk::{AuthoringSession, NameTarget, Snapshot};

pub struct Session {
    authoring: AuthoringSession,
    io: ProjectIo,
}
impl Session {
    pub fn import(path: &Path, output: &Path) -> Result<Self> {
        let resources = ResourceStore::default();
        let (report, project, _) = ProjectImporter {
            resources: resources.clone(),
        }
        .import_model3(path);
        if !report.status.is_ok() {
            return Err(report.status.into());
        }
        let project = project.ok_or("Model3 import did not return content")?;
        let authoring = AuthoringSession::new(Snapshot::new(project.document, project.resources)?);
        Ok(Self {
            authoring,
            io: ProjectIo::new(output.join("cache"), resources),
        })
    }
    pub fn open(path: &Path, output: &Path) -> Result<Self> {
        let io = ProjectIo::new(output.join("cache"), ResourceStore::default());
        let (authoring, _) = AuthoringSession::open(&io, path, || true)?;
        Ok(Self { authoring, io })
    }
    fn model(&self) -> &zipzi_core::state::DocumentState {
        self.authoring.snapshot().document()
    }
    pub fn meshes(&self) -> Vec<String> {
        self.model().mesh_order().to_vec()
    }
    pub fn mesh(&self, id: &str) -> zipzi_core::Mesh {
        self.model().mesh(id).unwrap().clone()
    }
    pub fn assets(&self) -> Vec<String> {
        self.model().asset_order().to_vec()
    }
    pub fn pixels(&self, id: &str) -> Result<std::sync::Arc<[u8]>> {
        let asset = self.model().asset(id).unwrap();
        Ok(self
            .authoring
            .snapshot()
            .resources()
            .read_texture(asset)?
            .rgba)
    }
    pub fn parameter_count(&self) -> usize {
        self.model().parameter_order().len()
    }
    pub fn motion_count(&self) -> usize {
        self.model().motion_order().len()
    }
    pub fn dirty(&self) -> bool {
        self.authoring.is_dirty()
    }
    pub fn history(&self) -> serde_json::Value {
        let h = self.authoring.history_state();
        json!({"undo":h.undo_steps,"redo":h.redo_steps,"estimated_bytes":h.estimated_bytes,
            "max_steps":h.limits.max_steps,"target_bytes":h.limits.target_bytes})
    }
    fn edit(
        &mut self,
        label: &str,
        run: impl FnOnce(&mut zipzi_sdk::Commands<'_>) -> std::result::Result<(), zipzi_sdk::Status>,
    ) -> Result<()> {
        let mut draft = self.authoring.begin_draft();
        draft.batch(draft.source(), run)?;
        self.authoring.publish(draft.prepare(label)?)?;
        self.authoring.take_events();
        Ok(())
    }
    pub fn rename(&mut self, id: &str, name: &str) -> Result<()> {
        self.edit("Rename", |edit| {
            edit.rename(NameTarget::Mesh(id), name.into())
        })
    }
    pub fn move_vertex(&mut self, id: &str, vertex: u32, position: Vec2) -> Result<()> {
        self.edit("Vertex", |edit| {
            edit.set_vertex_positions(id, &[(vertex, position)])
        })
    }
    pub fn create_parameters(&mut self, parameters: Vec<Parameter>) -> Result<()> {
        self.edit("Create parameters", |edit| {
            for p in parameters {
                edit.create_parameter(p)?;
            }
            Ok(())
        })
    }
    pub fn undo(&mut self) -> Result<()> {
        self.authoring.undo(self.authoring.version())?;
        self.authoring.take_events();
        Ok(())
    }
    pub fn redo(&mut self) -> Result<()> {
        self.authoring.redo(self.authoring.version())?;
        self.authoring.take_events();
        Ok(())
    }
    pub fn save(&mut self, _: &Path) -> Result<()> {
        assert!(self.authoring.save(&self.io, || true)?.durable);
        Ok(())
    }
    pub fn save_as(&mut self, path: &Path) -> Result<()> {
        assert!(
            self.authoring
                .save_as(&self.io, path, None, || true)?
                .durable
        );
        Ok(())
    }
}

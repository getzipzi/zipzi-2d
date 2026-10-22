use super::*;
use serde_json::json;
impl ObjectIndex {
    /// Caller verifies unchanged identities, collections and reference endpoints.
    pub(crate) fn refresh_pose_properties(&mut self, model: &Model, object: &ObjectRef) -> bool {
        let (name, properties) = match object.kind {
            ObjectKind::Pose => {
                let Some(p) = model.pose() else {
                    return false;
                };
                (
                    "Pose".into(),
                    json!({"fade_in":p.fade_in,"file_type":p.file_type,"extensions":p.extensions}),
                )
            }
            ObjectKind::PoseGroup => {
                let Some(g) = model.get_pose_group(&object.id) else {
                    return false;
                };
                (g.name.clone(), json!({"name":g.name}))
            }
            _ => return false,
        };
        let Some(&slot) = self.structure.slots.get(object) else {
            return false;
        };
        self.objects[slot].name = name;
        self.objects[slot].properties = properties;
        true
    }
}

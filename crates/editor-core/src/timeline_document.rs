use super::Document;
use anyhow::Result;
use kasane_core::document::MotionClip;
use kasane_sdk::{MotionClipCandidate, Version};

impl Document {
    pub(crate) fn commit_timeline_candidate(
        &mut self,
        candidate: MotionClipCandidate,
        version: Version,
        label: &str,
    ) -> Result<()> {
        self.finish_edit()?;
        let (_, receipt) = self.session.edit(label, Some(version), |edit| {
            edit.replace_motion_candidate(candidate)
        })?;
        self.session.drain_events();
        if receipt.changed {
            self.pristine_blank = false;
            self.invalidate_projection(false);
        }
        Ok(())
    }
    pub(crate) fn create_timeline_clip(&mut self) -> Result<String> {
        self.finish_edit()?;
        let names: std::collections::HashSet<_> = self
            .model()
            .motion_order()
            .iter()
            .map(|id| self.model().get_motion(id).unwrap().name.as_str())
            .collect();
        let name = std::iter::once("New Motion".to_owned())
            .chain((2u64..).map(|i| format!("New Motion {i}")))
            .find(|name| !names.contains(name.as_str()))
            .unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        self.session.edit("New motion", None, |edit| {
            edit.create_motion(MotionClip {
                id: id.clone(),
                name,
                duration: 2.,
                fps: 30.,
                looping: false,
                restricted_beziers: false,
                fade_in: Some(0.),
                fade_out: Some(0.),
                tracks: vec![],
                events: vec![],
                extensions: Default::default(),
                meta_extensions: Default::default(),
                opaque_source_ids: None,
                opaque_source_content_hash: None,
            })
        })?;
        self.session.drain_events();
        self.pristine_blank = false;
        self.invalidate_projection(false);
        Ok(id)
    }
}

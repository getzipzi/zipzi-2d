//! Motion and expression resource descriptions.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MotionSelection {
    pub clip_id: String,
    /// Stable registration identity; None requests the Clip root registration.
    pub entry: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MotionResource {
    pub selection: MotionSelection,
    pub name: String,
    pub group: Option<String>,
    pub group_id: Option<String>,
    pub entry_index: usize,
    pub duration: f32,
    pub looping: bool,
    pub sound: Option<String>,
    pub fps: f32,
    /// Effective registration → clip → SDK default fades, in seconds.
    pub fade_in: f32,
    pub fade_out: f32,
    pub tracks: usize,
    pub events: usize,
}
impl MotionResource {
    pub fn playable(&self) -> bool {
        self.duration.is_finite() && self.duration > 0. && self.duration * 60. <= 1_000_000.
    }
}

#[derive(Clone, Debug)]
pub struct ExpressionResource {
    pub id: String,
    pub name: String,
    pub unresolved_targets: Vec<String>,
    pub disabled_entries: Vec<zipzi_core::document::DisabledExpressionEntry>,
    pub fade_in: f32,
    pub fade_out: f32,
    pub parameters: usize,
    pub blends: Vec<zipzi_core::document::ExpressionBlend>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExpressionTrigger {
    pub expression_id: String,
    pub time: f32,
    pub transition: ExpressionTransition,
}

impl Document {
    pub fn expression_resources(&self) -> Vec<ExpressionResource> {
        use zipzi_core::document::ExpressionTarget;
        let model = self.model();
        model
            .expression_order()
            .iter()
            .filter_map(|id| {
                let expression = model.get_expression(id)?;
                Some(ExpressionResource {
                    id: id.clone(),
                    name: expression.name.clone(),
                    disabled_entries: model
                        .expression_projection(id)
                        .expect("validated expression projects")
                        .disabled,
                    fade_in: expression.fade_in.unwrap_or(1.),
                    fade_out: expression.fade_out.unwrap_or(1.),
                    parameters: expression.entries.len(),
                    blends: [
                        zipzi_core::document::ExpressionBlend::Add,
                        zipzi_core::document::ExpressionBlend::Multiply,
                        zipzi_core::document::ExpressionBlend::Overwrite,
                    ]
                    .into_iter()
                    .filter(|blend| {
                        expression.entries.iter().any(|entry| {
                            entry
                                .blend
                                .unwrap_or(zipzi_core::document::ExpressionBlend::Add)
                                == *blend
                        })
                    })
                    .collect(),
                    unresolved_targets: expression
                        .entries
                        .iter()
                        .filter_map(|entry| match &entry.target {
                            ExpressionTarget::Unresolved { runtime_id } => Some(runtime_id.clone()),
                            _ => None,
                        })
                        .collect(),
                })
            })
            .collect()
    }
    pub fn motion_resources(&self) -> Vec<MotionResource> {
        let model = self.model();
        let mut result = Vec::new();
        for (group_id, group, entries) in model
            .motion_groups()
            .iter()
            .map(|g| {
                (
                    Some(g.id.clone()),
                    Some(g.name.clone()),
                    g.entries.as_slice(),
                )
            })
            .chain(std::iter::once((None, None, model.motion_roots())))
        {
            for (index, entry) in entries.iter().enumerate() {
                if let Some(clip) = model.get_motion(&entry.clip_id) {
                    result.push(MotionResource {
                        selection: MotionSelection {
                            clip_id: clip.id.clone(),
                            entry: Some(entry.id.clone()),
                        },
                        group: group.clone(),
                        group_id: group_id.clone(),
                        entry_index: index,
                        name: clip.name.clone(),
                        duration: clip.duration,
                        looping: clip.looping,
                        sound: entry
                            .sound
                            .as_ref()
                            .and_then(|r| model.attachment_path(r))
                            .map(str::to_owned),
                        fps: clip.fps,
                        fade_in: entry.fade_in.or(clip.fade_in).unwrap_or(1.),
                        fade_out: entry.fade_out.or(clip.fade_out).unwrap_or(1.),
                        tracks: clip.tracks.len(),
                        events: clip.events.len(),
                    });
                }
            }
        }
        result
    }
}

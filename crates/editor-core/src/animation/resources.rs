//! Motion and expression resource descriptions.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MotionSelection {
    pub clip_id: String,
    /// Keep both registration position and clip identity. A reordered entry
    /// must never silently play a different clip.
    pub entry: Option<(String, usize)>,
}

#[derive(Clone, Debug)]
pub struct MotionResource {
    pub selection: MotionSelection,
    pub name: String,
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
    pub fade_in: f32,
    pub fade_out: f32,
    pub parameters: usize,
    pub blends: Vec<kasane_core::document::ExpressionBlend>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExpressionTrigger {
    pub expression_id: String,
    pub time: f32,
    pub transition: ExpressionTransition,
}

impl Document {
    pub fn expression_resources(&self) -> Vec<ExpressionResource> {
        use kasane_core::document::ExpressionTarget;
        let model = self.model();
        model
            .expression_order()
            .iter()
            .filter_map(|id| {
                let expression = model.get_expression(id)?;
                Some(ExpressionResource {
                    id: id.clone(),
                    name: expression.name.clone(),
                    fade_in: expression.fade_in.unwrap_or(1.),
                    fade_out: expression.fade_out.unwrap_or(1.),
                    parameters: expression.entries.len(),
                    blends: [
                        kasane_core::document::ExpressionBlend::Add,
                        kasane_core::document::ExpressionBlend::Multiply,
                        kasane_core::document::ExpressionBlend::Overwrite,
                    ]
                    .into_iter()
                    .filter(|blend| {
                        expression.entries.iter().any(|entry| {
                            entry
                                .blend
                                .unwrap_or(kasane_core::document::ExpressionBlend::Add)
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
        let mut registered = HashSet::new();
        for group in model.motion_groups() {
            for (index, entry) in group.entries.iter().enumerate() {
                if let Some(clip) = model.get_motion(&entry.clip_id) {
                    registered.insert(clip.id.clone());
                    result.push(MotionResource {
                        selection: MotionSelection {
                            clip_id: clip.id.clone(),
                            entry: Some((group.name.clone(), index)),
                        },
                        name: clip.name.clone(),
                        duration: clip.duration,
                        looping: clip.looping,
                        sound: entry.sound.clone(),
                        fps: clip.fps,
                        fade_in: entry.fade_in.or(clip.fade_in).unwrap_or(1.),
                        fade_out: entry.fade_out.or(clip.fade_out).unwrap_or(1.),
                        tracks: clip.tracks.len(),
                        events: clip.events.len(),
                    });
                }
            }
        }
        for id in model.motion_order() {
            if !registered.contains(id)
                && let Some(clip) = model.get_motion(id)
            {
                result.push(MotionResource {
                    selection: MotionSelection {
                        clip_id: id.clone(),
                        entry: None,
                    },
                    name: clip.name.clone(),
                    duration: clip.duration,
                    looping: clip.looping,
                    sound: None,
                    fps: clip.fps,
                    fade_in: clip.fade_in.unwrap_or(1.),
                    fade_out: clip.fade_out.unwrap_or(1.),
                    tracks: clip.tracks.len(),
                    events: clip.events.len(),
                });
            }
        }
        result
    }
}

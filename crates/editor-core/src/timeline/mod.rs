//! Timeline projections and session state. Persistent edits belong to the SDK.
mod projection;
pub use projection::*;
mod editing;
pub use editing::*;
mod identity;
pub(crate) use identity::TimelinePanels;
pub use identity::*;
mod clipboard;
pub use clipboard::TimelinePasteTarget;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TimelineChannelId {
    Parameter(String),
    Model(String),
    PartOpacity(String),
    Unresolved {
        category: String,
        runtime_id: String,
    },
}

impl TimelineChannelId {
    pub fn target(&self) -> kasane_core::document::MotionTrackTarget {
        use kasane_core::document::MotionTrackTarget as T;
        match self {
            Self::Parameter(id) => T::Parameter {
                parameter_id: id.clone(),
            },
            Self::Model(id) => T::Model {
                runtime_id: id.clone(),
            },
            Self::PartOpacity(id) => T::PartOpacity {
                part_id: id.clone(),
            },
            Self::Unresolved {
                category,
                runtime_id,
            } => T::Unresolved {
                category: category.clone(),
                runtime_id: runtime_id.clone(),
            },
        }
    }
    pub fn from_target(target: &kasane_core::document::MotionTrackTarget) -> Self {
        use kasane_core::document::MotionTrackTarget as T;
        match target {
            T::Parameter { parameter_id } => Self::Parameter(parameter_id.clone()),
            T::Model { runtime_id } => Self::Model(runtime_id.clone()),
            T::PartOpacity { part_id } => Self::PartOpacity(part_id.clone()),
            T::Unresolved {
                category,
                runtime_id,
            } => Self::Unresolved {
                category: category.clone(),
                runtime_id: runtime_id.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TimelineCategory {
    Model,
    Parts,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TimelineRowId {
    Group(Option<String>),
    Category(TimelineCategory),
    Channel(TimelineChannelId),
    Track(String),
    Events,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimelineKeyRef {
    pub clip_id: String,
    pub track_id: String,
    pub key_id: String,
}
impl TimelineKeyRef {
    pub fn sdk(&self) -> kasane_sdk::MotionKeyRef {
        kasane_sdk::MotionKeyRef {
            track_id: self.track_id.clone(),
            key_id: self.key_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TimelineDisplayMode {
    #[default]
    Overview,
    Curves,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TimelineFilter {
    #[default]
    Animated,
    All,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AnimationPreviewMode {
    Editing,
    #[default]
    Result,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineViewState {
    pub collapsed: HashSet<TimelineRowId>,
    pub search: String,
    pub filter: TimelineFilter,
    pub mode: TimelineDisplayMode,
    pub time_range: (f64, f64),
    pub scroll: f64,
    pub normalized: bool,
    pub snap: bool,
    pub channels: BTreeSet<TimelineRowId>,
    /// The channel whose units the curve ruler displays, without hiding peers.
    pub curve_focus: Option<TimelineChannelId>,
    pub keys: BTreeSet<TimelineKeyRef>,
    pub events: BTreeSet<String>,
    pub reveal_row: Option<TimelineRowId>,
}
impl Default for TimelineViewState {
    fn default() -> Self {
        Self {
            collapsed: HashSet::new(),
            search: String::new(),
            filter: TimelineFilter::Animated,
            mode: TimelineDisplayMode::Overview,
            time_range: (0., 2.),
            scroll: 0.,
            normalized: true,
            snap: true,
            channels: BTreeSet::new(),
            curve_focus: None,
            keys: BTreeSet::new(),
            events: BTreeSet::new(),
            reveal_row: None,
        }
    }
}

impl TimelineViewState {
    /// Keep the current zoom while bringing an explicitly located time into view.
    pub fn reveal_time(&mut self, time: f64) {
        let (left, right) = self.time_range;
        let width = right - left;
        let margin = width * 0.05;
        if time < left + margin || time > right - margin {
            let left = (time - width / 2.).max(0.);
            self.time_range = (left, left + width);
        }
    }
}

pub(crate) struct TimelineClipState {
    pub view: TimelineViewState,
    pub projection: Arc<TimelineProjection>,
    pub visible: Arc<[TimelineRow]>,
    pub generation: u64,
}
impl TimelineClipState {
    fn reveal(&mut self, keys: &[TimelineKeyRef]) {
        let mut ancestors: Vec<&TimelineRow> = Vec::new();
        let mut channels = BTreeSet::new();
        for row in self.projection.rows.iter() {
            while ancestors
                .last()
                .is_some_and(|parent| parent.depth >= row.depth)
            {
                ancestors.pop();
            }
            if row.keys.iter().any(|key| keys.contains(key)) {
                channels.insert(row.id.clone());
                for parent in &ancestors {
                    self.view.collapsed.remove(&parent.id);
                }
                self.view.reveal_row = Some(row.id.clone());
            }
            ancestors.push(row);
        }
        self.view.keys = keys.iter().cloned().collect();
        self.view.events.clear();
        self.view.channels = channels;
        if !self
            .projection
            .visible_rows(&self.view)
            .iter()
            .any(|row| row.keys.iter().any(|key| keys.contains(key)))
        {
            self.view.search.clear();
        }
        if let Some(key) = keys.first().and_then(|key| self.projection.key(key)) {
            self.view.reveal_time(key.time);
        }
        self.refresh_visible();
    }
    fn refresh_visible(&mut self) {
        self.visible = self.projection.visible_rows(&self.view).into();
        self.retain_selection();
    }
    fn retain_selection(&mut self) {
        let tracks: HashSet<_> = self
            .visible
            .iter()
            .filter_map(|row| row.track_id.as_ref())
            .collect();
        self.view
            .keys
            .retain(|key| tracks.contains(&key.track_id) && self.projection.key(key).is_some());
        self.view
            .events
            .retain(|id| self.projection.events.iter().any(|event| &event.id == id));
        let ids: HashSet<_> = self.visible.iter().map(|row| &row.id).collect();
        self.view.channels.retain(|row| ids.contains(row));
        if self.view.curve_focus.as_ref().is_some_and(|channel| {
            !self.visible.iter().any(|row| {
                self.view.channels.contains(&row.id) && row.channel.as_ref() == Some(channel)
            })
        }) {
            self.view.curve_focus = None;
        }
    }
}

#[derive(Default)]
pub(crate) struct TimelineSession {
    pub clips: HashMap<String, TimelineClipState>,
    pub active: Option<String>,
    pub edit: Option<TimelineEdit>,
    pub clipboard: Option<clipboard::TimelineClipboard>,
    pub next_edit: u64,
    pub binding_generation: u64,
}
impl TimelineSession {
    pub fn reconcile(&mut self, document: &crate::Document, active: Option<&str>) {
        self.clips
            .retain(|id, _| document.motion_definition(id).is_some());
        let active = active
            .filter(|id| document.motion_definition(id).is_some())
            .map(str::to_owned);
        if self.active != active {
            self.binding_generation += 1;
        }
        self.active = active;
        let Some(id) = &self.active else {
            return;
        };
        let generation = document.model_generation();
        let state = self.clips.entry(id.clone()).or_insert_with(|| {
            let projection =
                Arc::new(TimelineProjection::new(document, id).expect("valid active clip"));
            let view = TimelineViewState {
                time_range: (0., projection.display_end),
                ..Default::default()
            };
            let visible = projection.visible_rows(&view).into();
            TimelineClipState {
                view,
                projection,
                visible,
                generation,
            }
        });
        if state.generation != generation {
            state.projection =
                Arc::new(TimelineProjection::new(document, id).expect("valid active clip"));
            state.generation = generation;
            state.refresh_visible();
        }
    }
    pub fn current(&self) -> Option<&TimelineClipState> {
        self.clips.get(self.active.as_ref()?)
    }
    pub fn current_mut(&mut self) -> Option<&mut TimelineClipState> {
        self.clips.get_mut(self.active.as_ref()?)
    }
}

/// Frame snapping is a command decision. Imported times are never quantized by projection.
pub fn timeline_time(time: f64, fps: f32, snap: bool) -> anyhow::Result<f32> {
    anyhow::ensure!(
        time.is_finite() && fps.is_finite() && fps > 0.,
        "Invalid timeline time or FPS"
    );
    let time = if snap {
        (time * f64::from(fps)).round() / f64::from(fps)
    } else {
        time
    };
    let time = time as f32;
    anyhow::ensure!(time.is_finite(), "Time is outside f32 precision");
    Ok(time)
}

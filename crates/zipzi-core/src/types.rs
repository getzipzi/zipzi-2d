use serde::{Deserialize, Serialize};

pub use zipzi_contract::model::{
    Appearance, BlendMode, Canvas, CoordinateSpace, ExportSettings, ImageAsset, ImageContent,
    ModelSetup, PreciseVec2, RotationPose, Status, TransformKind, Vec2, VertexId,
};

pub use zipzi_contract::model::{
    Part, PartKeyform, RotationKeyform, RotationTransform, SceneKeyform, Transform, TransformData,
    WarpKeyform, WarpTransform,
};

/// Borrowed projection for interpolation and external formats. Neutral values
/// are synthesized, never stored in the document as irrelevant source fields.
pub struct SceneSample<'a> {
    pub keys: &'a [f32],
    pub positions: &'a [Vec2],
    pub rotation: RotationPose,
    pub appearance: Appearance,
    pub draw_order: f32,
}

pub use zipzi_contract::model::Mesh;

pub use zipzi_contract::model::{MeshConstraint, MeshConstraintKind, MeshMeshing};

pub use zipzi_contract::model::{Parameter, ParameterKind};

pub use zipzi_contract::model::{
    BindingAxis, BindingSample, BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable,
    BlendShapeTargetKind, ConstraintSample, DeltaGlueKeyform, DeltaKeyforms, DeltaMeshKeyform,
    DeltaOffscreenKeyform, DeltaPartKeyform, DeltaRotationKeyform, DeltaWarpKeyform, Glue,
    GlueBinding, GlueKeyform, GlueVertexPair,
};

pub trait BindingAxisEditing: Sized {
    fn new(parameter_id: impl Into<String>, keys: Vec<f32>) -> Self;
    fn set_values(&mut self, values: Vec<f32>);
}
impl BindingAxisEditing for BindingAxis {
    fn new(parameter_id: impl Into<String>, keys: Vec<f32>) -> Self {
        let samples = keys
            .iter()
            .map(|&value| BindingSample {
                id: uuid::Uuid::new_v4().to_string(),
                value,
            })
            .collect();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            parameter_id: parameter_id.into(),
            keys,
            samples,
        }
    }
    fn set_values(&mut self, values: Vec<f32>) {
        assert_eq!(
            values.len(),
            self.samples.len(),
            "Use a grid conversion to change sample count"
        );
        for (sample, &value) in self.samples.iter_mut().zip(&values) {
            sample.value = value;
        }
        self.keys = values;
    }
}
pub use zipzi_contract::model::{MeshBinding, MeshKeyform, Offscreen, OffscreenKeyform};

pub trait OffscreenLookup {
    fn keyform_index(
        &self,
        slot: usize,
        bound_key_count: Option<usize>,
    ) -> Result<Option<usize>, Status>;
}
impl OffscreenLookup for Offscreen {
    /// Resolve a Part key slot. None is a neutral appearance, including negative
    /// sentinels. Only an unbound Part may implicitly use static keyform zero.
    fn keyform_index(
        &self,
        slot: usize,
        bound_key_count: Option<usize>,
    ) -> Result<Option<usize>, Status> {
        let count = bound_key_count.unwrap_or(1);
        if slot >= count
            || (!self.part_keyform_indices.is_empty() && self.part_keyform_indices.len() != count)
        {
            return Err(Status::error("INVALID_LENGTH", &self.id));
        }
        let index = self
            .part_keyform_indices
            .get(slot)
            .copied()
            .unwrap_or_else(|| {
                if bound_key_count.is_none() && !self.keyforms.is_empty() {
                    0
                } else {
                    -1
                }
            });
        if index < 0 {
            return Ok(None);
        }
        if index as usize >= self.keyforms.len() {
            return Err(Status::error("INDEX_OUT_OF_BOUNDS", &self.id));
        }
        Ok(Some(index as usize))
    }
}

pub use zipzi_contract::model::{PartId, TransformId};

pub use zipzi_contract::model::SceneTrack;

pub trait SceneTrackEvaluation {
    fn sample(&self, index: usize) -> SceneSample<'_>;
    fn reorder(&mut self, order: &[usize]);
    fn replace(&mut self, form: SceneKeyform) -> Status;
    fn samples(&self) -> impl ExactSizeIterator<Item = SceneSample<'_>>;
}
impl SceneTrackEvaluation for SceneTrack {
    fn sample(&self, index: usize) -> SceneSample<'_> {
        match self {
            Self::Warp { keyforms, .. } => {
                let f = &keyforms[index];
                SceneSample {
                    keys: &f.keys,
                    positions: &f.positions,
                    rotation: RotationPose::default(),
                    appearance: f.appearance,
                    draw_order: 0.0,
                }
            }
            Self::Rotation { keyforms, .. } => {
                let f = &keyforms[index];
                SceneSample {
                    keys: &f.keys,
                    positions: &[],
                    rotation: f.rotation,
                    appearance: f.appearance,
                    draw_order: 0.0,
                }
            }
            Self::Part { keyforms, .. } => {
                let f = &keyforms[index];
                SceneSample {
                    keys: &f.keys,
                    positions: &[],
                    rotation: RotationPose::default(),
                    appearance: Appearance::default(),
                    draw_order: f.draw_order,
                }
            }
        }
    }
    fn reorder(&mut self, order: &[usize]) {
        fn apply<T>(values: &mut Vec<T>, order: &[usize]) {
            let mut old: Vec<_> = values.drain(..).map(Some).collect();
            values.extend(order.iter().map(|&i| old[i].take().unwrap()));
        }
        match self {
            Self::Warp { keyforms, .. } => apply(keyforms, order),
            Self::Rotation { keyforms, .. } => apply(keyforms, order),
            Self::Part { keyforms, .. } => apply(keyforms, order),
        }
    }
    fn replace(&mut self, form: SceneKeyform) -> Status {
        fn update<T>(forms: &mut [T], form: T, keys: impl Fn(&T) -> &[f32]) -> Status {
            if let Some(slot) = forms.iter_mut().find(|v| keys(v) == keys(&form)) {
                *slot = form;
                Status::ok()
            } else {
                Status::error("INVALID_KEY_COMBINATION", "No matching keyform")
            }
        }
        match (self, form) {
            (Self::Warp { keyforms, .. }, SceneKeyform::Warp(f)) => {
                update(keyforms, f, |f| &f.keys)
            }
            (Self::Rotation { keyforms, .. }, SceneKeyform::Rotation(f)) => {
                update(keyforms, f, |f| &f.keys)
            }
            (Self::Part { keyforms, .. }, SceneKeyform::Part(f)) => {
                update(keyforms, f, |f| &f.keys)
            }
            _ => Status::error("INVALID_KEYFORM_TYPE", "Keyform must match its track"),
        }
    }
    fn samples(&self) -> impl ExactSizeIterator<Item = SceneSample<'_>> {
        (0..self.len()).map(|i| self.sample(i))
    }
}

pub use zipzi_contract::model::SceneBinding;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VertexMapping {
    pub new_id: VertexId,
    pub old_id: Option<VertexId>,
}

pub use zipzi_contract::model::ChangeKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChangeSet {
    pub kind: ChangeKind,
    pub mesh_ids: Vec<String>,
    pub revision: u64,
    pub object_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EditResult {
    pub status: Status,
    pub changes: ChangeSet,
    pub referrers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VertexPositionUpdate {
    pub mesh_id: String,
    pub vertex_ids: Vec<VertexId>,
    pub positions: Vec<Vec2>,
}

/// A validated edit that can be staged in a document transaction.
///
/// New atomic edit kinds can be added here without adding another parallel
/// transaction API. A failed commit applies none of the staged edits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DocumentEdit {
    VertexPositions(VertexPositionUpdate),
    MeshName { mesh_id: String, name: String },
}

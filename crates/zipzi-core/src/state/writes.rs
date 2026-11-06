/// Persistent record collections; writes never use string field patches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum RecordDomain {
    Asset,
    Parameter,
    Part,
    Transform,
    Mesh,
    Binding,
    SceneBinding,
    BlendKeyTable,
    BlendConstraint,
    BlendBinding,
    Glue,
    Offscreen,
    Expression,
    Motion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum RecordChange {
    Created,
    Replaced,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RecordWrite {
    pub domain: RecordDomain,
    pub id: String,
    pub change: RecordChange,
}

/// Net writes, derived solely from typed writes and their touched records.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WriteSet {
    pub(super) orders: std::collections::BTreeSet<RecordDomain>,
    pub(super) roots: std::collections::BTreeSet<RootDomain>,
    pub(super) estimated_bytes: usize,
    pub(super) records: Vec<RecordWrite>,
    pub(super) setup: bool,
    pub(super) assets_order: bool,
    pub(super) parameters_order: bool,
    #[serde(skip)]
    pub(super) animation_inputs_changed: bool,
}

impl WriteSet {
    /// The same touched domains in the opposite publication direction.
    /// The history estimate belongs to the retained step, so it is unchanged.
    pub fn reversed(&self) -> Self {
        let mut reversed = self.clone();
        for record in &mut reversed.records {
            if matches!(
                record.domain,
                RecordDomain::Motion | RecordDomain::Expression
            ) && record.change == RecordChange::Created
            {
                reversed.animation_inputs_changed = true;
            }
            record.change = match record.change {
                RecordChange::Created => RecordChange::Deleted,
                RecordChange::Deleted => RecordChange::Created,
                RecordChange::Replaced => RecordChange::Replaced,
            };
        }
        reversed
    }
    /// Fixed conservative step estimate; never an admission limit.
    pub fn estimated_bytes(&self) -> usize {
        self.estimated_bytes
    }
    pub fn root_changed(&self, domain: RootDomain) -> bool {
        self.roots.contains(&domain)
    }
    pub fn records(&self) -> &[RecordWrite] {
        &self.records
    }
    /// Existing playback recipes must be rebuilt when their evaluated inputs change.
    pub fn animation_inputs_changed(&self) -> bool {
        self.animation_inputs_changed
    }
    pub fn setup_changed(&self) -> bool {
        self.setup
    }
    pub fn order_changed(&self, domain: RecordDomain) -> bool {
        match domain {
            RecordDomain::Asset => self.assets_order,
            RecordDomain::Parameter => self.parameters_order,
            _ => self.orders.contains(&domain),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
            && !self.setup
            && !self.assets_order
            && !self.parameters_order
            && self.orders.is_empty()
            && self.roots.is_empty()
    }
}

/// Independently shared persistent singleton/metadata roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum RootDomain {
    DrawOrderGroups,
    DisplayInfo,
    HierarchyOrder,
    EditorState,
    MotionGroups,
    MotionRoots,
    Pose,
    Physics,
    MissingAttachments,
    Model3Settings,
    PackageAttachments,
}

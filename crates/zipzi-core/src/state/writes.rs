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
}

impl WriteSet {
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

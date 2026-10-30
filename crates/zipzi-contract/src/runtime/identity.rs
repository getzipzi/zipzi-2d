#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct RuntimeInstanceId(Uuid);
impl RuntimeInstanceId {
    pub fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
}
impl std::fmt::Display for RuntimeInstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::str::FromStr for RuntimeInstanceId {
    type Err = uuid::Error;
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

macro_rules! ids {
    ($($name:ident),*) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        #[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiFields)]
        pub struct $name {
            /// Runtime instance that allocated this handle; handles are invalid across instances.
            pub(crate) runtime: RuntimeInstanceId,
            /// Runtime-allocated serial, meaningful only together with the instance and handle kind.
            pub(crate) serial: u64,
        }
        impl crate::api_shape::ApiShape for $name {
            fn api_shape() -> crate::api_shape::Shape {
                crate::api_shape::Shape::Opaque { name: concat!(module_path!(), "::", stringify!($name)).into() }
            }
        }
        impl $name {
            /// Construct an identity. Runtime still validates existence and ownership.
            pub fn from_parts(runtime: RuntimeInstanceId, serial: u64) -> Self { Self { runtime, serial } }
            pub fn runtime_instance(&self) -> RuntimeInstanceId { self.runtime }
            pub fn serial(&self) -> u64 { self.serial }
        }
    )*};
}
ids!(
    SessionId,
    ClientId,
    EditId,
    JobId,
    OperationId,
    PlanId,
    ImageId,
    BlobId,
    ObjectRefId,
    FieldAddressId,
    PreviewId
);
impl From<EditId> for OperationId {
    fn from(id: EditId) -> Self {
        Self {
            runtime: id.runtime,
            serial: id.serial,
        }
    }
}
impl From<JobId> for OperationId {
    fn from(id: JobId) -> Self {
        Self {
            runtime: id.runtime,
            serial: id.serial,
        }
    }
}

/// The first R1 source kinds. Candidate sequence distinguishes frames even if
/// the formal SDK version did not change. Snapshot/Proposal tokens arrive later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
pub enum SourceRef {
    Formal {
        session: SessionId,
        version: Version,
    },
    Edit {
        session: SessionId,
        edit: EditId,
        base: Version,
        sequence: u64,
    },
}

impl SourceRef {
    pub fn session(self) -> SessionId {
        match self {
            Self::Formal { session, .. } | Self::Edit { session, .. } => session,
        }
    }
    pub fn version(self) -> Version {
        match self {
            Self::Formal { version, .. } => version,
            Self::Edit { base, .. } => base,
        }
    }
}

impl crate::api_shape::ApiShape for SourceRef {
    fn api_shape() -> crate::api_shape::Shape {
        crate::api_shape::Shape::Opaque {
            name: concat!(module_path!(), "::SourceRef").into(),
        }
    }
}

impl crate::binary_wire::WireWrite for RuntimeInstanceId {
    const SIGNATURE: u128 = crate::binary_wire::signature("runtime-instance-uuid-v1");
    fn encode<W: std::io::Write>(&self, out: &mut W) -> crate::binary_wire::Result<()> {
        out.write_all(self.0.as_bytes())?;
        Ok(())
    }
    fn expansion_bytes(&self) -> usize {
        128
    }
}
impl crate::binary_wire::WireRead for RuntimeInstanceId {
    fn input_cost(input: &mut crate::binary_wire::Reader<'_>) -> crate::binary_wire::Result<usize> {
        input.take(16)?;
        Ok(16)
    }
    fn decode(input: &mut crate::binary_wire::Reader<'_>) -> crate::binary_wire::Result<Self> {
        Ok(Self(Uuid::from_bytes(input.take(16)?.try_into().unwrap())))
    }
}

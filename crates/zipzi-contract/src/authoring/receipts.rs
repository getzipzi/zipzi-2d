#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkError {
    pub code: Box<str>,
    pub message: Box<str>,
    pub operation: &'static str,
    pub object_ids: Vec<String>,
    pub field_path: Option<Box<str>>,
    pub expected_version: Option<Box<Version>>,
    pub actual_version: Option<Box<Version>>,
    pub referrers: Box<[String]>,
}

impl std::fmt::Display for SdkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for SdkError {}

impl SdkError {
    pub fn new(code: &str, message: &str, operation: &'static str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            operation,
            object_ids: Vec::new(),
            field_path: None,
            expected_version: None,
            actual_version: None,
            referrers: Box::new([]),
        }
    }
    pub fn from_status(
        status: crate::model::Status,
        operation: &'static str,
        ids: Vec<String>,
    ) -> Self {
        Self {
            code: status.code.into_boxed_str(),
            message: status.message.into_boxed_str(),
            operation,
            object_ids: ids,
            field_path: None,
            expected_version: None,
            actual_version: None,
            referrers: Box::new([]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditReceipt {
    pub label: String,
    pub before: Version,
    pub after: Version,
    pub kind: crate::model::ChangeKind,
    pub object_ids: Vec<String>,
    pub changed: bool,
    pub changes: crate::changes::ChangeSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveReceipt {
    pub before: Version,
    pub after: Version,
    pub manifest: PathBuf,
    pub warnings: Vec<String>,
    pub history_warnings: Vec<String>,
    pub durable: bool,
}

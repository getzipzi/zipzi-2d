/// Identity of the complete finalized native project archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
pub struct ProjectFingerprint {
    pub sha256: String,
    pub length: u64,
}

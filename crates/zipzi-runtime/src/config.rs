/// R0 defaults for the objects enabled in R1. Estimates conservatively count
/// shared document content per holder; these are not RSS or GPU limits.
#[derive(Debug, Clone, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct RuntimeLimits {
    /// Maximum independent frozen animation Previews owned by one client.
    pub previews_per_client: usize,
    /// Conservative aggregate Preview storage limit, including replay/cache workspace.
    pub preview_bytes: usize,
    /// Maximum retained activations per Preview; Reset retains schedules.
    pub preview_schedule_entries: usize,
    /// SDK checkpoint-cache budget per Preview; transient replay state is charged separately.
    pub preview_seek_cache_bytes: usize,
    /// Maximum records returned by one collection page or selected batch.
    pub collection_records: usize,
    /// Maximum escaped metadata bytes in one collection page.
    pub collection_bytes: usize,
    /// Maximum entries examined by one collection page.
    pub collection_scan: usize,
    /// Maximum registered clients; this is a count, not a byte limit.
    pub clients: usize,
    /// Maximum resident document Sessions; this is a count, not a byte limit.
    pub sessions: usize,
    /// Maximum simultaneous candidates per client/Session pair; this is a count, not a byte limit.
    pub edits_per_client_session: usize,
    /// Maximum simultaneous candidates per Session; this is a count, not a byte limit.
    pub edits_per_session: usize,
    /// Maximum simultaneous candidates in this Runtime; this is a count, not a byte limit.
    pub edits_total: usize,
    /// Conservative byte limit for candidate document storage; not RSS or GPU memory.
    pub candidate_bytes: usize,
    /// Conservative byte limit for frozen snapshots and queued work; not RSS or GPU memory.
    pub frozen_bytes: usize,
    /// Conservative byte limit for aggregate Runtime-accounted storage; not RSS or GPU memory.
    pub accounted_bytes: usize,
    /// Conservative byte limit for retained image storage; not RSS or GPU memory.
    pub image_bytes: usize,
    /// Maximum retained plans per client; this is a count, not a byte limit.
    pub plans_per_client: usize,
    /// Maximum retained object/field references per client; this is a count, not a byte limit.
    pub references_per_client: usize,
    /// Maximum prepared images per client; this is a count, not a byte limit.
    pub prepared_images_per_client: usize,
    /// Conservative byte limit for retained plan storage; not RSS or GPU memory.
    pub plan_bytes: usize,
    /// Conservative byte limit for retained binary blob storage; not RSS or GPU memory.
    pub blob_bytes: usize,
    /// Maximum retained binary blobs per client; this is a count, not a byte limit.
    pub blobs_per_client: usize,
    /// Conservative byte limit for retained undo/redo history; not RSS or GPU memory.
    pub history_bytes: usize,
    /// Maximum retained Jobs in the Runtime; this is a count, not a byte limit.
    pub jobs_total: usize,
    /// Maximum retained Jobs per client; this is a count, not a byte limit.
    pub jobs_per_client: usize,
    /// Maximum concurrently running host Jobs; this is a count, not a byte limit.
    pub running_jobs: usize,
    /// Maximum retained events per Session stream; this is a count, not a byte limit.
    pub session_events: usize,
    /// Conservative byte limit for event payloads per Session stream; not RSS or GPU memory.
    pub session_event_bytes: usize,
    /// Maximum retained events in the registry stream; this is a count, not a byte limit.
    pub registry_events: usize,
    /// Conservative byte limit for registry event payloads; not RSS or GPU memory.
    pub registry_event_bytes: usize,
}
impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            previews_per_client: 8,
            preview_bytes: 128 << 20,
            preview_schedule_entries: 4096,
            preview_seek_cache_bytes: 2 << 20,
            collection_records: 256,
            collection_bytes: 256 << 10,
            collection_scan: 16384,
            clients: 8,
            sessions: 32,
            edits_per_client_session: 1,
            edits_per_session: 4,
            edits_total: 16,
            candidate_bytes: 512 << 20,
            frozen_bytes: 256 << 20,
            accounted_bytes: usize::try_from(8u64 << 30).unwrap_or(usize::MAX),
            image_bytes: usize::try_from(4u64 << 30).unwrap_or(usize::MAX),
            plans_per_client: 64,
            references_per_client: 1024,
            prepared_images_per_client: 64,
            plan_bytes: 128 << 20,
            blob_bytes: 512 << 20,
            blobs_per_client: 8,
            history_bytes: 512 << 20,
            jobs_total: 32,
            jobs_per_client: 8,
            running_jobs: 2,
            session_events: 512,
            session_event_bytes: 8 << 20,
            registry_events: 256,
            registry_event_bytes: 1 << 20,
        }
    }
}

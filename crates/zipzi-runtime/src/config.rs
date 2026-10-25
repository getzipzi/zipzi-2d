/// R0 defaults for the objects enabled in R1. Estimates conservatively count
/// shared document content per holder; these are not RSS or GPU limits.
#[derive(Debug, Clone)]
pub struct RuntimeLimits {
    pub clients: usize,
    pub sessions: usize,
    pub edits_per_client_session: usize,
    pub edits_per_session: usize,
    pub edits_total: usize,
    pub candidate_bytes: usize,
    pub frozen_bytes: usize,
    pub accounted_bytes: usize,
    pub image_bytes: usize,
    pub plans_per_client: usize,
    pub references_per_client: usize,
    pub prepared_images_per_client: usize,
    pub plan_bytes: usize,
    pub blob_bytes: usize,
    pub blobs_per_client: usize,
    pub history_bytes: usize,
    pub jobs_total: usize,
    pub jobs_per_client: usize,
    pub running_jobs: usize,
    pub session_events: usize,
    pub session_event_bytes: usize,
    pub registry_events: usize,
    pub registry_event_bytes: usize,
}
impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            clients: 8,
            sessions: 32,
            edits_per_client_session: 1,
            edits_per_session: 4,
            edits_total: 16,
            candidate_bytes: 512 << 20,
            frozen_bytes: 256 << 20,
            accounted_bytes: 2 << 30,
            image_bytes: 768 << 20,
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

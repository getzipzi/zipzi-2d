use super::*;
use zipzi_contract::{
    kernel_commands::{Command, CommandOutcome},
    kernel_wire::Query,
};
impl Runtime {
    pub fn instance_id(&self) -> RuntimeInstanceId {
        self.instance
    }
    pub fn limits(&self) -> &RuntimeLimits {
        &self.limits
    }
    pub fn project_io(&self) -> &ProjectIo {
        &self.project_io
    }
    pub fn configure_project_io(&mut self, io: ProjectIo) {
        self.project_io = io;
    }
    pub fn resources(&self) -> &zipzi_project::ResourceStore {
        &self.resources
    }
    pub fn transfer_budget(&self) -> ImageMemoryBudget {
        self.budget.clone()
    }
    pub fn reserve_transfer(&self, bytes: usize) -> Result<TransferReservation> {
        self.budget.reserve_transfer(bytes)
    }
    pub fn list_sessions(&self, client: ClientId) -> Result<Vec<serde_json::Value>> {
        self.client(client)?;
        Ok(self
            .sessions
            .iter()
            .map(|(id, entry)| {
                serde_json::json!({"session":id,
            "version":entry.sdk.version(),"document_id":entry.sdk.snapshot().document().id(),
            "path":entry.sdk.project().map(|p| p.path()),"dirty":entry.sdk.is_dirty()})
            })
            .collect())
    }
    pub fn detach(&mut self, client: ClientId, session: SessionKey) -> Result<()> {
        self.session(client, session)?;
        self.sessions
            .get_mut(&session)
            .unwrap()
            .users
            .remove(&client);
        self.drafts
            .retain(|_, d| !(d.owner == client && d.source.base.session == session));
        for j in self.jobs.values_mut().filter(|j| {
            j.info.client == client
                && j.info.source.session() == session
                && !j.info.state.is_terminal()
        }) {
            j.cancel.cancel();
            j.info.state = JobState::Cancelling;
        }
        Ok(())
    }
    pub fn commands(
        &mut self,
        client: ClientId,
        expected: DraftSource,
        commands: Vec<Command>,
    ) -> Result<(DraftSource, Vec<CommandOutcome>)> {
        let resources = self.resources.clone();
        self.batch(client, expected, move |c| {
            if commands.len() > 4096 {
                return Err(error("RESOURCE_LIMIT", "Batch command count exceeds limit"));
            }
            commands
                .into_iter()
                .map(|command| {
                    c.execute(command, &resources, Default::default(), &Default::default())
                })
                .collect()
        })
    }
    pub fn command_job(
        &mut self,
        client: ClientId,
        expected: DraftSource,
        commands: Vec<Command>,
    ) -> Result<JobId> {
        let resources = self.resources.clone();
        self.request_batch(client, expected, TaskBudget::default(), move |c, cancel| {
            if commands.len() > 4096 {
                return Err(error("RESOURCE_LIMIT", "Batch command count exceeds limit"));
            }
            let control = cancel.conversion_control();
            for command in commands {
                cancel.check()?;
                c.execute(command, &resources, Default::default(), &control)?;
            }
            Ok(())
        })
    }
    pub fn preview_commands(
        &mut self,
        client: ClientId,
        expected: DraftSource,
        commands: Vec<Command>,
    ) -> Result<(DraftSource, Vec<CommandOutcome>)> {
        let resources = self.resources.clone();
        self.preview(client, expected, move |c| {
            if commands.len() > 4096 {
                return Err(error(
                    "RESOURCE_LIMIT",
                    "Preview command count exceeds limit",
                ));
            }
            commands
                .into_iter()
                .map(|command| {
                    c.execute(command, &resources, Default::default(), &Default::default())
                })
                .collect()
        })
    }
    pub fn query(
        &self,
        client: ClientId,
        source: QuerySource,
        query: Query,
    ) -> Result<serde_json::Value> {
        let snapshot = match source {
            QuerySource::Published(expected) => {
                let snapshot = self.read_snapshot(client, expected.session)?;
                if snapshot.source != source {
                    return Err(error("STALE_SOURCE", "Query publication is stale"));
                }
                snapshot
            }
            QuerySource::Draft(expected) => self.read_draft(client, expected)?,
        };
        crate::query::read(&snapshot, query, self.limits.result_bytes.min(1 << 20))
    }
    pub fn cancel_host_jobs(&mut self) {
        for j in self
            .jobs
            .values_mut()
            .filter(|j| !j.info.state.is_terminal())
        {
            j.cancel.cancel();
            j.info.state = JobState::Cancelling;
        }
    }
    pub fn outstanding_jobs(&self) -> usize {
        self.jobs
            .values()
            .filter(|j| !j.info.state.is_terminal())
            .count()
    }
}

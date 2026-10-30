use crate::{ClientId, JobId, Result, RuntimeError, RuntimeInstanceId, SessionId};
use std::collections::VecDeque;
use std::sync::Arc;
use zipzi_sdk::EditReceiptSize;

pub use zipzi_contract::runtime::EventCursor;

pub use zipzi_contract::runtime::{EventKind, RuntimeEvent};

pub(crate) trait RuntimeEventBytes {
    fn estimated_bytes(&self) -> usize;
}
impl RuntimeEventBytes for RuntimeEvent {
    fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match &self.kind {
                EventKind::Published(receipt) => receipt.estimated_bytes(),
                EventKind::Saved(receipt) => {
                    receipt.manifest.as_os_str().len()
                        + receipt
                            .warnings
                            .iter()
                            .chain(&receipt.history_warnings)
                            .map(String::capacity)
                            .sum::<usize>()
                }
                EventKind::Created(info)
                | EventKind::Closed(info)
                | EventKind::StateChanged(info) => {
                    info.path.as_ref().map_or(0, |p| p.as_os_str().len())
                        + info.fault.as_ref().map_or(0, |e| e.to_string().len())
                }
            }
    }
}

pub(crate) struct EventLog {
    cursor: EventCursor,
    items: VecDeque<(Arc<RuntimeEvent>, usize)>,
    bytes: usize,
    max_items: usize,
    max_bytes: usize,
}
impl EventLog {
    pub(crate) fn new(
        runtime: RuntimeInstanceId,
        session: Option<SessionId>,
        max_items: usize,
        max_bytes: usize,
    ) -> Self {
        Self {
            cursor: EventCursor {
                runtime,
                session,
                sequence: 0,
            },
            items: VecDeque::new(),
            bytes: 0,
            max_items,
            max_bytes,
        }
    }
    pub(crate) fn cursor(&self) -> EventCursor {
        self.cursor
    }
    pub(crate) fn clear(&mut self) {
        self.items.clear();
        self.bytes = 0;
    }
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
    pub(crate) fn push(
        &mut self,
        client: Option<ClientId>,
        job: Option<JobId>,
        operation: crate::OperationId,
        kind: EventKind,
    ) {
        self.cursor.sequence = self
            .cursor
            .sequence
            .checked_add(1)
            .expect("event sequence exhausted");
        let event = Arc::new(RuntimeEvent {
            cursor: self.cursor,
            client,
            job,
            operation,
            kind,
        });
        let size = event.estimated_bytes();
        // Oversized events advance the cursor but cannot silently disappear:
        // every reader preceding this event receives ResyncRequired.
        self.bytes = self.bytes.saturating_add(size);
        self.items.push_back((event, size));
        while self.items.len() > self.max_items || self.bytes > self.max_bytes {
            let (_, size) = self.items.pop_front().expect("nonempty event log");
            self.bytes -= size;
        }
    }
    pub(crate) fn since(&self, cursor: EventCursor) -> Result<Vec<Arc<RuntimeEvent>>> {
        let oldest = self
            .items
            .front()
            .map_or(self.cursor.sequence, |(e, _)| e.cursor.sequence - 1);
        if cursor.runtime != self.cursor.runtime
            || cursor.session != self.cursor.session
            || cursor.sequence < oldest
            || cursor.sequence > self.cursor.sequence
        {
            return Err(RuntimeError::ResyncRequired);
        }
        Ok(self
            .items
            .iter()
            .filter(|(e, _)| e.cursor.sequence > cursor.sequence)
            .map(|(e, _)| e.clone())
            .collect())
    }
}

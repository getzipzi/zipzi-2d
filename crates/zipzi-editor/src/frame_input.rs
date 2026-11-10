//! LeadingLatest scheduling state. The interaction owns the work and the frame callback.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameTicket<K> {
    pub(crate) owner: K,
    generation: u64,
}

pub(crate) enum Offer<K, S> {
    ApplyNow(S, FrameTicket<K>),
    Retained,
}

pub(crate) struct FrameInput<K, S> {
    generation: u64,
    scheduled: Option<FrameTicket<K>>,
    pending: Option<S>,
}

impl<K, S> Default for FrameInput<K, S> {
    fn default() -> Self {
        Self {
            generation: 0,
            scheduled: None,
            pending: None,
        }
    }
}

impl<K: Copy + Eq, S> FrameInput<K, S> {
    pub(crate) fn is_scheduled(&self, owner: K) -> bool {
        self.scheduled.is_some_and(|ticket| ticket.owner == owner)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn offer(
        &mut self,
        owner: K,
        mut sample: S,
        merge: impl FnOnce(S, &mut S),
    ) -> Offer<K, S> {
        if self.is_scheduled(owner) {
            if let Some(previous) = self.pending.take() {
                merge(previous, &mut sample);
            }
            self.pending = Some(sample);
            return Offer::Retained;
        }
        self.generation += 1;
        let ticket = FrameTicket {
            owner,
            generation: self.generation,
        };
        self.scheduled = Some(ticket);
        self.pending = None;
        Offer::ApplyNow(sample, ticket)
    }

    pub(crate) fn take_for_frame(&mut self, ticket: FrameTicket<K>) -> Option<S> {
        if self.scheduled != Some(ticket) {
            return None;
        }
        self.drain_for_barrier()
    }

    pub(crate) fn drain_for_barrier(&mut self) -> Option<S> {
        self.scheduled = None;
        self.pending.take()
    }

    pub(crate) fn invalidate(&mut self) {
        self.drain_for_barrier();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_leading_and_latest_then_reopens_for_next_input() {
        let mut slot = FrameInput::default();
        let Offer::ApplyNow(1, first) = slot.offer(0, 1, |_, _| {}) else {
            panic!("first sample should apply immediately");
        };
        assert!(matches!(slot.offer(0, 2, |_, _| {}), Offer::Retained));
        assert!(matches!(slot.offer(0, 3, |_, _| {}), Offer::Retained));
        assert_eq!(slot.take_for_frame(first), Some(3));
        let Offer::ApplyNow(4, next) = slot.offer(0, 4, |_, _| {}) else {
            panic!("a trailing callback should reopen the slot");
        };
        assert_eq!(slot.take_for_frame(first), None);
        assert_eq!(slot.take_for_frame(next), None);
        assert!(matches!(slot.offer(0, 5, |_, _| {}), Offer::ApplyNow(5, _)));
    }

    #[test]
    fn merges_pending_and_drains_once_before_the_frame() {
        let mut slot = FrameInput::default();
        let merge = |old, new: &mut u8| *new |= old;
        let Offer::ApplyNow(1, ticket) = slot.offer(0, 1, merge) else {
            panic!("first demand should apply immediately");
        };
        slot.offer(0, 2, merge);
        slot.offer(0, 4, merge);
        assert_eq!(slot.drain_for_barrier(), Some(6));
        assert_eq!(slot.take_for_frame(ticket), None);
        assert!(!slot.has_pending());
    }

    #[test]
    fn old_tickets_cannot_touch_a_restarted_slot_or_another_owner() {
        let mut slot = FrameInput::default();
        let Offer::ApplyNow(_, old) = slot.offer(0, 1, |_, _| {}) else {
            unreachable!();
        };
        slot.offer(0, 2, |_, _| {});
        slot.invalidate();
        let Offer::ApplyNow(_, restarted) = slot.offer(0, 3, |_, _| {}) else {
            unreachable!();
        };
        slot.offer(0, 4, |_, _| {});
        assert_eq!(slot.take_for_frame(old), None);
        assert_eq!(slot.take_for_frame(restarted), Some(4));

        let Offer::ApplyNow(_, previous_owner) = slot.offer(0, 5, |_, _| {}) else {
            unreachable!();
        };
        slot.offer(0, 6, |_, _| {});
        let Offer::ApplyNow(_, next_owner) = slot.offer(1, 7, |_, _| {}) else {
            unreachable!();
        };
        assert!(!slot.has_pending());
        slot.offer(1, 8, |_, _| {});
        assert_eq!(slot.take_for_frame(previous_owner), None);
        assert_eq!(slot.take_for_frame(next_owner), Some(8));
    }
}

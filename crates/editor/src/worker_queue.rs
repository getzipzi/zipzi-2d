//! Coalesce each owner independently while preserving FIFO service between owners.
use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    sync::{Arc, Mutex},
};

pub(crate) struct Coalesced<K, V> {
    order: VecDeque<K>,
    values: HashMap<K, V>,
}
impl<K, V> Default for Coalesced<K, V> {
    fn default() -> Self {
        Self {
            order: VecDeque::new(),
            values: HashMap::new(),
        }
    }
}
impl<K: Copy + Eq + Hash, V> Coalesced<K, V> {
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if !self.values.contains_key(&key) {
            self.order.push_back(key);
        }
        self.values.insert(key, value)
    }
    pub fn pop(&mut self) -> Option<V> {
        self.order
            .pop_front()
            .and_then(|key| self.values.remove(&key))
    }
    pub fn remove(&mut self, key: K) -> Option<V> {
        self.order.retain(|id| *id != key);
        self.values.remove(&key)
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

pub(crate) struct MailboxSender<K, V> {
    values: Arc<Mutex<Coalesced<K, V>>>,
    wake: async_channel::Sender<()>,
}
pub(crate) struct MailboxReceiver<K, V> {
    values: Arc<Mutex<Coalesced<K, V>>>,
    wake: async_channel::Receiver<()>,
}
pub(crate) fn mailbox<K: Copy + Eq + Hash, V>() -> (MailboxSender<K, V>, MailboxReceiver<K, V>) {
    let values = Arc::new(Mutex::new(Coalesced::default()));
    let (send, recv) = async_channel::bounded(1);
    (
        MailboxSender {
            values: values.clone(),
            wake: send,
        },
        MailboxReceiver { values, wake: recv },
    )
}
impl<K: Copy + Eq + Hash, V> MailboxSender<K, V> {
    pub fn send(&self, key: K, value: V) -> Result<(), ()> {
        if self.wake.is_closed() {
            return Err(());
        }
        self.values.lock().map_err(|_| ())?.insert(key, value);
        // A pending wake already represents every queued owner. Never stall the worker.
        let _ = self.wake.try_send(());
        Ok(())
    }
}
impl<K: Copy + Eq + Hash, V> MailboxReceiver<K, V> {
    pub async fn recv(&self) -> Result<V, ()> {
        loop {
            if let Some(value) = self.values.lock().map_err(|_| ())?.pop() {
                return Ok(value);
            }
            self.wake.recv().await.map_err(|_| ())?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacements_do_not_displace_or_starve_other_owners() {
        let mut queue = Coalesced::default();
        queue.insert(1, 10);
        queue.insert(2, 20);
        for value in 11..100 {
            queue.insert(1, value);
        }
        assert_eq!(queue.pop(), Some(99));
        queue.insert(1, 100);
        assert_eq!(queue.pop(), Some(20));
        assert_eq!(queue.pop(), Some(100));
        assert!(queue.is_empty());
        queue.insert(1, 200);
        assert_eq!(queue.remove(1), Some(200));
        assert!(queue.pop().is_none());
    }
    #[test]
    fn unread_results_are_bounded_per_owner_and_never_block_other_owners() {
        let (sender, receiver) = mailbox();
        for value in 0..1000 {
            sender.send(1, value).unwrap();
        }
        sender.send(2, 2000).unwrap();
        assert_eq!(receiver.values.lock().unwrap().pop(), Some(999));
        assert_eq!(receiver.values.lock().unwrap().pop(), Some(2000));
    }
}

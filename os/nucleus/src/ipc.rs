//! Tier-gated zero-copy IPC — the Singularity result, proof-gated.
//!
//! In a single address space, sending a message is moving ownership of a heap
//! allocation: a pointer handoff, no serialization, no kernel copy, no trap.
//! Rust's move semantics make this *provably* safe — the sender cannot touch
//! the message after send, because it no longer owns it. That is the whole
//! trick Linux cannot pull: process IPC costs two traps, two context switches
//! and at least one copy precisely because neither side trusts the other's
//! memory. Here the trust ledger gates the fast path instead: a channel
//! declares the minimum tier allowed to use it, and an unproven task is simply
//! denied — it must go through a supervised (copying) boundary instead.

use alloc::boxed::Box;
use alloc::collections::VecDeque;

use crate::proof::Tier;
use crate::sync::SpinLock;

#[derive(Debug, PartialEq, Eq)]
pub enum SendError<T> {
    /// The sender's tier does not clear the channel's bar; the payload is
    /// returned so the caller can route it through a supervised path.
    TierDenied(Box<T>),
}

pub struct Channel<T> {
    queue: SpinLock<VecDeque<Box<T>>>,
    /// The most-caged tier still allowed on the zero-copy path.
    min_tier: Tier,
    crossings: SpinLock<u64>,
}

impl<T> Channel<T> {
    pub const fn new(min_tier: Tier) -> Self {
        Self {
            queue: SpinLock::new(VecDeque::new()),
            min_tier,
            crossings: SpinLock::new(0),
        }
    }

    /// Zero-copy send: ownership of the allocation moves; nothing is copied.
    pub fn send(&self, message: Box<T>, sender_tier: Tier) -> Result<(), SendError<T>> {
        if sender_tier.rank() > self.min_tier.rank() {
            return Err(SendError::TierDenied(message));
        }
        self.queue.lock().push_back(message);
        *self.crossings.lock() += 1;
        Ok(())
    }

    pub fn recv(&self) -> Option<Box<T>> {
        self.queue.lock().pop_front()
    }

    pub fn crossings(&self) -> u64 {
        *self.crossings.lock()
    }

    pub fn len(&self) -> usize {
        self.queue.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_moves_without_copying() {
        let channel: Channel<[u8; 4096]> = Channel::new(Tier::Partial);
        let message = Box::new([7u8; 4096]);
        let addr_before = &*message as *const _ as usize;
        channel.send(message, Tier::Proven).unwrap();
        let received = channel.recv().unwrap();
        // The very same allocation came out — zero bytes were copied.
        assert_eq!(&*received as *const _ as usize, addr_before);
        assert_eq!(received[0], 7);
        assert_eq!(channel.crossings(), 1);
    }

    #[test]
    fn unproven_sender_is_denied_the_fast_path() {
        let channel: Channel<u64> = Channel::new(Tier::Partial);
        let denied = channel.send(Box::new(42), Tier::Unproven);
        match denied {
            Err(SendError::TierDenied(payload)) => assert_eq!(*payload, 42),
            _ => panic!("unproven sender must be denied"),
        }
        assert_eq!(channel.crossings(), 0);
    }
}

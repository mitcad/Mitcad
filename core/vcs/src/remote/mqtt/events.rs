// SPDX-License-Identifier: MIT
//! The checked events of a hub's connections, waiting for the application
//! to read them ([`super::LiveHub::events`]).

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;

/// Events waiting longer than this many are lost, the oldest first.
pub const MAX_EVENTS: usize = 10_000;
/// One read takes at most this many events.
pub const MAX_READ: usize = 1_000;

#[derive(Default)]
struct Queue {
    events: VecDeque<Value>,
    /// Events lost to the limit since the last read.
    lost: u64,
    closed: bool,
}

/// A queue of events written by the connections' threads and read by one
/// thread of the application.
#[derive(Default)]
pub struct EventQueue {
    queue: Mutex<Queue>,
    ready: Condvar,
}

impl EventQueue {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn push(&self, event: Value) {
        let mut queue = self.lock();
        if queue.events.len() >= MAX_EVENTS {
            queue.events.pop_front();
            queue.lost += 1;
        }
        queue.events.push_back(event);
        drop(queue);
        self.ready.notify_all();
    }

    /// A `dropped` event: it takes the place of the last event waiting when
    /// that is a `dropped` event of the same connection, so that a stream
    /// of bad messages does not fill the queue.
    pub fn push_dropped(&self, event: Value) {
        let mut queue = self.lock();
        if let Some(last) = queue.events.back_mut()
            && last["type"] == "dropped"
            && last["connection"] == event["connection"]
        {
            *last = event;
            return;
        }
        drop(queue);
        self.push(event);
    }

    /// The events waiting, after waiting up to `timeout` for one; the
    /// number lost since the last read; whether the hub is closed.
    pub fn take(&self, timeout: Duration) -> (Vec<Value>, u64, bool) {
        let deadline = Instant::now() + timeout;
        let mut queue = self.lock();
        while queue.events.is_empty() && !queue.closed {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            queue = self
                .ready
                .wait_timeout(queue, deadline - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        let n = queue.events.len().min(MAX_READ);
        let events = queue.events.drain(..n).collect();
        let lost = std::mem::take(&mut queue.lost);
        (events, lost, queue.closed)
    }

    /// Ends the waiting: every read returns at once from now on.
    pub fn close(&self) {
        self.lock().closed = true;
        self.ready.notify_all();
    }
}

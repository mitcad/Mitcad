// SPDX-License-Identifier: MIT
//! Live updates for C++ (mitcad#89, `mitcad_vcs::remote::mqtt`): the hub of
//! the application's MQTT connections, driven with JSON commands, and the
//! checked events of every connection, read with a blocking call on a
//! worker thread. The application never sees MQTT's bytes.
//!
//! The application keeps one hub. It runs the commands on its UI thread
//! (`test` blocks up to its timeout: on a worker thread) and reads the
//! events on a worker thread of its own, both through `&LiveHub`: the hub
//! is `Sync` (a mutex, and a condition variable for the events), so the two
//! threads may use it at once; `close` ends a waiting read.

use std::time::Duration;

use mitcad_vcs::remote::mqtt::{self, HubOptions, LiveError};

/// The application's live updates.
pub struct LiveHub(mqtt::LiveHub);

/// A hub with the default times (`MITCAD_TEST_LIVE_RETRY_MS` shortens the
/// back-off for tests).
pub fn new_live_hub() -> Box<LiveHub> {
    Box::new(LiveHub(mqtt::LiveHub::new(HubOptions::from_env())))
}

impl LiveHub {
    pub fn command(&self, json: &str) -> Result<String, LiveError> {
        self.0.command(json)
    }

    pub fn events(&self, timeout_ms: u32) -> String {
        self.0.events(Duration::from_millis(u64::from(timeout_ms)))
    }
}

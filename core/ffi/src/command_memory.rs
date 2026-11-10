// SPDX-License-Identifier: MIT
//! The memory guard of model commands (mitcad#1480,
//! `mitcad_model::monitor::memory_guard`): in a process that turns it on
//! ([`guard_command_memory`]: the headless MCP server, the application
//! with its live MCP server, `mitcad-cli info` and `run`), every model
//! command runs with the process's memory measured against its limits, as
//! the imports' memory guard measures it (`memory.rs`, mitcad#80), and is
//! stopped with an error that says so before the process runs out of
//! memory. The imports keep their own guard and are not stopped by this
//! one; nor are undo and redo (`lib.rs`), which put back what a refused
//! command array changed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use mitcad_model::RecomputeMonitor;
use mitcad_model::monitor::memory_guard::{self, MemoryReading};
use mitcad_model_api::{ApiError, DocumentApi};

use crate::Document;
use crate::memory::MemoryUse;

/// Whether model commands run under the memory guard in this process.
static GUARDED: AtomicBool = AtomicBool::new(false);

/// The commands' own memory limit in bytes; 0 for the process's limits
/// only.
static LIMIT: AtomicU64 = AtomicU64::new(0);

/// The share of the tightest limit from which the allocator's free memory
/// is given back to the system before the guard decides (mitcad#885:
/// resident memory the process no longer uses counts against the limits).
const MEMORY_TIGHT: f64 = 0.70;

/// Turns the memory guard of model commands on in this process, with a
/// limit of the commands' own in MiB beside the process's limits (none
/// or 0: the process's limits only; a lower one for tests).
pub fn guard_command_memory(limit_mib: f64) {
    let limit = if limit_mib.is_finite() && limit_mib > 0.0 {
        (limit_mib * f64::from(1u32 << 20)) as u64
    } else {
        0
    };
    LIMIT.store(limit, Ordering::Relaxed);
    GUARDED.store(true, Ordering::Relaxed);
}

/// The memory in use against the commands' own limit when that is the
/// tighter one: the resident memory against it (as `MemoryUse::within`
/// for an import's limit).
fn within(mut memory: MemoryUse, limit: u64) -> MemoryUse {
    if limit > 0 && (memory.limit == 0 || memory.resident as f64 / limit as f64 > memory.share()) {
        memory.used = memory.resident;
        memory.limit = limit;
        memory.limit_kind = "the command memory limit".to_owned();
    }
    memory
}

/// What the guard measures: the process against its limits and `limit`,
/// the allocator's free memory given back first when the memory is tight.
fn measurer(limit: u64) -> impl FnMut() -> MemoryReading + Send + 'static {
    let mut release: Option<mitcad_import::FreeMemoryRelease> = None;
    move || {
        let mut memory = within(crate::memory::memory_use(), limit);
        let release =
            release.get_or_insert_with(|| mitcad_import::FreeMemoryRelease::new(memory.resident));
        let now = Instant::now();
        if memory.share() >= MEMORY_TIGHT && release.due(memory.resident, true, now) {
            crate::memory::release_free_memory();
            memory = within(crate::memory::memory_use(), limit);
            release.done(memory.resident, now);
        }
        MemoryReading {
            share: memory.share(),
            text: memory.describe(),
            failed_allocations: memory.failed_allocations,
        }
    }
}

impl Document {
    /// A model command (not an import, undo or redo), under the memory guard when the
    /// process turned it on: with the job's monitor attached (the
    /// application's), else with one of the command's own. A command the
    /// guard stopped fails with the guard's reason and changes nothing.
    pub(crate) fn guarded_command(&mut self, json: &str) -> Result<String, ApiError> {
        self.guarded_model_action(|doc| doc.command(json))
    }

    /// Runs a caller-controlled model action under the same command guard.
    pub(crate) fn guarded_model_action<T>(
        &mut self,
        action: impl FnOnce(
            &mut mitcad_model::Document<crate::kernel::OcctKernel>,
        ) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        if !GUARDED.load(Ordering::Relaxed) {
            return action(&mut self.0);
        }
        let attached = self.0.monitor().cloned();
        let monitor = match &attached {
            Some(monitor) => Arc::clone(monitor),
            None => {
                let own = Arc::new(RecomputeMonitor::new());
                self.0.set_monitor(Some(Arc::clone(&own)));
                own
            }
        };
        let limit = LIMIT.load(Ordering::Relaxed);
        let result = memory_guard::guarded(&monitor, measurer(limit), || action(&mut self.0));
        if attached.is_none() {
            self.0.set_monitor(None);
        }
        match (result, monitor.cancel_reason()) {
            (Err(_), Some(reason)) => Err(ApiError(reason)),
            (result, _) => result,
        }
    }
}

// SPDX-License-Identifier: MIT
//! The process's memory against the limits it runs under (mitcad#80), for
//! the import's memory guard (`f3d_import.rs`): an allocation that fails
//! ends the process (Rust's allocation failures cannot be caught), so the
//! import gives up its remaining definitions before the memory runs out.

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// The process's memory against the tightest of its limits.
    #[derive(Debug, Clone)]
    struct MemoryUse {
        /// Bytes in use as the limit counts them (address space, data,
        /// committed or resident memory).
        used: u64,
        /// The limit in bytes; 0 when none is known.
        limit: u64,
        /// The resident memory in bytes.
        resident: u64,
        /// What the limit is ("address-space limit", ...); empty without
        /// one.
        limit_kind: String,
        /// How many allocations inside the geometry kernel have failed in
        /// the process so far (mitcad#132), also those it caught inside.
        failed_allocations: u64,
    }

    unsafe extern "C++" {
        include!("bridge/memory.hpp");

        fn memory_use() -> MemoryUse;
    }
}

pub use ffi::{MemoryUse, memory_use};

impl MemoryUse {
    /// The share of the limit in use (0 without a limit).
    pub fn share(&self) -> f64 {
        if self.limit == 0 {
            0.0
        } else {
            self.used as f64 / self.limit as f64
        }
    }

    /// The memory in use against a limit given in bytes, when that is the
    /// tighter one: the resident memory against it.
    pub fn within(mut self, limit: Option<u64>) -> Self {
        if let Some(limit) = limit.filter(|l| *l > 0)
            && (self.limit == 0 || self.resident as f64 / limit as f64 > self.share())
        {
            self.used = self.resident;
            self.limit = limit;
            self.limit_kind = "the import's memory limit".to_owned();
        }
        self
    }

    /// E.g. "9.8 GiB of 11.4 GiB (address-space limit)".
    pub fn describe(&self) -> String {
        if self.limit == 0 {
            format!("{} resident", size(self.resident))
        } else {
            format!(
                "{} of {} ({})",
                size(self.used),
                size(self.limit),
                self.limit_kind
            )
        }
    }
}

/// A size in MiB, or in GiB from 1 GiB on.
fn size(bytes: u64) -> String {
    let mib = bytes as f64 / f64::from(1u32 << 20);
    if mib < 1024.0 {
        format!("{mib:.0} MiB")
    } else {
        format!("{:.1} GiB", mib / 1024.0)
    }
}

// SPDX-License-Identifier: MIT
//! What the fuzz targets of the file readers share (mitcad#939): the time
//! budget each input must be read within. The targets are in
//! `fuzz_targets/`, the seed corpora are written by
//! `examples/write_seeds.rs`; `docs/development.md` ("Fuzz targets of the
//! file readers") tells how to run them.

use std::time::{Duration, Instant};

/// The most one input may take to be read, refused or reported. The
/// inputs a fuzzer makes are a few KB (the seeds at most a few hundred),
/// which every reader handles in milliseconds; an input that takes
/// seconds is a hang or a quadratic blow-up waiting for a larger file.
/// libFuzzer's own `-timeout` stops an input that never returns.
pub const BUDGET: Duration = Duration::from_secs(10);

/// Runs one input through `read` and asserts that it returned within
/// [`BUDGET`]; a panic inside `read` is a finding of the fuzzer as it is.
pub fn within_budget<T>(target: &str, input: &[u8], read: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let out = read();
    let took = started.elapsed();
    assert!(
        took <= BUDGET,
        "{target}: an input of {} bytes took {took:?}, more than the budget of {BUDGET:?}",
        input.len()
    );
    out
}

// SPDX-License-Identifier: MIT
//! `mitcad-testrun`: runs a build tree's ctests one `ctest` per test,
//! measures each (wall and CPU time, peak memory of the process tree, the
//! kind of result), keeps the machine's profiles between runs, and either
//! runs the tests at a fixed parallelism (`--measure`) or schedules them by
//! a memory budget and a CPU gate (`--schedule`). See README.md.

pub mod changed;
pub mod concurrency;
pub mod corpus;
pub mod ctest;
pub mod macos_policy;
pub mod options;
pub mod profile;
pub mod report;
pub mod runner;
pub mod schedule;
pub mod sys;

// SPDX-License-Identifier: MIT
//! The runner's native macOS checks (their modules are empty elsewhere).
//!
//! One test binary of the crate's integration tests, the files of this
//! folder its modules (mitcad#2496). A new test file here is declared
//! below in the order of the names; a file no binary declares fails
//! repo.rust-test-groups. The ctests run each test in a process of its
//! own (core/cargo-tests.cmake), so a test sets up all it needs itself.

mod macos_driver;
mod macos_process_tree;
mod macos_scheduling;

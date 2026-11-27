// SPDX-License-Identifier: MIT
//! The reader's limits on hostile drawings, measured by the counting
//! allocator of this binary's own (a binary apart from the other DXF tests).
//!
//! One test binary of the crate's integration tests, the files of this
//! folder its modules (mitcad#2496). A new test file here is declared
//! below in the order of the names; a file no binary declares fails
//! repo.rust-test-groups. The ctests run each test in a process of its
//! own (core/cargo-tests.cmake), so a test sets up all it needs itself.

mod limits;

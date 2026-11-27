// SPDX-License-Identifier: MIT
//! The `.solidcraft` import of designs saved as `.sc`.
//!
//! One test binary of the crate's integration tests, the files of this
//! folder its modules (mitcad#2496). A new test file here is declared
//! below in the order of the names; a file no binary declares fails
//! repo.rust-test-groups. The ctests run each test in a process of its
//! own (core/cargo-tests.cmake), so a test sets up all it needs itself.

mod sc_extension;

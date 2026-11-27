// SPDX-License-Identifier: MIT
//! The posts' programs run through the controllers' own code.
//!
//! One test binary of the crate's integration tests, the files of this
//! folder its modules (mitcad#2496). A new test file here is declared
//! below in the order of the names; a file no binary declares fails
//! repo.rust-test-groups. The ctests run each test in a process of its
//! own (core/cargo-tests.cmake), so a test sets up all it needs itself.

mod controller_sim;
mod linuxcnc_lathe_sim;
mod sheet_cut_sim;

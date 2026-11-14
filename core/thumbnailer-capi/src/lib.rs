// SPDX-License-Identifier: MIT
//! The static library Windows Explorer's thumbnail handler (mitcad#1695)
//! and the Finder's Quick Look extension (mitcad#1696) link (the CMake
//! target `mitcad_thumbnailer_capi`): the C ABI of
//! `mitcad-thumbnailer` (`core/thumbnailer/src/capi.rs`, declared in
//! `include/mitcad_thumbnailer.h`) with every Rust library it uses. Linking
//! the rlib here puts its `#[no_mangle]` functions into this library.

pub use mitcad_thumbnailer::capi;

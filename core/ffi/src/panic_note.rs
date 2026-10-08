// SPDX-License-Identifier: MIT
//! A Rust panic's message for the application's crash reports (mitcad#62).
//!
//! A panic that reaches the C++ bridge aborts the process (cxx does not let
//! it unwind into C++), and the crash handler's report then says only
//! `SIGABRT`. The hook installed here hands the panic's message and place to
//! the application first (`mitcad::crash::noteMessage`), then runs the
//! hook that was there before, which prints it as usual.

use std::ffi::c_char;

/// Where the messages go: a C function taking UTF-8 bytes and their length.
pub type PanicSink = extern "C" fn(message: *const c_char, length: usize);

/// Installs the hook; the application calls it once at start-up.
#[unsafe(no_mangle)]
pub extern "C" fn mitcad_set_panic_sink(sink: PanicSink) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let text = format!("Rust panic: {info}");
        sink(text.as_ptr().cast(), text.len());
        previous(info);
    }));
}

/// For tests (`core.bridge`): a panic caught here, so that the hook runs
/// without the abort that a panic across the bridge brings.
#[unsafe(no_mangle)]
pub extern "C" fn mitcad_test_panic_note() {
    let _ = std::panic::catch_unwind(|| panic!("panic for a test of the note"));
}

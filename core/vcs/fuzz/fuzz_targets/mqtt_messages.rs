// SPDX-License-Identifier: MIT
//! The readers of live messages on arbitrary bytes: a topic name, a 0
//! byte, a payload (also read on every topic of Mitcad's forms).
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    mitcad_vcs::remote::mqtt::fuzzing::message(data);
});

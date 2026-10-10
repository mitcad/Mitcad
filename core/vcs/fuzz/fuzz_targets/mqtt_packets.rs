// SPDX-License-Identifier: MIT
//! The MQTT packet decoder of live updates on arbitrary bytes, as a broker
//! could send them.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    mitcad_vcs::remote::mqtt::fuzzing::packets(data);
});

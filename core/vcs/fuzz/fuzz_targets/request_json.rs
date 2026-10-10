// SPDX-License-Identifier: MIT
//! `request.json` from a request ref: read or dropped, never a panic; what
//! is read is within the limits and written back the same.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_vcs::remote::locks::{MAX_MESSAGE, MAX_NAME, is_project_path, read_request};

fuzz_target!(|data: &[u8]| {
    if let Ok(request) = read_request(data) {
        assert!(is_project_path(&request.path));
        assert!(request.requester.name.chars().count() <= MAX_NAME);
        assert!(
            request
                .message
                .as_ref()
                .is_none_or(|m| m.chars().count() <= MAX_MESSAGE)
        );
        assert_eq!(read_request(&request.to_json()).as_ref(), Ok(&request));
    }
});

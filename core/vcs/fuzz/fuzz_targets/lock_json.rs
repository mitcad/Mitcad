// SPDX-License-Identifier: MIT
//! `lock.json` from a lock ref: read or dropped, never a panic; what is
//! read is within the limits, and Mitcad writing it back gives a file that
//! is read again (answers and request ids may be left out to fit).
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_vcs::remote::locks::{MAX_FILE_SIZE, MAX_NAME, clean_text, is_project_path, read_lock};

fuzz_target!(|data: &[u8]| {
    if let Ok(lock) = read_lock(data) {
        assert!(is_project_path(&lock.path));
        assert!(lock.owner.name.chars().count() <= MAX_NAME);
        assert_eq!(clean_text(&lock.owner.name), lock.owner.name);
        assert!((1..=120).contains(&lock.idle_minutes));
        assert!((5..=600).contains(&lock.poll_seconds));
        let written = lock.to_json();
        assert!(written.len() <= MAX_FILE_SIZE);
        let again = read_lock(&written).expect("a lock Mitcad wrote is read");
        assert_eq!(
            (&again.path, &again.owner, &again.session, again.state),
            (&lock.path, &lock.owner, &lock.session, lock.state)
        );
        assert_eq!(
            (again.taken_at, again.refreshed_at, again.active_at),
            (lock.taken_at, lock.refreshed_at, lock.active_at)
        );
    }
});

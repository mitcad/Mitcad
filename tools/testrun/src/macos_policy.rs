// SPDX-License-Identifier: MIT
//! Admission for audited macOS harnesses. Only their application lifetime
//! owns Aqua; independent setup/verification may overlap. Other UI tests
//! retain their CTest desktop locks and sensitive lane.

use crate::ctest::TestDef;
use crate::profile::Prediction;

pub const HARNESS_LABEL: &str = "macos_gui_harness";
pub const HEADLESS_LABEL: &str = "macos_headless";
pub const CARGO_LABEL: &str = "macos_cargo_build";
pub const HARNESS_MAX: usize = 2;
pub const CARGO_MEMORY: u64 = 6 << 30;

pub fn labelled(def: &TestDef, label: &str) -> bool {
    cfg!(target_os = "macos") && def.labels.iter().any(|l| l == label)
}

pub fn prediction(def: &TestDef, mut prediction: Prediction) -> Prediction {
    if labelled(def, CARGO_LABEL) {
        // Two compiler jobs, up to 2.5 GiB each, plus parent/link overhead.
        // Old profiles measured only the largest process: never use them
        // to admit a fresh parallel compiler build with a smaller reserve.
        prediction.memory = prediction.memory.max(CARGO_MEMORY);
        prediction.cores = prediction.cores.max(2.0);
    }
    prediction
}

pub fn cargo_jobs(caller: Option<&str>, room: u64) -> usize {
    let requested = caller
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(2)
        .clamp(1, 2);
    let memory_jobs = (room.saturating_sub(1 << 30) / (2560 << 20)).clamp(1, 2) as usize;
    requested.min(memory_jobs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compiler_jobs_leave_room_and_keep_a_lower_explicit_limit() {
        assert_eq!(cargo_jobs(None, 8 << 30), 2);
        assert_eq!(cargo_jobs(Some("1"), 8 << 30), 1);
        assert_eq!(cargo_jobs(Some("16"), 8 << 30), 2);
        assert_eq!(cargo_jobs(Some("0"), 8 << 30), 1);
        assert_eq!(cargo_jobs(None, 4 << 30), 1);
        assert_eq!(cargo_jobs(None, 0), 1);
    }
    #[test]
    fn compiler_reservation_covers_old_single_process_profiles() {
        let def = TestDef {
            labels: vec![CARGO_LABEL.into()],
            ..TestDef::default()
        };
        let old = Prediction {
            memory: 512 << 20,
            cores: 1.0,
            wall: 60.0,
            memory_raw: 512 << 20,
            known: true,
        };
        let new = prediction(&def, old);
        if cfg!(target_os = "macos") {
            assert_eq!(new.memory, CARGO_MEMORY);
            assert_eq!(new.cores, 2.0);
        } else {
            assert_eq!(new, old);
        }
    }
}

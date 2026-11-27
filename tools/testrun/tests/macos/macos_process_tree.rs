// SPDX-License-Identifier: MIT
//! Real native accounting of a child and grandchild, in a private process
//! group. No GUI, corpus, persistent profiles or release artifacts.
#![cfg(target_os = "macos")]

use std::fs;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use mitcad_testrun::sys::{self, Process, Spec};

struct Fixture {
    process: Process,
    root: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.process.kill();
        let until = Instant::now() + Duration::from_secs(5);
        while self.process.poll().is_none() && Instant::now() < until {
            sleep(Duration::from_millis(10));
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn samples_children_and_grandchildren_together_then_reaps_the_group() {
    let root = std::env::temp_dir().join(format!("mitcad-native-tree-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let script = r#"
import os, sys, time
from pathlib import Path
root = Path(sys.argv[1])
child = os.fork()
if child == 0:
    grandchild = os.fork()
    memory = bytearray(64 * 1024 * 1024)
    for i in range(0, len(memory), 4096): memory[i] = 1
    (root / ('grandchild' if grandchild == 0 else 'child')).touch()
    while not (root / 'release').exists(): time.sleep(0.01)
    if grandchild != 0: os.waitpid(grandchild, 0)
    os._exit(0)
os.waitpid(child, 0)
"#;
    let process = sys::spawn(&Spec {
        program: "python3".into(),
        args: vec![
            "-c".into(),
            script.into(),
            root.to_string_lossy().into_owned(),
        ],
        log: root.join("children.log"),
        limit: None,
        seq: 1,
        cgroup: false,
        env: vec![],
    })
    .unwrap();
    let mut fixture = Fixture { process, root };
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(
            fixture.process.poll().is_none(),
            "fixture ended before release"
        );
        if fixture.root.join("child").exists()
            && fixture.root.join("grandchild").exists()
            && fixture
                .process
                .current()
                .is_some_and(|rss| rss >= 128 << 20)
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "the two live 64 MiB allocations were not measured together"
        );
        sleep(Duration::from_millis(20));
    }
    fs::write(fixture.root.join("release"), "").unwrap();
    loop {
        if let Some(exit) = fixture.process.poll() {
            assert!(exit.success);
            break;
        }
        assert!(Instant::now() < until, "child or grandchild was not reaped");
        sleep(Duration::from_millis(20));
    }
    let usage = fixture.process.usage();
    assert!(usage.tree);
    assert!(usage.peak >= 128 << 20);
    assert!(
        !usage.over_memory,
        "sampled accounting must not claim a hard limit"
    );
}

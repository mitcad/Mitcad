// SPDX-License-Identifier: MIT
//! The import's time and memory guards: tries that give way, stops, memory pressure and kernel calls that do not return.

use super::*;

/// The block's import (sketch, new body, join) on a thread of its own, its
/// first extrusion's kernel call running `hook`: the report and how many
/// extrusions the kernel was asked for.
fn import_block_on_thread(
    options: Options,
    hook: Box<dyn FnMut(&MockKernel) + Send>,
) -> std::thread::JoinHandle<(DesignReport, usize)> {
    std::thread::spawn(move || {
        let (items, mut geometry) = block_history(Vec::new());
        let kernel = TestKernel::default();
        let mut hook = Some(hook);
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        *kernel.on_call.borrow_mut() = Some((
            "extrude_feature",
            Box::new(move |mock: &MockKernel| {
                counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if let Some(mut h) = hook.take() {
                    h(mock);
                }
            }),
        ));
        let mut doc = Document::new(kernel);
        let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, &options);
        (report, calls.load(std::sync::atomic::Ordering::Relaxed))
    })
}

#[test]
fn a_try_the_watchdog_gave_up_does_no_more_items() {
    wall_clock();
    // The first extrusion's kernel call blocks past the hang limit (until
    // released); the watchdog gives the try up meanwhile (mitcad#71).
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    let progress = Arc::new(crate::Progress::new());
    let (entered, blocked) = mpsc::channel::<()>();
    let (release, released) = mpsc::channel::<()>();
    let worker = import_block_on_thread(
        Options {
            progress: Some(progress.clone()),
            ..Options::default()
        },
        Box::new(move |_| {
            entered.send(()).unwrap();
            released.recv().unwrap();
        }),
    );
    blocked
        .recv_timeout(std::time::Duration::from_secs(30))
        .unwrap();
    // No step while the call blocks: the watchdog's hang.
    let step = progress.step.load(Ordering::Relaxed);
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(progress.step.load(Ordering::Relaxed), step);
    assert_eq!(progress.item.load(Ordering::Relaxed), 1);
    progress.abandon();
    assert!(progress.abandoned());
    release.send(()).unwrap();
    let (report, extrusions) = worker.join().unwrap();
    // Once released it did no more items nor definitions: the join after
    // it was never started, nor the final comparison, and no definition
    // was tried after the blocked one.
    assert_eq!(report.items.len(), 2, "{}", report.text());
    assert_eq!(extrusions, 1, "{}", report.text());
    assert_eq!(progress.item.load(Ordering::Relaxed), 1);
    assert!(report.bodies.is_empty(), "{}", report.text());
    assert!(progress.failed_items().is_empty());
}

#[test]
fn an_extrusion_out_of_memory_takes_its_state_as_do_the_items_while_memory_is_low() {
    // The kernel's first extrusion fails for lack of memory (a C++
    // allocation that failed, mitcad#80): the process is low on memory, and
    // with no memory guard to say it recovered, it stays so.
    let (items, mut geometry) = block_history(Vec::new());
    let kernel = TestKernel::default();
    kernel.out_of_memory.set(1);
    let mut doc = Document::new(kernel);
    let progress = Arc::new(crate::Progress::new());
    let options = Options {
        progress: Some(progress.clone()),
        ..Options::default()
    };
    let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, &options);
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Fallback, Outcome::Fallback],
        "{}",
        report.text()
    );
    let note = |i: usize| report.items[i].note.clone().unwrap_or_default();
    assert!(note(1).contains(LOW_MEMORY), "{}", report.text());
    assert!(note(2).contains(LOW_MEMORY), "{}", report.text());
    // No extrusion was evaluated after the one that ran out of memory.
    assert_eq!(doc.kernel().mock.count("extrude_feature"), 0);
    assert_eq!(doc.bodies().len(), 1);
    let low = report.low_memory.clone().expect("the report says where");
    assert_eq!(low.item, Some(1));
    assert_eq!(low.name.as_deref(), Some("Extrude1"));
    assert_eq!(low.items, 2);
    assert!(
        low.memory.contains(KernelError::OUT_OF_MEMORY),
        "{}",
        low.memory
    );
    assert!(
        report.warnings.iter().any(|w| w.contains(
            "ran low on memory at Extrude1 (a geometry kernel operation ran out of memory"
        ) && w.ends_with("compared by volume only")),
        "{}",
        report.text()
    );
    // Not the items' own failures: a try run again tries them.
    assert!(progress.failed_items().is_empty());
}

#[test]
fn a_definition_cut_short_for_lack_of_memory_falls_back_and_the_import_goes_on() {
    wall_clock();
    // While the first extrusion is evaluated, the memory guard finds the
    // process low on memory (mitcad#80): the evaluation stops, as the
    // kernel's long operations do on request, and the item takes its
    // state. The memory has recovered for the join after it.
    let progress = Arc::new(crate::Progress::new());
    let guard = progress.clone();
    let worker = import_block_on_thread(
        Options {
            progress: Some(progress.clone()),
            ..Options::default()
        },
        Box::new(move |mock| {
            guard.low_memory("9.9 GiB of 10.0 GiB (test)");
            slow_call(mock);
            guard.memory_recovered();
        }),
    );
    let (report, extrusions) = worker.join().unwrap();
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Fallback, Outcome::Fallback],
        "{}",
        report.text()
    );
    let note = |i: usize| report.items[i].note.clone().unwrap_or_default();
    assert!(note(1).contains(LOW_MEMORY), "{}", report.text());
    // The join was tried (the mock kernel's join on the fallback's body is
    // not its state).
    assert!(!note(2).contains(LOW_MEMORY), "{}", report.text());
    assert!(extrusions > 1, "{}", report.text());
    let low = report.low_memory.clone().expect("the report says where");
    assert_eq!((low.item, low.items), (Some(1), 1));
    assert_eq!(low.memory, "9.9 GiB of 10.0 GiB (test)");
    let warning = report
        .warnings
        .iter()
        .find(|w| w.contains("ran low on memory"))
        .expect("a warning");
    assert!(
        warning.ends_with("cut short or not tried and 1 modeling items took the file's bodies"),
        "{warning}"
    );
}

#[test]
fn the_failed_operation_names_a_failed_allocation_the_guard_saw_first() {
    // The memory guard polls on its own thread: it can count a failed
    // allocation before the operation that failed reports it. The report
    // tells the operation's words, once, and not those of a later low;
    // also when the guard saw the memory recover meanwhile (a failed
    // allocation leaves the memory as it was).
    let progress = crate::Progress::new();
    progress.low_memory_unattributed("an allocation in the geometry kernel failed (test)");
    progress.low_memory("building a body of the file: out of memory (test)");
    progress.low_memory("extrude: out of memory (test)");
    assert_eq!(progress.memory_lows(), 1);
    let (memory, _) = progress.first_low_memory().expect("low");
    assert_eq!(memory, "building a body of the file: out of memory (test)");
    let progress = crate::Progress::new();
    progress.low_memory_unattributed("an allocation in the geometry kernel failed (test)");
    progress.memory_recovered();
    progress.low_memory("extrude: out of memory (test)");
    assert!(progress.is_memory_low());
    let (memory, _) = progress.first_low_memory().expect("low");
    assert_eq!(memory, "extrude: out of memory (test)");
    // A first low the memory guard measured is not renamed.
    let progress = crate::Progress::new();
    progress.low_memory("9.9 GiB of 10.0 GiB (test)");
    progress.low_memory("extrude: out of memory (test)");
    progress.name_failed_allocation("fillet: out of memory (test)");
    let (memory, _) = progress.first_low_memory().expect("low");
    assert_eq!(memory, "9.9 GiB of 10.0 GiB (test)");
    // A worker's operation names the failure, before or after the guard
    // counted it, without making the import low on memory itself.
    let progress = crate::Progress::new();
    progress.name_failed_allocation("extrude: out of memory (test)");
    assert!(!progress.is_memory_low());
    progress.low_memory_unattributed("an allocation in the geometry kernel failed (test)");
    let (memory, _) = progress.first_low_memory().expect("low");
    assert_eq!(memory, "extrude: out of memory (test)");
    let progress = crate::Progress::new();
    progress.low_memory_unattributed("an allocation in the geometry kernel failed (test)");
    progress.name_failed_allocation("extrude: out of memory (test)");
    let (memory, _) = progress.first_low_memory().expect("low");
    assert_eq!(memory, "extrude: out of memory (test)");
}

#[test]
fn the_next_item_waits_for_the_memory_guard_to_see_the_memory_recover() {
    wall_clock();
    // The memory guard measures the process every so often: it sees the
    // memory recover after the definition cut short only a little later,
    // and the join after it is tried all the same.
    let progress = Arc::new(crate::Progress::new());
    let guard = progress.clone();
    let worker = import_block_on_thread(
        Options {
            progress: Some(progress.clone()),
            ..Options::default()
        },
        Box::new(move |mock| {
            guard.low_memory("9.9 GiB of 10.0 GiB (test)");
            slow_call(mock);
            let later = guard.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                later.memory_recovered();
            });
        }),
    );
    let (report, _) = worker.join().unwrap();
    let note = |i: usize| report.items[i].note.clone().unwrap_or_default();
    assert!(note(1).contains(LOW_MEMORY), "{}", report.text());
    assert!(!note(2).contains(LOW_MEMORY), "{}", report.text());
}

#[test]
fn memory_tight_drops_what_the_import_can_build_again() {
    wall_clock();
    // Asked to while the first extrusion is evaluated (mitcad#80), the
    // import drops before its next item the cached results of the
    // definitions it tried and the bodies of the history states behind it;
    // what it imports stays the same.
    let run = |tight: bool| {
        let progress = Arc::new(crate::Progress::new());
        let guard = progress.clone();
        let worker = import_block_on_thread(
            Options {
                progress: Some(progress.clone()),
                ..Options::default()
            },
            Box::new(move |_| {
                if tight {
                    guard.tight_on_memory();
                }
            }),
        );
        let (report, _) = worker.join().unwrap();
        (report, progress)
    };
    let (plain, _) = run(false);
    let (report, progress) = run(true);
    let outcomes = |r: &DesignReport| r.items.iter().map(|i| i.outcome).collect::<Vec<_>>();
    assert_eq!(outcomes(&report), outcomes(&plain), "{}", report.text());
    let json = |r: &DesignReport| serde_json::to_value(r).unwrap();
    assert_eq!(json(&report), json(&plain));
    assert!(report.low_memory.is_none());
    assert!(!progress.take_tight());
}

#[test]
fn the_oracle_builds_released_states_again() {
    let (_, mut geometry) = block_history(Vec::new());
    let kernel = TestKernel::default();
    let mut oracle = Oracle::new(&mut geometry);
    let before: Vec<Sig> = oracle.sigs(&kernel, 1);
    oracle.sigs(&kernel, 2);
    oracle.cursor = Some(2);
    assert_eq!(oracle.kept(), 2);
    assert_eq!(oracle.release_behind(), 1);
    assert_eq!(oracle.kept(), 1);
    assert_eq!(oracle.sigs(&kernel, 1), before);
    // (Built again, not counted again.)
    assert_eq!(oracle.built, 2);
}

#[test]
fn the_states_behind_the_replay_are_released_after_each_item() {
    // The bodies of the history states the replay has gone past go before
    // its next item, with what the file's geometry built for them alone
    // (mitcad#885: a design of 116 items kept the bodies of 69 states of
    // 5 000 faces, 1.3 GB, and their measures to its end), without being
    // asked to by the memory guard; the extrusions are verified against
    // their states all the same.
    let (items, mut geometry) = block_history(Vec::new());
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    for item in &report.items[1..] {
        assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
        assert_eq!(item.verified, Some(true), "{}", report.text());
    }
    assert!(report.history.reached_end, "{}", report.text());
    // Once the replay is at each state: the empty one before the first
    // extrusion, the extrusion's before the join, and the join's at the end.
    assert_eq!(geometry.released_before, [0, 1, 2], "{}", report.text());
}

#[test]
fn a_definition_that_takes_longer_than_its_item_is_cut_short() {
    wall_clock();
    // The first extrusion's kernel call would take 30 s, checking for a
    // request to stop as the geometry kernel's long operations do; the item
    // has a quarter of a second (mitcad#69). Its only definition has the
    // whole time (of several, the first would give way at its share).
    let started = std::time::Instant::now();
    let worker = import_block_on_thread(
        Options {
            item_seconds: 0.25,
            max_candidates: 1,
            ..Options::default()
        },
        Box::new(slow_call),
    );
    let (report, _) = worker.join().unwrap();
    let elapsed = started.elapsed().as_secs_f64();
    assert!(
        elapsed < 10.0,
        "the item's time did not cut it short: {elapsed} s"
    );
    // The item gave up like one out of time and took its state; the join
    // after it was tried in its own time (the mock kernel's join on the
    // fallback's body is not the state).
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Fallback, Outcome::Fallback],
        "{}",
        report.text()
    );
    let note = |i: usize| report.items[i].note.clone().unwrap_or_default();
    assert!(note(1).contains(OUT_OF_TIME), "{}", report.text());
    // It says which definition the time went to (mitcad#78).
    assert!(
        note(1).contains("; cut short after 0.")
            && note(1).ends_with(" s: definition 1 of 1 (extrude, new_body, 1 profile)"),
        "{}",
        report.text()
    );
    assert!(!note(2).contains(OUT_OF_TIME), "{}", report.text());
}

/// The hang watchdog of `mitcad-ffi` in short: watches the progress of the
/// import `worker` runs and gives the try up ([`Progress::abandon`]) once it
/// made no step for longer than `limit`. The worker's result, and the
/// longest time the import made no step.
fn watched<T>(
    progress: &Progress,
    limit: std::time::Duration,
    worker: std::thread::JoinHandle<T>,
) -> (T, std::time::Duration) {
    use std::sync::atomic::Ordering;
    let mut step = progress.step.load(Ordering::Relaxed);
    let mut moved = std::time::Instant::now();
    let mut longest = std::time::Duration::ZERO;
    while !worker.is_finished() {
        std::thread::sleep(std::time::Duration::from_millis(5));
        let now = progress.step.load(Ordering::Relaxed);
        if now != step {
            step = now;
            moved = std::time::Instant::now();
        }
        longest = longest.max(moved.elapsed());
        if moved.elapsed() > limit {
            progress.abandon();
        }
    }
    (worker.join().unwrap(), longest)
}

/// A design whose first item is a base feature of `bodies` stored bodies
/// (the file's first history state), imported on a thread of its own by a
/// test kernel whose `mass_properties` calls run `hook`.
fn import_base_feature_on_thread(
    bodies: usize,
    progress: Arc<Progress>,
    hook: Box<dyn FnMut(&MockKernel) + Send>,
) -> std::thread::JoinHandle<DesignReport> {
    std::thread::spawn(move || {
        let stored: Vec<MockShape> = (0..bodies)
            .map(|i| MockShape::imported(&format!("stored body {i}"), 6))
            .collect();
        let mut geometry = TestGeometry {
            states: vec![Vec::new(), stored],
            items: HashMap::from([(0, 1)]),
            ..TestGeometry::default()
        };
        let items = vec![json!({"index": 0, "name": "Base Feature1", "objectType": "BaseFeature"})];
        let kernel = TestKernel::default();
        *kernel.on_call.borrow_mut() = Some(("mass_properties", hook));
        let mut doc = Document::new(kernel);
        let options = Options {
            progress: Some(progress),
            ..Options::default()
        };
        import_design(&mut doc, &timeline_dump(items), &mut geometry, &options)
    })
}

#[test]
fn long_loops_of_the_import_tick_the_watchdog() {
    wall_clock();
    // Measuring the bodies of a large history state takes long, a kernel
    // call each (mitcad#82: 110 s on the first item of a large design,
    // whose hang limit was 60 s): here 30 bodies of 20 ms each, measured
    // as the state, as the replay's and as the stored design's, under a
    // hang limit of 0.3 s. The import ticks between the calls, so the
    // watchdog does not take it for a hang.
    let limit = std::time::Duration::from_millis(300);
    let progress = Arc::new(Progress::new());
    let started = std::time::Instant::now();
    let worker = import_base_feature_on_thread(
        30,
        progress.clone(),
        Box::new(|_| std::thread::sleep(std::time::Duration::from_millis(20))),
    );
    let (report, longest) = watched(&progress, limit, worker);
    let took = started.elapsed();
    assert!(took > 2 * limit, "the import took only {took:?}");
    assert!(
        !progress.abandoned(),
        "no step for {longest:?}: {}",
        report.text()
    );
    // (The file's own base feature: its stored bodies are its translation.)
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(outcomes, [Outcome::Parametric], "{}", report.text());
    assert_eq!(report.bodies.len(), 30, "{}", report.text());
}

#[test]
fn a_kernel_call_that_does_not_return_still_looks_hung() {
    wall_clock();
    // The same import, one measurement blocking for longer than the hang
    // limit (a kernel call that does not return): the ticks around it do
    // not hide it, and the watchdog gives the try up.
    let limit = std::time::Duration::from_millis(300);
    let progress = Arc::new(Progress::new());
    let mut calls = 0;
    let worker = import_base_feature_on_thread(
        30,
        progress.clone(),
        Box::new(move |_| {
            calls += 1;
            if calls == 5 {
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }
        }),
    );
    let (report, longest) = watched(&progress, limit, worker);
    assert!(progress.abandoned(), "{}", report.text());
    assert!(longest > limit, "{longest:?}");
    // Given up: the stored design was not compared.
    assert!(report.bodies.is_empty(), "{}", report.text());
}

#[test]
fn a_join_of_the_same_bodies_waits_behind_one_that_gave_way() {
    let mut turns = Turns::default();
    let combine = |target: &str, tools: &[&str], operation: &str, keep: bool| {
        Candidate::new(json!({"type": "combine", "target": target, "tools": tools,
                              "operation": operation, "keep_tools": keep}))
    };
    let candidates = vec![
        combine("F2.b0", &["F12.b0"], "join", false),
        combine("F12.b0", &["F2.b0"], "join", false),
        combine("F12.b0", &["F2.b0"], "join", true),
        combine("F12.b0", &["F2.b0"], "cut", false),
        combine("F2.b0", &["F12.b0", "F3.b0"], "join", false),
        combine("F12.b0", &["F2.b0"], "intersect", false),
    ];
    assert!(!turns.waits_behind(&candidates, 1));
    turns.cut_short(0, 20.0, true);
    // The same union with the other body as the target, keeping its tools
    // or not, waits behind the first; a cut, a union of more bodies and an
    // intersection are other booleans.
    assert!(turns.waits_behind(&candidates, 1));
    assert!(turns.waits_behind(&candidates, 2));
    assert!(!turns.waits_behind(&candidates, 3));
    assert!(!turns.waits_behind(&candidates, 4));
    assert!(!turns.waits_behind(&candidates, 5));
    // With the time they ran: tried again only when more is left.
    assert_eq!(turns.again(25.0), Some(0));
    assert_eq!(turns.again(25.0), Some(1));
    assert_eq!(turns.again(15.0), None);
}

#[test]
fn definitions_that_gave_way_wait_for_the_rest_of_the_time() {
    let mut turns = Turns::default();
    // A share of the item's time each, the last one the rest.
    assert_eq!(turns.share(60.0, false, false), Some(SHARE * 60.0));
    assert_eq!(turns.share(60.0, true, false), None);
    turns.cut_short(1, 15.0, true);
    turns.cut_short(0, 16.0, true);
    // While some wait, the last one has a share too; they have the rest.
    assert_eq!(turns.share(60.0, true, false), Some(SHARE * 60.0));
    assert_eq!(turns.share(60.0, false, true), None);
    let join = json!({"type": "extrude", "operation": "join",
                      "profiles": [{"sketch": "F1", "region": "r{c1}"}]});
    let mut limited = join.clone();
    limited["participants"] = json!(["F2.b0"]);
    let candidates = vec![
        Candidate::new(json!({"type": "fillet"})),
        Candidate::new(join),
        Candidate::new(limited),
    ];
    // The join limited to participants waits behind the join untried.
    assert!(turns.waits_behind(&candidates, 2));
    assert!(!turns.waits_behind(&candidates, 0));
    // By rank, while more time is left than they ran.
    assert_eq!(turns.again(20.0), Some(1));
    assert_eq!(turns.again(16.0), Some(2));
    assert_eq!(turns.again(14.0), None);
    turns.cut_short(1, 19.0, false);
    let (seconds, note) = turns.note(&candidates).unwrap();
    assert_eq!(seconds, 19.0);
    assert_eq!(
        note,
        "cut short after 19.0 s: definition 2 of 3 (extrude, join, 1 profile) (and 1 other)"
    );
}

#[test]
fn a_slow_definition_gives_way_to_the_next_ones() {
    wall_clock();
    // The file's extrusion goes against the sketch's normal; the one
    // along it is tried first and would take 30 s: at its share of the
    // item's 2 s it gives way, and the next one gives the state (mitcad#78).
    // Before, it used up the item's time.
    let slow = SlowExtrusions {
        regions: &[LARGE],
        along: true,
        against: false,
        seconds: 30.0,
        deaf: false,
    };
    for known in [true, false] {
        let (report, def, slow, elapsed) =
            slow_profile_import(known, (LARGE, 10.0, true), slow, 2.0);
        assert_eq!(
            report.items[1].outcome,
            Outcome::Parametric,
            "{}",
            report.text()
        );
        assert_eq!(report.items[1].verified, Some(true));
        assert_eq!(def["profiles"][0]["region"], LARGE, "{}", report.text());
        assert_eq!(def["flip"], true, "{}", report.text());
        assert_eq!(slow, 1, "{}", report.text());
        assert!(elapsed < 10.0, "{elapsed} s");
    }
}

#[test]
fn a_definition_that_gave_way_gets_the_rest_of_the_time() {
    wall_clock();
    // The file's extrusion, tried first, takes 1 s, more than its share (a
    // third of 2 s): it gives way, the others give no state, and it is
    // tried again with the rest of the item's time and gives the state.
    let slow = SlowExtrusions {
        regions: &[LARGE],
        along: true,
        against: false,
        seconds: 1.0,
        deaf: false,
    };
    for known in [true, false] {
        let (report, def, slow, _) = slow_profile_import(known, (LARGE, 10.0, false), slow, 2.0);
        assert_eq!(
            report.items[1].outcome,
            Outcome::Parametric,
            "{}",
            report.text()
        );
        assert_eq!(def["profiles"][0]["region"], LARGE, "{}", report.text());
        assert_ne!(def["flip"], true, "{}", report.text());
        assert_eq!(slow, 2, "{}", report.text());
    }
}

#[test]
fn an_item_out_of_time_names_the_definition_its_time_went_to() {
    wall_clock();
    // No definition gives the file's state (an extrusion by 20 mm), and
    // the one of both rectangles along the sketch's normal takes longer
    // than the item's second: it gives way, is tried again with the rest
    // of the time and cut short there; the item falls back and says which
    // definition its time went to.
    let slow = SlowExtrusions {
        regions: &[LARGE, SMALL],
        along: true,
        against: false,
        seconds: 30.0,
        deaf: false,
    };
    for known in [true, false] {
        let (report, _, slow, elapsed) =
            slow_profile_import(known, (SMALL, 20.0, false), slow, 1.0);
        assert_eq!(
            report.items[1].outcome,
            Outcome::Fallback,
            "{}",
            report.text()
        );
        let note = report.items[1].note.clone().unwrap_or_default();
        assert!(
            note.contains(&format!("{OUT_OF_TIME}; cut short after 0."))
                && note.contains(" (extrude, new_body, 2 profiles)"),
            "{}",
            report.text()
        );
        assert!(slow >= 1, "{}", report.text());
        assert!(elapsed < 10.0, "{elapsed} s");
    }
}

#[test]
fn points_along_a_segment_are_inside_between_its_crossings() {
    use mitcad_import_replay::internal::ops::inside_at;
    // In at a quarter, out at a half, in again at three quarters.
    let crossings = [(0.25, true), (0.5, false), (0.75, true)];
    let inside: Vec<bool> = [0.1, 0.3, 0.6, 0.9]
        .iter()
        .map(|t| inside_at(&crossings, *t))
        .collect();
    assert_eq!(inside, [false, true, false, true]);
    // Out at a half only: inside before it.
    assert!(inside_at(&[(0.5, false)], 0.2));
    assert!(!inside_at(&[(0.5, false)], 0.7));
    // No crossing: outside.
    assert!(!inside_at(&[], 0.5));
}

/// A thread on the side of a cylinder, its offset of the threaded face
/// (the sizing offset) running `offset`: the document, the report and the
/// seconds the import took.
fn import_thread_with_offset(
    offset: Box<dyn FnMut(&MockKernel) + Send>,
) -> (Document<TestKernel>, DesignReport, f64) {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let mut items = vec![
        circle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let side = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "side")
        .cloned()
        .expect("a side face");
    let info = json!({"_type": "ThreadInfo", "threadType": "ISO Metric profile",
        "threadDesignation": "M10x1.5", "threadClass": "6g", "isInternal": false,
        "majorDiameter": 0.985, "minorDiameter": 0.8141});
    items.push(thread_item(
        2,
        info,
        true,
        1.0,
        face_point(&side).map(|x| x / 10.0),
    ));
    let k = kernel();
    *k.on_call.borrow_mut() = Some(("offset_faces", offset));
    let mut doc = Document::new(k);
    let started = std::time::Instant::now();
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    (doc, report, started.elapsed().as_secs_f64())
}

#[test]
fn a_sizing_offset_that_takes_long_gives_way_to_the_next_definition() {
    wall_clock();
    // The offset of the threaded face would work for 30 s: it is cut short
    // at its few seconds and the thread comes in without it (mitcad#68).
    let (doc, report, elapsed) = import_thread_with_offset(Box::new(|mock: &MockKernel| {
        busy_call_for(mock, 30.0);
    }));
    assert!(
        (SIZING_OFFSET_SECONDS..10.0).contains(&elapsed),
        "{elapsed} s"
    );
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    assert!(
        !doc.features()
            .any(|f| matches!(f.def, FeatureDef::OffsetFace(_))),
        "{}",
        report.text()
    );
}

#[test]
fn a_sizing_offset_slowed_by_the_load_beside_it_is_not_cut_short() {
    // The offset works a second longer than its time on the wall clock,
    // on a machine so busy with other work that the import gets a quarter
    // of the processors it wants: its time is on the import's clock, which
    // leaves the load out (mitcad#874), so the thread keeps its faces
    // sized by the offset (0.75 s of its 2 s).
    mitcad_model::monitor::pace_for_tests(Some(0.25));
    let (doc, report, elapsed) = import_thread_with_offset(Box::new(|mock: &MockKernel| {
        busy_call_for(mock, SIZING_OFFSET_SECONDS + 1.0);
    }));
    mitcad_model::monitor::pace_for_tests(None);
    assert!(elapsed >= SIZING_OFFSET_SECONDS + 1.0, "{elapsed} s");
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    assert!(
        doc.features()
            .any(|f| matches!(f.def, FeatureDef::OffsetFace(_))),
        "{}",
        report.text()
    );
}

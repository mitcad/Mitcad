// SPDX-License-Identifier: MIT
//! Hostile block references (mitcad#926): an INSERT array of 10^8 cells and
//! nested arrays that multiply read in bounded memory and time, with
//! warnings. Its own test binary: it counts the allocated bytes.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use mitcad_dxf::{Drawing, Geometry, MAX_ARRAY_CELLS, Point2, READ_BUDGET, read_str, read_within};

/// The system allocator, counting the bytes allocated now and at most.
struct Counting;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grew(bytes: usize) {
    let now = CURRENT.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

// SAFETY: every call is passed on to the system allocator unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            grew(layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            grew(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            if new_size >= layout.size() {
                grew(new_size - layout.size());
            } else {
                CURRENT.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        p
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The tests of this file one at a time, so each measures only itself.
static SERIAL: Mutex<()> = Mutex::new(());

/// The drawing `read` returns, with the most bytes allocated meanwhile
/// beyond those allocated before, and the time it took.
fn measured(read: impl FnOnce() -> Drawing) -> (Drawing, usize, Duration) {
    let base = CURRENT.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let start = Instant::now();
    let drawing = read();
    let elapsed = start.elapsed();
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
    (drawing, peak, elapsed)
}

/// DXF text of group pairs.
fn dxf(pairs: &[(i32, &str)]) -> String {
    pairs
        .iter()
        .map(|(code, value)| format!("{code}\n{value}\n"))
        .collect()
}

/// A file with the given BLOCKS and ENTITIES section groups.
fn file(blocks: &[(i32, &str)], model: &[(i32, &str)]) -> String {
    let mut pairs = vec![(0, "SECTION"), (2, "BLOCKS")];
    pairs.extend_from_slice(blocks);
    pairs.extend_from_slice(&[(0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
    pairs.extend_from_slice(model);
    pairs.extend_from_slice(&[(0, "ENDSEC"), (0, "EOF")]);
    dxf(&pairs)
}

/// The groups of a block `name` (base point at the origin) holding `items`.
fn block<'a>(name: &'a str, items: &[(i32, &'a str)]) -> Vec<(i32, &'a str)> {
    let mut pairs = vec![
        (0, "BLOCK"),
        (8, "0"),
        (2, name),
        (70, "0"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
    ];
    pairs.extend_from_slice(items);
    pairs.push((0, "ENDBLK"));
    pairs
}

/// A line from (0, 0) to (1, 0) on layer 0.
const LINE: &[(i32, &str)] = &[
    (0, "LINE"),
    (8, "0"),
    (10, "0"),
    (20, "0"),
    (11, "1"),
    (21, "0"),
];

/// An INSERT of `name` at the origin, an array of `columns` × `rows` cells
/// spaced `spacing` apart.
fn array<'a>(
    name: &'a str,
    columns: &'a str,
    rows: &'a str,
    spacing: &'a str,
) -> Vec<(i32, &'a str)> {
    vec![
        (0, "INSERT"),
        (8, "Arrays"),
        (2, name),
        (10, "0"),
        (20, "0"),
        (70, columns),
        (71, rows),
        (44, spacing),
        (45, spacing),
    ]
}

fn warned(drawing: &Drawing, text: &str) -> usize {
    drawing.warnings.iter().filter(|w| w.contains(text)).count()
}

#[test]
fn an_insert_of_10000_by_10000_cells_reads_in_bounded_memory() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let text = file(&block("ONE", LINE), &array("ONE", "10000", "10000", "2"));
    let (drawing, peak, _) = measured(|| read_str(&text).unwrap());
    // Before mitcad#926 the 10^8 cells' transforms alone took 4.8 GB.
    assert!(peak < 200 << 20, "{peak} bytes");
    assert_eq!(drawing.entities.len() as u64, MAX_ARRAY_CELLS);
    assert_eq!(
        warned(
            &drawing,
            "array of 10000 × 10000 cells: only the first 100000 cells (10 rows of 10000) are read"
        ),
        1,
        "{:?}",
        drawing.warnings
    );
    // The last cell read: column 9999 of row 9.
    match drawing.entities.last().map(|e| &e.geometry) {
        Some(Geometry::Line { start, .. }) => {
            assert_eq!(*start, Point2::new(19998.0, 18.0));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_array_of_a_missing_block_is_one_warning() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // The scenario of mitcad#926: the block need not exist.
    let text = file(&[], &array("NOPE", "10000", "10000", "1"));
    let (drawing, peak, elapsed) = measured(|| read_str(&text).unwrap());
    assert!(peak < 200 << 20, "{peak} bytes");
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    assert!(drawing.entities.is_empty());
    assert_eq!(warned(&drawing, "array of 10000 × 10000 cells"), 1);
    assert_eq!(
        warned(&drawing, "block NOPE is missing"),
        1,
        "{:?}",
        drawing.warnings
    );
}

#[test]
fn nested_arrays_of_1000_cells_stop_at_the_budget() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // Three levels of 10 × 100 cells: 10^9 lines without the budget.
    let mut blocks = block("C", LINE);
    blocks.extend(block("B", &array("C", "10", "100", "2")));
    blocks.extend(block("A", &array("B", "10", "100", "300")));
    let text = file(&blocks, &array("A", "10", "100", "40000"));
    let (drawing, peak, elapsed) = measured(|| read_str(&text).unwrap());
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    assert!(peak < 400 << 20, "{peak} bytes");
    let placed = drawing.entities.len() as u64;
    assert!(placed > 0 && placed < READ_BUDGET, "{placed}");
    assert_eq!(
        warned(
            &drawing,
            "the drawing places more than 1000000 entities and block references"
        ),
        1,
        "{:?}",
        drawing.warnings
    );
    // Each array is within the cell cap: no cap warning.
    assert_eq!(
        warned(&drawing, "only the first"),
        0,
        "{:?}",
        drawing.warnings
    );
    // Entities on layer 0 take the layer of the outermost reference.
    assert!(drawing.entities.iter().all(|e| e.layer == "Arrays"));
}

#[test]
fn the_budget_counts_entities_and_references() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut two = LINE.to_vec();
    two.extend_from_slice(LINE);
    let text = file(&block("TWO", &two), &array("TWO", "3", "2", "5"));
    // 6 cells of 2 lines: 6 references and 12 lines.
    let all = read_within(text.as_bytes(), 18).unwrap();
    assert_eq!(all.entities.len(), 12);
    assert!(all.warnings.is_empty(), "{:?}", all.warnings);
    // Three cells with their lines take 9; the fourth cell the tenth.
    let cut = read_within(text.as_bytes(), 10).unwrap();
    assert_eq!(cut.entities.len(), 6);
    assert_eq!(
        warned(&cut, "the drawing places more than 10 entities"),
        1,
        "{:?}",
        cut.warnings
    );
}

#[test]
fn a_missing_block_inside_an_array_is_reported_once() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let inner = [(0, "INSERT"), (2, "NOPE"), (10, "0"), (20, "0")];
    let text = file(&block("HOLDER", &inner), &array("HOLDER", "10", "10", "1"));
    let drawing = read_str(&text).unwrap();
    assert!(drawing.entities.is_empty());
    assert_eq!(drawing.warnings.len(), 1, "{:?}", drawing.warnings);
    assert!(drawing.warnings[0].contains("block NOPE is missing"));
}

#[test]
fn a_wide_array_keeps_its_first_rows() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let text = file(&block("ONE", LINE), &array("ONE", "300", "1000", "2"));
    let drawing = read_str(&text).unwrap();
    // 100000 / 300 = 333 whole rows.
    assert_eq!(drawing.entities.len(), 300 * 333);
    assert_eq!(
        warned(
            &drawing,
            "array of 300 × 1000 cells: only the first 99900 cells (333 rows of 300) are read"
        ),
        1,
        "{:?}",
        drawing.warnings
    );
    match drawing.entities.last().map(|e| &e.geometry) {
        Some(Geometry::Line { start, .. }) => {
            assert_eq!(*start, Point2::new(598.0, 664.0));
        }
        other => panic!("{other:?}"),
    }
}

// SPDX-License-Identifier: MIT
//! Readers of files the user chose, run in the application's own process
//! (mitcad#922): the `dxf_info` and `svg_info` queries, the DXF and SVG
//! inserts and the print settings' read. A panic in such a reader (a bug
//! that a hostile or broken file reaches) would cross the C++ bridge,
//! which aborts the process and loses the session's unsaved work; run
//! through [`catch_panic`], it is the reader's error, with the panic's
//! message. (The imports of whole CAD files and their listings of
//! references and links run in the import process instead, where a fault
//! ends only that process.)

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

/// For tests (mitcad#922): the file (its file name, or its whole path)
/// whose reader panics, as a reader's bug would, wherever
/// [`test_panic`] is called for it: the readers [`catch_panic`] runs, and
/// the import process's listings of an `.f3d`/`.f3z` design's references
/// and an `.rvt` project's links.
pub const TEST_PANIC_VARIABLE: &str = "MITCAD_TEST_PARSER_PANIC";

/// Panics when [`TEST_PANIC_VARIABLE`] names the file `path`; does
/// nothing otherwise (also when the variable is not set).
pub fn test_panic(path: &str) {
    let Some(named) = std::env::var_os(TEST_PANIC_VARIABLE) else {
        return;
    };
    if named.is_empty() {
        return;
    }
    let file = Path::new(path);
    if file.as_os_str() == named || file.file_name() == Some(named.as_os_str()) {
        panic!(
            "a test panic in the reader of {} ({TEST_PANIC_VARIABLE})",
            file.display()
        );
    }
}

/// Runs `read`, a reader of the file `path`, on the calling thread; a
/// panic in it is returned as the error `error` makes of a text naming the
/// file and the panic's message. The reader must leave nothing half
/// changed that outlives it (it reads the file into data of its own).
pub fn catch_panic<T, E>(
    path: &str,
    read: impl FnOnce() -> Result<T, E>,
    error: impl FnOnce(String) -> E,
) -> Result<T, E> {
    match catch_unwind(AssertUnwindSafe(|| {
        test_panic(path);
        read()
    })) {
        Ok(result) => result,
        Err(payload) => Err(error(panic_text(path, payload))),
    }
}

/// The error text of a reader's panic.
fn panic_text(path: &str, payload: Box<dyn Any + Send>) -> String {
    let name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);
    format!(
        "{name}: the file's reader failed (an internal error): {}",
        panic_message(payload)
    )
}

/// A panic's message: its text, or a note that it had none.
pub fn panic_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(text) => *text,
        Err(payload) => payload.downcast_ref::<&str>().map_or_else(
            || "a panic without a message".to_owned(),
            |s| (*s).to_owned(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panicking_reader_is_an_error_with_its_message() {
        let read: Result<u32, String> = catch_panic(
            "/some/folder/drawing.dxf",
            || -> Result<u32, String> { panic!("an index out of range in the reader") },
            |e| e,
        );
        let error = read.unwrap_err();
        assert!(
            error.starts_with("drawing.dxf: the file's reader failed"),
            "{error}"
        );
        assert!(
            error.contains("an index out of range in the reader"),
            "{error}"
        );
        // A formatted message too.
        let read: Result<(), String> = catch_panic(
            "plate.3mf",
            || -> Result<(), String> { panic!("entry {} of {}", 3, 2) },
            |e| e,
        );
        assert!(read.unwrap_err().contains("entry 3 of 2"));
    }

    #[test]
    fn a_reader_that_does_not_panic_gives_its_own_result() {
        assert_eq!(catch_panic("a.dxf", || Ok::<_, String>(7), |e| e), Ok(7));
        assert_eq!(
            catch_panic("a.dxf", || Err::<u32, _>("no such file".to_owned()), |e| e),
            Err("no such file".to_owned())
        );
    }

    #[test]
    fn panic_messages_of_both_kinds() {
        assert_eq!(panic_message(Box::new("static text")), "static text");
        assert_eq!(panic_message(Box::new("owned".to_owned())), "owned");
        assert_eq!(panic_message(Box::new(5_u8)), "a panic without a message");
    }
}

// SPDX-License-Identifier: MIT
//! The ASM history stream of `.smbh` blobs. After the live entities come
//! `Begin-of-ASM-History-Data` (`history_stream`), one `delta_state` record
//! per operation and `End-of-ASM-History-Section`, then copies of entities
//! as they were before the operations. Each delta state lists bulletins
//! that pair an entity with its copy. Rolling the states back gives the
//! bodies as they were before the newest operations.
//!
//! Layouts *(verified on the corpus, meanings partly assumed)*:
//! - `history_stream`: integers (current state number twice, 0, a count),
//!   then pointers into the delta states: -1, the current state, the oldest
//!   state, -1. These pointers number the delta states from 0 in file
//!   order.
//! - `delta_state`: state number, 1, 0, then pointers to the newer and the
//!   older state and to itself, -1, 0, a boolean, then bulletin boards:
//!   `1 <state> 2` followed by bulletins `1 <before> <after>` and a 0, the
//!   list of boards ending with a 0, then one more integer.
//! - Bulletin pointers use the numbering of the entities, in which the
//!   history section takes no numbers (see [`AsmFile::record_of`]).

use std::collections::HashMap;

use super::file::AsmFile;
use super::geom::{Cursor, GeomError, Result};
use super::token::Token;

/// The changes of one operation.
#[derive(Clone, Debug, PartialEq)]
pub struct DeltaState {
    /// State number; later states have larger numbers.
    pub id: i64,
    /// (before, after) as record indices: `(None, Some(e))` for an entity
    /// the operation created, `(Some(c), None)` for one it deleted (the
    /// copy `c` stands for the entity), `(Some(c), Some(e))` for a change
    /// (`c` holds the data of `e` before the operation).
    pub bulletins: Vec<(Option<usize>, Option<usize>)>,
}

impl DeltaState {
    /// Numbers of created, changed and deleted entities.
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut n = (0, 0, 0);
        for b in &self.bulletins {
            match b {
                (None, Some(_)) => n.0 += 1,
                (Some(_), Some(_)) => n.1 += 1,
                (Some(_), None) => n.2 += 1,
                (None, None) => {}
            }
        }
        n
    }
}

/// The history of an `.smbh` blob.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    /// From the current state back to the oldest.
    pub states: Vec<DeltaState>,
}

fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(GeomError(msg.into()))
}

/// Pointers of a record's fields, in order.
fn pointers(file: &AsmFile, record: usize) -> Vec<i64> {
    file.fields(record)
        .iter()
        .filter_map(|t| match t {
            Token::Ptr(p) => Some(*p),
            _ => None,
        })
        .collect()
}

fn parse_state(file: &AsmFile, record: usize) -> Result<(DeltaState, i64, i64)> {
    let r = &file.records[record];
    let mut c = Cursor::new(&file.tokens, r.fields.start);
    let id = c.int()?;
    c.int()?;
    c.int()?;
    let _newer = c.ptr()?;
    let older = c.ptr()?;
    let this = c.ptr()?;
    c.ptr()?;
    c.ptr()?;
    c.boolean()?;
    let entity = |p: i64| -> Result<Option<usize>> {
        if p < 0 {
            return Ok(None);
        }
        file.record_of(p)
            .map(Some)
            .ok_or_else(|| GeomError(format!("bulletin pointer {p} outside the file")))
    };
    let mut bulletins = Vec::new();
    while c.int()? != 0 {
        c.ptr()?; // the board's state
        c.int()?;
        while c.int()? != 0 {
            let before = entity(c.ptr()?)?;
            let after = entity(c.ptr()?)?;
            bulletins.push((before, after));
        }
    }
    if c.pos > r.fields.end {
        return err(format!("delta_state {record} is shorter than expected"));
    }
    Ok((DeltaState { id, bulletins }, older, this))
}

impl History {
    /// The history of the file; `None` when it has none.
    pub fn parse(file: &AsmFile) -> Result<Option<History>> {
        let Some(section) = file.history_section.clone() else {
            return Ok(None);
        };
        if section.is_empty()
            || file.records[section.start].type_name != "Begin-of-ASM-History-Data"
        {
            return err("history section without its header");
        }
        let first_state = section.start + 1;
        let mut parsed = Vec::new();
        for (i, record) in (first_state..section.end).enumerate() {
            if file.records[record].type_name != "delta_state" {
                return err(format!(
                    "record {record} in the history section is not a delta_state"
                ));
            }
            let (state, older, this) = parse_state(file, record)?;
            if this != i as i64 {
                return err(format!("delta_state {record} calls itself {this}"));
            }
            parsed.push((state, older));
        }
        let current = match pointers(file, section.start).get(1) {
            Some(&p) if p >= 0 && (p as usize) < parsed.len() => p as usize,
            _ => return err("history_stream without a current state"),
        };
        let mut states = Vec::new();
        let mut at = Some(current);
        while let Some(i) = at {
            if states.len() > parsed.len() {
                return err("delta states form a cycle");
            }
            let (state, older) = &parsed[i];
            states.push(state.clone());
            at = usize::try_from(*older).ok().filter(|&j| j < parsed.len());
        }
        Ok(Some(History { states }))
    }

    /// Entity -> record holding its data after rolling back the newest
    /// `steps` states (`None`: the entity did not exist yet). Entities not
    /// in the map are unchanged.
    pub fn view(&self, steps: usize) -> HashMap<usize, Option<usize>> {
        let mut view = HashMap::new();
        for state in self.states.iter().take(steps) {
            // A deleted entity (no `after`) needs nothing: its copy stands
            // for it.
            for &(before, after) in &state.bulletins {
                if let Some(entity) = after {
                    view.insert(entity, before);
                }
            }
        }
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asm::writer::Writer;

    /// A file with two live points, a history of two states and copies:
    /// state 2 moved point 1 (copy 3) and created point 2; state 1 created
    /// point 1.
    fn history_file() -> AsmFile {
        let mut w = Writer::new(2, 3);
        w.record("asmheader").ptr(-1).int(-1).str("x").end();
        w.record("point").head().pos([1.0, 0.0, 0.0]).end();
        w.record("point").head().pos([2.0, 0.0, 0.0]).end();
        w.record("Begin-of-ASM-History-Data")
            .ident("history_stream")
            .int(2)
            .int(2)
            .int(0)
            .int(9)
            .ptr(-1)
            .ptr(0)
            .ptr(1)
            .ptr(-1)
            .end();
        // State 2 (index 0): newer -1, older 1, itself 0.
        w.record("delta_state")
            .int(2)
            .int(1)
            .int(0)
            .ptr(-1)
            .ptr(1)
            .ptr(0)
            .ptr(-1)
            .ptr(0);
        w.bool(false).int(1).ptr(0).int(2);
        w.int(1)
            .ptr(3)
            .ptr(1)
            .int(1)
            .ptr(-1)
            .ptr(2)
            .int(0)
            .int(0)
            .int(0)
            .end();
        // State 1 (index 1): created point 1.
        w.record("delta_state")
            .int(1)
            .int(1)
            .int(0)
            .ptr(0)
            .ptr(-1)
            .ptr(1)
            .ptr(-1)
            .ptr(0);
        w.bool(false).int(1).ptr(1).int(2);
        w.int(1).ptr(-1).ptr(1).int(0).int(0).int(0).end();
        w.record("End-of-ASM-History-Section");
        w.record("point").head().pos([0.5, 0.0, 0.0]).end();
        AsmFile::parse(&w.finish()).unwrap()
    }

    #[test]
    fn parses_and_rolls_back_delta_states() {
        let f = history_file();
        assert_eq!(f.history_section, Some(3..6));
        // Pointer 3 is the copy, record 6.
        assert_eq!(f.record_of(3), Some(6));
        assert_eq!(f.record_of(2), Some(2));
        let h = History::parse(&f).unwrap().unwrap();
        assert_eq!(h.states.len(), 2);
        assert_eq!(h.states[0].id, 2);
        assert_eq!(
            h.states[0].bulletins,
            vec![(Some(6), Some(1)), (None, Some(2))]
        );
        assert_eq!(h.states[0].counts(), (1, 1, 0));
        assert!(h.view(0).is_empty());
        let one = h.view(1);
        assert_eq!(one.get(&1), Some(&Some(6)));
        assert_eq!(one.get(&2), Some(&None));
        assert_eq!(h.view(2).get(&1), Some(&None));
    }
}

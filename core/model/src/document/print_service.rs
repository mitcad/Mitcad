// SPDX-License-Identifier: MIT
//! Components not for print services (mitcad#2214). A library may say in
//! its manifest that its components, or some of them, must not be printed
//! as a service (`non_printable`, `docs/libraries.md`); the design keeps
//! that on each inserted part's library record
//! ([`crate::library::LibraryRef::non_printable`]) and on BUILD's library
//! sources, so that it holds without the library.
//!
//! The flag belongs to the component and its occurrences, never to the
//! design: a body is not for print services where it is placed inside such
//! a component (its own component, or any component its occurrence path
//! passes through: a nested occurrence), or when a placed part of such a
//! source made it. The design's own bodies and the other components stay
//! printable. The user's own export and Send to Slicer are not affected;
//! the `print_service_check` query (`commands.md`, "Print service check")
//! answers which bodies a future cloud print service may take.

use super::Document;
use crate::features::FeatureDef;
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::library::LibraryRef;

/// Why a body at a place may not go to a print service: the component
/// whose library record says so, and the record.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintServiceBlock {
    /// The component of the library part (the body's own, or one its
    /// occurrence path passes through; the root for a placed part made in
    /// the root).
    pub component: ComponentUid,
    /// How many occurrences of the body's path lead to that component
    /// (`path[..depth]` places it); the whole path for the body's own
    /// component.
    pub depth: usize,
    /// Its library record (the library, the designation, the licence).
    pub record: LibraryRef,
}

impl<K: Kernel> Document<K> {
    /// The library record of a component when it says the component is
    /// not for print services: its own record (a library part, linked or
    /// copied), else the record its BUILD or options entry gives it (the
    /// library source it is made of). None for every other component.
    pub fn non_printable_record(&self, component: ComponentUid) -> Option<LibraryRef> {
        let c = self.state.assembly.component(component)?;
        if let Some(record) = c.library.as_ref().filter(|l| l.non_printable) {
            return Some(record.clone());
        }
        if let Some(source) = c
            .build
            .as_ref()
            .and_then(|b| b.source)
            .and_then(|id| self.state.library_sources.get(id))
            .filter(|s| s.library.non_printable)
        {
            let mut record = source.library.clone();
            if let Some(entry) = self.build_list_entry(component) {
                record = entry.library;
            }
            return Some(record);
        }
        self.options_list_entry(component)
            .map(|entry| entry.library)
            .filter(|l| l.non_printable)
    }

    /// Whether body `body`, placed by the occurrences `path` (from the root
    /// down to its component; empty for the root's bodies and a body
    /// placed nowhere), may go to a print service: None when it may, else
    /// the library part that forbids it. A placed part made of a
    /// non-printable source and the body's own component come first, then
    /// the components of the path from the innermost out.
    pub fn print_service_block(
        &self,
        body: BodyUid,
        path: &[OccurrenceUid],
    ) -> Option<PrintServiceBlock> {
        let own = self.body_component(body).unwrap_or(ComponentUid::ROOT);
        // A placed part made of a non-printable source, in any component.
        if let Some(FeatureDef::DerivedPart(def)) = self.state.entry(body.feature).map(|e| &e.def)
            && let Some(source) = self
                .state
                .library_sources
                .get(def.source)
                .filter(|s| s.library.non_printable)
        {
            let record = self
                .non_printable_record(own)
                .unwrap_or_else(|| source.library.clone());
            return Some(PrintServiceBlock {
                component: own,
                depth: path.len(),
                record,
            });
        }
        if let Some(record) = self.non_printable_record(own) {
            return Some(PrintServiceBlock {
                component: own,
                depth: path.len(),
                record,
            });
        }
        let assembly = &self.state.assembly;
        for depth in (1..=path.len()).rev() {
            let Some(occurrence) = assembly.occurrence(path[depth - 1]) else {
                continue;
            };
            if let Some(record) = self.non_printable_record(occurrence.component) {
                return Some(PrintServiceBlock {
                    component: occurrence.component,
                    depth,
                    record,
                });
            }
        }
        None
    }
}

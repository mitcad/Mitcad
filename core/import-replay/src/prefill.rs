// SPDX-License-Identifier: MIT
//! Components of other documents filled before the replay (mitcad#196).
//!
//! The caller that imports the documents a design refers to hands them to
//! [`crate::replay_design`]. Once the replay has placed each occurrence
//! of such a component as an occurrence of an empty component
//! ([`crate::components`]), they are copied into those components with their
//! histories ([`crate::references::Linker`]), so that the items that name
//! their joints' sides, faces, bodies and sketches inside them find them as
//! they find those of any other component. The replay matches the design's
//! own history, which holds none of their bodies: the document leaves the
//! filled components' bodies out of [`Document::bodies`] while the import
//! runs ([`Document::set_import_scopes`]; a reference into another
//! document looks among those of its scope, `refs.rs`), and the matching
//! looks at the bodies outside them. The occurrences inside them start
//! where the design's last captured positions put them, as the design's
//! own do; the captured positions come in as features at the end
//! ([`crate::references::Linker::finish`]).

use std::collections::HashMap;

use mitcad_model::Kernel;

use crate::Importer;
use crate::references::{Linker, split_variants};

impl<K: Kernel> Importer<'_, K> {
    /// Fills the components of other documents from `sources` and puts the
    /// occurrences inside them where the design's captured positions leave
    /// them. Components of documents not in `sources` stay empty.
    pub(crate) fn fill_references(&mut self) {
        let Some(sources) = self.sources else {
            return;
        };
        if self.report.components.inserted.is_empty() {
            return;
        }
        // Occurrences the captured positions arrange differently inside
        // get components of their own, each filled with its own copy.
        for (of, made) in split_variants(self.doc, &mut self.report, sources) {
            self.components.add_inserted_copy(of, made);
        }
        let mut linker = Linker::default();
        let mut documents: Vec<String> = Vec::new();
        for i in &self.report.components.inserted {
            if !documents.contains(&i.document) {
                documents.push(i.document.clone());
            }
        }
        for document in documents {
            if let Some(source) = sources.get(&document) {
                linker.fill(self.doc, &mut self.report, &document, source);
            }
        }
        if self.report.components.filled.is_empty() {
            // (The positions inside are left out at the end, with why.)
            self.linker = Some(linker);
            return;
        }
        // Components placed several times inside a copy, whose occurrences
        // the design arranges differently, get a copy each.
        linker.separate(self.doc, &mut self.report);
        self.components.set_filled(&self.report.components.filled);
        // The occurrences inside start where the design's captured
        // positions leave them.
        let last = linker.plan(self.doc, &self.report);
        // (Every placement the positions gave them, for the items between.)
        for (index, occurrence, local) in linker.trail() {
            let stored = self
                .doc
                .assembly()
                .occurrence(occurrence)
                .map_or(mitcad_model::Transform::IDENTITY, |o| o.transform);
            self.captures.add_inside(occurrence, stored, index, local);
        }
        for (_, occurrence, local) in last {
            if let Err(e) = self.doc.set_occurrence_transform(occurrence, local, false) {
                let name = self.doc.assembly().occurrence_name(occurrence);
                self.report
                    .warnings
                    .push(format!("{name}: its captured position: {e}"));
            }
        }
        // The documents' own scopes (the GUIDs that name entities in them).
        let mut scopes: Vec<(String, Vec<mitcad_model::ComponentUid>)> = Vec::new();
        for i in &self.report.components.inserted {
            if !self.report.components.filled.contains(&i.component) {
                continue;
            }
            let name = i
                .document_guid
                .as_deref()
                .map_or_else(|| i.document.clone(), str::to_lowercase);
            match scopes.iter_mut().find(|(n, _)| *n == name) {
                Some((_, components)) => components.push(i.component),
                None => scopes.push((name, vec![i.component])),
            }
        }
        self.doc.set_import_scopes(scopes.clone());
        self.find_contexts(&scopes, &mut linker);
        if crate::tracing() {
            let a = self.doc.assembly();
            for o in &a.occurrences {
                let mut chain = vec![o.uid];
                let mut at = o.parent;
                while !at.is_root() {
                    let Some(up) = a.occurrences_of(at).next() else {
                        break;
                    };
                    chain.push(up.uid);
                    at = up.parent;
                }
                chain.reverse();
                let t = self.doc.path_transform(&chain);
                eprintln!(
                    "import: occurrence {} is at {:?} {:?}",
                    a.occurrence_name(o.uid),
                    t.translation.map(|v| (v * 1000.0).round() / 1000.0),
                    t.linear.map(|r| r.map(|v| (v * 1000.0).round() / 1000.0)),
                );
            }
        }
        self.linker = Some(linker);
    }

    /// The captured positions inside the filled components come in as
    /// features; the document's body view is the whole one again.
    pub(crate) fn finish_references(&mut self) {
        if let Some(linker) = self.linker.take() {
            self.report.components.link = linker.finish(self.doc, &mut self.report);
        }
        self.doc.set_import_scopes(Vec::new());
        self.doc.set_import_focus(HashMap::new());
        // Its occurrences by their object ids, with those of the documents
        // filled in, for a design that inserts this one (mitcad#2528).
        self.report.components.object_guids = std::sync::Arc::new(crate::references::object_guids(
            self.doc,
            &self.report.components,
            self.sources,
        ));
    }
}

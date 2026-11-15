// SPDX-License-Identifier: MIT
//! Bodies by their records (mitcad#1802, mitcad#1808).
//!
//! A body input refers to its body record, which names the body object
//! (`_f3d.body_entity`); that object's record among the file's body
//! records (`_f3d.appearance.bodies`) gives the blob and ASM record of the
//! stored body (`blob#record`, [`crate::history::StoredBody::source`]). In
//! the history state the replay stands for, that stored body has its
//! faces and measures, and the replay's body with the same is the body
//! the input names.
//!
//! The item that made a body and the body's index among its bodies
//! (`producers.rs`) name it too, but that index counts the bodies the
//! item made in the file, not those the replay has: where a removal or a
//! cut and paste took one of them, or a fallback brought the item's bodies
//! with others, the index names another body. The record goes first, and
//! the body it gives refuses the producer's index where they differ; the
//! index is the fallback where the record does not settle the body.
//!
//! A record is where the body is at the end of the timeline: a body a
//! later cut and paste moved into another component has its record in that
//! component's blob, and an earlier state of a blob can hold another body
//! under the same record number. So a body that a cut and paste at or
//! after the item moves is not looked for by its record (in a reported
//! design, 3 cut and paste items took other bodies of their destination's
//! blob by their sources' records), and where the item that made the body made
//! one body with its own features and the input names it by the first
//! index, that body is taken whatever the record says (in a reported
//! design, the record of an earlier state named another body for such a
//! reference, whose producer's body the history state after it confirmed).
//! A wider check, that the record's body be one its producer made, lost 83
//! removals and 8 other items there: the replay does not follow bodies
//! that cut and paste or body transfers moved to their producers.

use mitcad_dump::Fingerprint;
use std::collections::HashMap;

use mitcad_model::{BodyUid, ComponentUid, Kernel};

use crate::Importer;
use crate::history::{Sig, StoredBody};

/// Where the record route does not settle a body input, why (for traces).
#[derive(Debug)]
pub(crate) enum Unsettled {
    /// The input names no body record, or the record no stored body.
    NoRecord,
    /// The state the replay stands for is not built, or does not hold the
    /// record's stored body (made after it, or taken out before it).
    NotInState,
    /// No body of the replay is the record's stored body.
    NoBody,
    /// Several of the replay's bodies are, and the item that made the
    /// body does not tell them apart.
    Several(Vec<BodyUid>),
    /// The replay's body that is it is not the only body the item that
    /// made the body made: the record names another body in this state.
    NotProduced(BodyUid),
    /// A cut and paste at or after the item moves the body into another
    /// component: its record is in that component's blob, not where the
    /// body is now.
    CutLater,
}

impl<K: Kernel> Importer<'_, K> {
    /// The stored body (`blob#record`) of a body input's record, where the
    /// file's body records name its body object.
    pub(crate) fn record_source(&self, fp: &Fingerprint) -> Option<String> {
        let entity = fp.f3d.as_ref()?.body_entity?;
        let record = self
            .dump
            .f3d
            .as_ref()?
            .appearance
            .as_ref()?
            .bodies
            .as_ref()?
            .iter()
            .find(|b| b.entity == Some(entity))?;
        Some(format!("{}#{}", record.blob.as_ref()?, record.record?))
    }

    /// Whether a body record of the file names the stored body (`blob#record`)
    /// and keeps it in the design (not `removed`).
    pub(crate) fn kept_in_design(&self, source: &str) -> bool {
        self.dump
            .f3d
            .as_ref()
            .and_then(|f| f.appearance.as_ref())
            .and_then(|a| a.bodies.as_ref())
            .into_iter()
            .flatten()
            .any(|b| {
                b.removed != Some(true)
                    && b.blob
                        .as_deref()
                        .zip(b.record)
                        .is_some_and(|(blob, record)| source == format!("{blob}#{record}"))
            })
    }

    /// The body objects a cut and paste moves into another component, with
    /// the last item that moves each ([`Importer::cut_sources`]).
    fn cut_sources(&self) -> &HashMap<u64, i64> {
        self.cut_sources.get_or_init(|| {
            let mut out = HashMap::new();
            let items = self.dump.timeline.as_ref().and_then(|t| t.items.as_deref());
            for item in items.unwrap_or_default() {
                let cut = item.object_type() == Some("CopyPasteBodies")
                    && item
                        .f3d
                        .as_ref()
                        .and_then(|f| f.base_name.as_deref())
                        .is_some_and(|n| n.starts_with("Cut"));
                let Some(index) = item.index.filter(|_| cut) else {
                    continue;
                };
                let Some(detail) = item
                    .detail
                    .as_ref()
                    .and_then(|d| serde_json::to_value(d).ok())
                else {
                    continue;
                };
                let sources = detail
                    .get("sourceBody")
                    .and_then(serde_json::Value::as_array);
                for entity in sources.into_iter().flatten().filter_map(|s| {
                    s.pointer("/_f3d/body_entity")
                        .and_then(serde_json::Value::as_u64)
                }) {
                    let last = out.entry(entity).or_insert(index);
                    *last = (*last).max(index);
                }
            }
            out
        })
    }

    /// The replay's solids of `component` with their signatures (as
    /// [`Importer::current_bodies`] measures them).
    fn component_bodies(&self, component: ComponentUid) -> Vec<(BodyUid, Sig)> {
        let kernel = self.doc.kernel();
        crate::body_scope::bodies(self.doc)
            .iter()
            .filter(|b| self.doc.body_component(b.uid) == Some(component))
            .filter_map(|b| Some((b.uid, self.measured(Sig::of(kernel, b.shape)?, b.uid))))
            .collect()
    }

    /// A replay body's signature as the file's body it stands for measures
    /// where it carries an accepted deviation ([`Importer::corrected`]).
    fn measured(&self, sig: Sig, uid: BodyUid) -> Sig {
        match self.deviated.get(&uid) {
            Some(o) => o.apply(&sig),
            None => sig,
        }
    }

    /// The replay's body that is the stored body `source` of the state
    /// `stored`: of the replay's bodies in the stored body's component,
    /// the one with its faces and measures, else the one within the
    /// import's tolerance (a replayed split piece can have its faces split
    /// otherwise than the file's); where several are (copies pasted where
    /// their originals are), the one the item that made the body made
    /// (`producer`, any of its bodies: the bodies are the same but for
    /// their maker). With the stored body's id.
    pub(crate) fn body_of_record(
        &self,
        source: &str,
        producer: Option<i64>,
        stored: &[(StoredBody<K::Shape>, Sig)],
    ) -> Result<(BodyUid, u64), Unsettled> {
        let (body, sig) = stored
            .iter()
            .find(|(b, _)| b.source == source)
            .ok_or(Unsettled::NotInState)?;
        let id = body.id.ok_or(Unsettled::NotInState)?;
        let current = self.component_bodies(self.components.of_body(body.component));
        let mut fits: Vec<BodyUid> = current
            .iter()
            .filter(|(_, s)| s.same(sig))
            .map(|(u, _)| *u)
            .collect();
        if fits.is_empty() {
            fits = current
                .iter()
                .filter(|(_, s)| s.matches(sig))
                .map(|(u, _)| *u)
                .collect();
        }
        if fits.len() > 1
            && let Some(producer) = producer
        {
            let (made, _) = self.made_by(producer);
            let own: Vec<BodyUid> = fits.iter().copied().filter(|b| made.contains(b)).collect();
            if !own.is_empty() {
                fits = own;
            }
        }
        match fits[..] {
            [] => Err(Unsettled::NoBody),
            [one] => Ok((one, id)),
            _ => Err(Unsettled::Several(fits)),
        }
    }

    /// A body input by its record in the state the replay stands for
    /// ([`Importer::body_of_record`]).
    pub(crate) fn recorded_body(&self, fp: &Fingerprint) -> Result<BodyUid, Unsettled> {
        let source = self.record_source(fp).ok_or(Unsettled::NoRecord)?;
        if let (Some(entity), Some(now)) = (
            fp.f3d.as_ref().and_then(|f| f.body_entity),
            self.translating,
        ) && self
            .cut_sources()
            .get(&entity)
            .is_some_and(|&last| last >= now)
        {
            if crate::tracing() {
                eprintln!("import: a body by its record {source}: a cut and paste moves it later");
            }
            return Err(Unsettled::CutLater);
        }
        let stored = self
            .oracle
            .cursor
            .and_then(|q| self.oracle.built_state(q))
            .ok_or(Unsettled::NotInState)?;
        let producer = fp.f3d.as_ref().and_then(|f| f.producer);
        let mut found = self.body_of_record(&source, producer, stored);
        // The only body of an item whose own features made one, named by
        // its first index, is that body whatever the record says (see the
        // module's notes).
        if let Ok((uid, _)) = found
            && let Some(producer) = producer
            && fp.f3d.as_ref().and_then(|f| f.body_index) == Some(0)
            && let (made, true) = self.made_by(producer)
            && let [only] = made[..]
            && only != uid
        {
            found = Err(Unsettled::NotProduced(uid));
        }
        if crate::tracing() {
            match &found {
                Ok((uid, _)) => eprintln!("import: a body by its record {source}: {uid}"),
                Err(Unsettled::Several(all)) => eprintln!(
                    "import: a body by its record {source}: several of the replay's: {all:?}"
                ),
                Err(Unsettled::NoBody) => {
                    eprintln!("import: a body by its record {source}: none of the replay's")
                }
                Err(Unsettled::NotProduced(uid)) => eprintln!(
                    "import: a body by its record {source}: {uid}, which the item that made the body did not make"
                ),
                Err(_) => {}
            }
        }
        found.map(|(uid, _)| uid)
    }
}

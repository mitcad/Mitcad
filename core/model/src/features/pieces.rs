// SPDX-License-Identifier: MIT
//! Body ids of the pieces a feature splits a body into (mitcad#408).
//!
//! A cut or intersection (an extrusion's and the other profile features',
//! a combine's, a primitive's, a pattern's or mirror's copies), a split
//! body or an assembly-scoped cut that leaves a body in several pieces
//! gives the body's id to one piece and new ids of the feature
//! (`<feature>.b0`, …) to the others. The first time the pieces come in
//! geometric order: the first keeps the body. The feature then remembers
//! each piece's volume and centre of mass ([`PieceGroup`], saved with the
//! feature in the design file), and later evaluations give the ids by
//! matching the pieces with the remembered ones, so that an upstream edit
//! that changes the order or the number of the pieces does not hand a later
//! feature's body to another piece:
//!
//! - A piece matches a remembered one when its volume is within a factor of
//!   two of it and its centre moved less than a quarter of the size of the
//!   pieces (the diagonal of their bounding box). The closest pairs are
//!   taken first; ties go to the remembered piece listed first, then to the
//!   piece first in geometric order.
//! - The body split keeps its id for the piece that matches it. Without a
//!   match it takes the piece left that comes closest to it (volume and
//!   centre), and the features that name the body warn that it is another
//!   piece now; when every piece matches another remembered piece, the body
//!   is gone.
//! - A remembered piece without a match is gone: its id goes to no other
//!   piece (it stays remembered, so a piece like it that comes back gets it
//!   again). Remove Body passes such a body by with a warning; other
//!   features that name it fail saying so. Pieces without a match get new
//!   ids.
//!
//! A body left whole that never was in pieces is not remembered, and
//! pieces the kernel cannot measure come in geometric order. What a
//! feature remembers is an input of its evaluation, as a parameter is
//! ([`Read::Pieces`]): a result fits both the memory it was made with and
//! the memory it leaves, so taking that memory into the document does not
//! make the next recompute evaluate the feature again.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use mitcad_vec3::distance;

use super::{EvalContext, is_false};
use crate::fingerprint::Fingerprint;
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{BoundingBox, Kernel};
use crate::recompute::Read;

/// A piece a feature left of a body, as it remembers it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PieceMark {
    pub body: BodyUid,
    /// Its volume (mm³).
    pub volume: f64,
    /// Its centre of mass (mm), in its component's coordinates.
    pub centre: [f64; 3],
    /// The last evaluation made no such piece.
    #[serde(default, skip_serializing_if = "is_false")]
    pub gone: bool,
}

/// The pieces a feature left of one body. The body's own piece comes
/// first, then the others in the order of their ids.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PieceGroup {
    /// The body the feature split.
    pub body: BodyUid,
    /// Which split of the body it is, when the feature splits the body (or
    /// its pieces) more than once (a pattern's copies): 0 for the first.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pass: u32,
    /// The diagonal of the pieces' bounding box (mm).
    pub size: f64,
    pub pieces: Vec<PieceMark>,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// What every feature remembers, by feature.
pub type SplitPieces = BTreeMap<FeatureUid, Arc<Vec<PieceGroup>>>;

/// Bodies that are other pieces of a split than before, with the warning of
/// the features that name them.
pub(crate) type ChangedPieces = Vec<(BodyUid, String)>;

/// A piece's volume may change by this factor and still match.
const VOLUME_FACTOR: f64 = 2.0;
/// A piece's centre may move by this part of the pieces' size and still
/// match.
const SHIFT: f64 = 0.25;

/// The pieces of an evaluating feature: what it remembers and what it
/// makes.
#[derive(Debug, Clone, Default)]
pub(crate) struct PieceState {
    remembered: Option<Arc<Vec<PieceGroup>>>,
    /// Indices of the feature's own bodies remembered: new bodies skip them.
    pub(crate) reserved: BTreeSet<u32>,
    made: Vec<PieceGroup>,
    passes: BTreeMap<BodyUid, u32>,
    /// The memory took part in the evaluation.
    pub(crate) used: bool,
    changed: ChangedPieces,
}

impl PieceState {
    pub(crate) fn new(feature: FeatureUid, remembered: Option<Arc<Vec<PieceGroup>>>) -> Self {
        let reserved = remembered
            .iter()
            .flat_map(|groups| groups.iter())
            .flat_map(|g| &g.pieces)
            .filter(|p| p.body.feature == feature)
            .map(|p| p.body.index)
            .collect();
        Self {
            remembered,
            reserved,
            ..Self::default()
        }
    }

    /// Counts `count` more passes over `body` (each cut of it is one, which
    /// numbers the splits it remembers): a pattern's run of features cut
    /// in one operation passes over the body as often as the features one
    /// by one (mitcad#855).
    pub(crate) fn skip_passes(&mut self, body: BodyUid, count: u32) {
        *self.passes.entry(body).or_insert(0) += count;
    }
}

/// The fingerprint of what a feature remembers (none and nothing alike).
pub(crate) fn fingerprint(groups: Option<&Arc<Vec<PieceGroup>>>) -> u128 {
    let mut fingerprint = Fingerprint::new("split pieces");
    for group in groups.into_iter().flat_map(|g| g.iter()) {
        fingerprint
            .u64(group.body.feature.0)
            .u32(group.body.index)
            .u32(group.pass)
            .f64(group.size)
            .u64(group.pieces.len() as u64);
        for piece in &group.pieces {
            fingerprint
                .u64(piece.body.feature.0)
                .u32(piece.body.index)
                .f64(piece.volume)
                .f64(piece.centre[0])
                .f64(piece.centre[1])
                .f64(piece.centre[2])
                .bool(piece.gone);
        }
    }
    fingerprint.finish()
}

/// A piece measured: volume, centre of mass and bounding box.
struct Measured {
    volume: f64,
    centre: [f64; 3],
    bounds: BoundingBox,
}

fn measure<K: Kernel>(kernel: &K, shape: &K::Shape) -> Option<Measured> {
    let mass = kernel.mass_properties(shape).ok()?;
    let bounds = kernel.bounding_box(shape).ok()??;
    let volume = mass.volume.abs();
    (volume.is_finite() && volume > 0.0 && mass.center.iter().all(|c| c.is_finite())).then_some(
        Measured {
            volume,
            centre: mass.center,
            bounds,
        },
    )
}

/// How far a piece (volume, centre) is from a remembered one: the change
/// of volume and the shift of the centre, each 1 at the most a match
/// allows (`scale`: the pieces' size); infinite for what cannot be
/// compared.
fn closeness(mark: &PieceMark, volume: f64, centre: &[f64; 3], scale: f64) -> (f64, f64) {
    if !(mark.volume > 0.0 && volume > 0.0 && scale > 0.0) {
        return (f64::INFINITY, f64::INFINITY);
    }
    let growth = (volume / mark.volume).ln().abs() / VOLUME_FACTOR.ln();
    let shift = distance(mark.centre, *centre) / (SHIFT * scale);
    (growth, shift)
}

/// How far a piece is from a remembered one, 0 for the same; None when it
/// does not match.
fn cost(mark: &PieceMark, volume: f64, centre: &[f64; 3], scale: f64) -> Option<f64> {
    let (growth, shift) = closeness(mark, volume, centre, scale);
    (growth <= 1.0 && shift <= 1.0).then_some(growth + shift)
}

/// Of the pieces not taken, the one closest to `mark`; the first of them
/// when none can be compared.
fn closest(
    mark: &PieceMark,
    pieces: &[(f64, [f64; 3])],
    free: &[usize],
    scale: f64,
) -> Option<usize> {
    let far = |p: usize| {
        let (growth, shift) = closeness(mark, pieces[p].0, &pieces[p].1, scale);
        growth + shift
    };
    free.iter()
        .copied()
        .min_by(|a, b| far(*a).total_cmp(&far(*b)).then(a.cmp(b)))
}

/// The remembered piece each piece takes (an index into `marks`), the
/// closest pairs first.
fn matches(marks: &[PieceMark], pieces: &[(f64, [f64; 3])], scale: f64) -> Vec<Option<usize>> {
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (m, mark) in marks.iter().enumerate() {
        for (p, (volume, centre)) in pieces.iter().enumerate() {
            if let Some(cost) = cost(mark, *volume, centre, scale) {
                pairs.push((cost, m, p));
            }
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut taken = vec![None; pieces.len()];
    let mut used = vec![false; marks.len()];
    for (_, m, p) in pairs {
        if !used[m] && taken[p].is_none() {
            used[m] = true;
            taken[p] = Some(m);
        }
    }
    taken
}

/// A volume or a distance for messages: whole numbers from 100 up.
fn amount(value: f64) -> String {
    crate::expr::format_number(value, if value.abs() >= 100.0 { 0 } else { 3 })
}

impl<K: Kernel> EvalContext<'_, K> {
    /// Ids for the pieces this feature left of `body`, in geometric order
    /// (see the module comment): the body's own piece first, then the
    /// others. `body` is not among them when no piece is left for it; the
    /// caller removes it then.
    pub(crate) fn piece_ids(
        &mut self,
        body: BodyUid,
        pieces: Vec<K::Shape>,
    ) -> Vec<(BodyUid, K::Shape)> {
        let pass = {
            let next = self.pieces.passes.entry(body).or_insert(0);
            *next += 1;
            *next - 1
        };
        let remembered = self.pieces.remembered.as_ref().and_then(|groups| {
            groups
                .iter()
                .find(|g| g.body == body && g.pass == pass)
                .cloned()
        });
        if remembered.is_none() && pieces.len() < 2 {
            return self.in_order(body, pieces);
        }
        let measured: Option<Vec<Measured>> =
            pieces.iter().map(|p| measure(self.kernel, p)).collect();
        let Some(measured) = measured else {
            // Pieces the kernel cannot measure come in geometric order; what
            // was remembered stays.
            if let Some(group) = remembered {
                self.pieces.used = true;
                self.pieces.made.push(group);
            }
            return self.in_order(body, pieces);
        };
        self.pieces.used = true;
        let size = measured
            .iter()
            .map(|m| m.bounds)
            .reduce(|a, b| a.union(&b))
            .map_or(0.0, |b| distance(b.min, b.max));
        let marks: &[PieceMark] = remembered.as_ref().map_or(&[], |g| &g.pieces);
        let scale = remembered.as_ref().map_or(size, |g| g.size.max(size));
        let found: Vec<(f64, [f64; 3])> = measured.iter().map(|m| (m.volume, m.centre)).collect();
        let taken = matches(marks, &found, scale);
        let mut ids: Vec<Option<BodyUid>> =
            taken.iter().map(|m| m.map(|i| marks[i].body)).collect();
        // The body without a match takes the piece left closest to it (the
        // first when it was not remembered).
        let free: Vec<usize> = (0..ids.len()).filter(|p| ids[*p].is_none()).collect();
        let own = marks.iter().find(|m| m.body == body);
        let fallback = match own {
            Some(mark) => closest(mark, &found, &free, scale),
            None => free.first().copied(),
        };
        if !ids.contains(&Some(body))
            && let Some(taken) = fallback
        {
            ids[taken] = Some(body);
            if let Some(mark) = own {
                let name = self.feature_name(self.uid);
                let piece = &measured[taken];
                self.pieces.changed.push((
                    body,
                    format!(
                        "body {body} is now another piece of what {name} split it into \
                         (volume {} → {} mm³, center moved {} mm)",
                        amount(mark.volume),
                        amount(piece.volume),
                        amount(distance(mark.centre, piece.centre)),
                    ),
                ));
            }
        }
        let ids: Vec<BodyUid> = ids
            .into_iter()
            .map(|id| id.unwrap_or_else(|| self.new_body()))
            .collect();
        // What to remember: the pieces made, then those remembered that are
        // gone.
        let mut kept: Vec<PieceMark> = ids
            .iter()
            .zip(&measured)
            .map(|(id, m)| PieceMark {
                body: *id,
                volume: m.volume,
                centre: m.centre,
                gone: false,
            })
            .collect();
        kept.extend(
            marks
                .iter()
                .filter(|m| !ids.contains(&m.body))
                .map(|m| PieceMark {
                    gone: true,
                    ..m.clone()
                }),
        );
        kept.sort_by_key(|m| (m.body != body, m.body));
        self.pieces.made.push(PieceGroup {
            body,
            pass,
            size,
            pieces: kept,
        });
        let mut named: Vec<(BodyUid, K::Shape)> = ids.into_iter().zip(pieces).collect();
        if let Some(own) = named.iter().position(|(id, _)| *id == body) {
            let first = named.remove(own);
            named.insert(0, first);
        }
        named
    }

    /// The pieces in geometric order: the first keeps the body.
    fn in_order(&mut self, body: BodyUid, pieces: Vec<K::Shape>) -> Vec<(BodyUid, K::Shape)> {
        let mut named = Vec::with_capacity(pieces.len());
        for (i, piece) in pieces.into_iter().enumerate() {
            let id = if i == 0 { body } else { self.new_body() };
            named.push((id, piece));
        }
        named
    }

    /// What the feature remembers after its evaluation and the bodies that
    /// are other pieces now, with their warnings; records the memory as
    /// read. None when the evaluation did not use the memory.
    pub(crate) fn finish_pieces(&mut self) -> (Option<Arc<Vec<PieceGroup>>>, ChangedPieces) {
        if !self.pieces.used {
            return (None, Vec::new());
        }
        let mut groups = std::mem::take(&mut self.pieces.made);
        // Bodies not split this time stay remembered.
        if let Some(before) = &self.pieces.remembered {
            for group in before.iter() {
                if !groups
                    .iter()
                    .any(|g| g.body == group.body && g.pass == group.pass)
                {
                    groups.push(group.clone());
                }
            }
        }
        groups.sort_by_key(|g| (g.body, g.pass));
        let groups = Arc::new(groups);
        let before = fingerprint(self.pieces.remembered.as_ref());
        let after = fingerprint(Some(&groups));
        let mut fits = vec![before];
        if after != before {
            fits.push(after);
        }
        self.reads.push(Read::Pieces(self.uid, fits));
        (Some(groups), std::mem::take(&mut self.pieces.changed))
    }

    /// Why `body` is not there, when it is a piece the feature that made
    /// it no longer makes; the read covers that feature's result.
    pub(crate) fn gone_piece(&mut self, body: BodyUid) -> Option<String> {
        let feature = *self.env.gone_pieces.get(&body)?;
        let version = self
            .env
            .history
            .iter()
            .find(|r| r.uid == feature)
            .and_then(|r| r.output.as_ref())
            .map(|o| o.version);
        let read = Read::Output(feature, version);
        if !self.reads.contains(&read) {
            self.reads.push(read);
        }
        Some(format!(
            "body {body} is a piece {} no longer makes (its pieces changed)",
            self.feature_name(feature)
        ))
    }

    /// Warns when `body` is another piece of a split than before.
    pub(crate) fn warn_changed_piece(&mut self, body: BodyUid) {
        if let Some(message) = self.env.changed_pieces.get(&body) {
            let message = message.clone();
            self.warn(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(index: u32, volume: f64, centre: [f64; 3]) -> PieceMark {
        PieceMark {
            body: BodyUid::new(FeatureUid(7), index),
            volume,
            centre,
            gone: false,
        }
    }

    #[test]
    fn pieces_match_the_closest_remembered_ones() {
        let marks = [
            mark(0, 1000.0, [0.0, 0.0, 0.0]),
            mark(1, 10.0, [100.0, 0.0, 0.0]),
            mark(2, 10.0, [-100.0, 0.0, 0.0]),
        ];
        // The same pieces in another order, a little changed.
        let pieces = [
            (11.0, [-98.0, 0.0, 0.0]),
            (900.0, [5.0, 0.0, 0.0]),
            (9.0, [101.0, 0.0, 0.0]),
        ];
        assert_eq!(
            matches(&marks, &pieces, 400.0),
            vec![Some(2), Some(0), Some(1)]
        );
    }

    #[test]
    fn a_piece_much_larger_smaller_or_farther_matches_nothing() {
        let marks = [mark(0, 1000.0, [0.0, 0.0, 0.0])];
        // Volume beyond a factor of two; centre beyond a quarter of 400 mm.
        let pieces = [
            (2100.0, [0.0, 0.0, 0.0]),
            (450.0, [0.0, 0.0, 0.0]),
            (1000.0, [0.0, 101.0, 0.0]),
        ];
        assert_eq!(matches(&marks, &pieces, 400.0), vec![None, None, None]);
        assert_eq!(
            matches(&marks, &[(1900.0, [0.0, 99.0, 0.0])], 400.0),
            vec![Some(0)]
        );
    }

    #[test]
    fn ties_go_to_the_first_remembered_piece_and_the_first_piece() {
        let marks = [mark(0, 10.0, [0.0; 3]), mark(1, 10.0, [0.0; 3])];
        let pieces = [(10.0, [0.0; 3]), (10.0, [0.0; 3])];
        assert_eq!(matches(&marks, &pieces, 100.0), vec![Some(0), Some(1)]);
    }

    #[test]
    fn the_fingerprint_tells_memories_apart() {
        let one = Arc::new(vec![PieceGroup {
            body: BodyUid::new(FeatureUid(2), 0),
            pass: 0,
            size: 10.0,
            pieces: vec![mark(0, 1.0, [0.0; 3])],
        }]);
        let mut gone = (*one).clone();
        gone[0].pieces[0].gone = true;
        assert_eq!(fingerprint(None), fingerprint(Some(&Arc::new(Vec::new()))));
        assert_ne!(fingerprint(None), fingerprint(Some(&one)));
        assert_ne!(fingerprint(Some(&one)), fingerprint(Some(&Arc::new(gone))));
    }
}

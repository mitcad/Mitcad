// SPDX-License-Identifier: MIT
//! The geometric check of an item's change (mitcad#138).
//!
//! The volume an item adds or removes is compared with the file's change
//! for it ([`crate::history::Change`], within 1 %). Volume and face count
//! cannot tell a wrong rounding (its surface off along its whole length)
//! from results that differ from the file only at the corners of a
//! rounding or by the file's spline approximations (a thin layer below any
//! tolerance): on a small change both are a few per cent. Where the
//! volumes do not settle an item (its change is off by more than
//! [`crate::history::UNSETTLED`]), its faces do: the faces its result made
//! or changed are measured against the file's state after it, and the
//! faces the file's operation made or changed against the result. The
//! result agrees when every main face lies on the other side's faces
//! (within [`TOLERANCE`]) and what does not is confined to corner patches
//! (small faces, each at most [`CORNER`] of its side's new area, together at
//! most [`CORNERS`], none further off than [`CORNER_DISTANCE`]); a main
//! face off is a different result.

use mitcad_model::{BodyUid, Kernel, KernelError};

use crate::history::{self, Sig};
use crate::{trace_clock, tracing};

/// How far (mm) a point of a new face may lie from the other side's faces:
/// the file's approximations stay within 1e-4 mm, a rounding of another
/// size or place lies 0.1 mm and more off.
pub(crate) const TOLERANCE: f64 = 0.01;

/// The part of a side's new area that a face off by more than [`TOLERANCE`]
/// may hold, a corner patch (where a rounding ends or meets another, a few
/// tenths of a per cent each in the designs looked at); a face larger than
/// that is a main face. A wall of an extrusion 0.1 mm off held 9 %.
pub(crate) const CORNER: f64 = 0.02;

/// The part of a side's new area that the corner patches off may hold
/// together.
pub(crate) const CORNERS: f64 = 0.05;

/// How far (mm) a corner patch off may lie from the other side's faces:
/// the corners of roundings that are the file's lay 0.03–0.25 mm off; a
/// patch further off is another corner or another face (2.5 mm and
/// 0.83 mm in the designs looked at).
pub(crate) const CORNER_DISTANCE: f64 = 0.3;

/// The distance (mm) a face whose points lie on the other side's is taken
/// to span for the volume between the sides ([`judge`]): ten times the
/// distance below which a point counts as on a face. (A floor of 0.001 mm
/// let through a cut whose end lay elsewhere on a face of 145 000 mm²: the
/// 16 points missed the 2 mm² it differed by, the 67 mm³ it was off did
/// not show.)
const SPAN_FLOOR: f64 = 1e-5;

/// How much more volume than its faces' area times their distance an
/// item's change may be off the file's ([`judge`]): the points measured
/// are a sample, and a face's largest distance among them can fall short of
/// its largest.
const SPAN_SLACK: f64 = 2.0;

/// Points measured on each new face.
const POINTS: usize = 16;

/// Points measured on each side at most: a pattern's copies can make
/// thousands of faces, which then get fewer points each (at least
/// [`FEWEST`]).
const ALL_POINTS: usize = 3000;

/// The fewest points measured on a new face.
const FEWEST: usize = 4;

/// A face one side made or changed: its area (mm²) and the largest
/// distance (mm) of its points from the other side's faces; None when it
/// is too thin to place points on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Deviation {
    pub area: f64,
    pub largest: Option<f64>,
}

impl Deviation {
    fn off(&self) -> bool {
        self.largest.is_some_and(|d| d > TOLERANCE || d.is_nan())
    }
}

/// What the faces say about a result.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Verdict {
    /// The result is the file's: why, for the report.
    Agrees(String),
    /// It is not: why.
    Differs(String),
}

/// One side's new faces summed up.
#[derive(Debug, Default)]
struct Side {
    /// The area of the faces measured.
    area: f64,
    /// The faces off and their area.
    off: usize,
    off_area: f64,
    /// The largest distance of a face off.
    worst: f64,
    /// The area of the largest face off.
    largest_off: f64,
    /// The volume (mm³) the faces can put between the two sides: each
    /// face's area times its distance, at least [`SPAN_FLOOR`].
    span: f64,
}

impl Side {
    fn of(faces: &[Deviation]) -> Side {
        let mut side = Side::default();
        for f in faces {
            let Some(largest) = f.largest else {
                continue;
            };
            side.area += f.area;
            side.span += f.area * largest.max(SPAN_FLOOR);
            if f.off() {
                side.off += 1;
                side.off_area += f.area;
                side.worst = side.worst.max(largest);
                side.largest_off = side.largest_off.max(f.area);
            }
        }
        side
    }

    /// Whether the faces off are corner patches only.
    fn corners_only(&self) -> bool {
        self.largest_off <= CORNER * self.area
            && self.off_area <= CORNERS * self.area
            && self.worst <= CORNER_DISTANCE
    }

    /// The faces off, for the report (None without any).
    fn describe(&self, whose: &str) -> Option<String> {
        (self.off > 0).then(|| {
            format!(
                "{} of {whose} new faces ({:.1} % of their area) up to {:.3} mm off",
                self.off,
                100.0 * self.off_area / self.area.max(1e-12),
                self.worst
            )
        })
    }
}

/// The verdict on a result whose new faces (`ours`, measured against the
/// file's state) and the file's new faces (`theirs`, measured against the
/// result) deviate so, while the volume it changes is `volume` mm³ off the
/// file's change. The faces must account for that volume: the volume
/// between two sides is about their faces' area times their distance (at
/// most [`SPAN_SLACK`] times what the points measured give: they are a
/// sample, and a part of a face they miss still shows in the volume). None
/// when neither side has a face to measure.
pub(crate) fn judge(ours: &[Deviation], theirs: &[Deviation], volume: f64) -> Option<Verdict> {
    let (a, b) = (Side::of(ours), Side::of(theirs));
    if a.area <= 0.0 && b.area <= 0.0 {
        return None;
    }
    let tolerance = format!("{TOLERANCE} mm");
    let what = [a.describe("its"), b.describe("the file's")]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
    if !a.corners_only() || !b.corners_only() {
        return Some(Verdict::Differs(format!(
            "its faces are not the file's beyond {tolerance}: {what}"
        )));
    }
    let span = SPAN_SLACK * a.span.max(b.span);
    if volume > span || volume.is_nan() {
        return Some(Verdict::Differs(format!(
            "its change differs from the file's by {volume:.4} mm³, more than its faces account \
             for ({span:.4} mm³)"
        )));
    }
    Some(Verdict::Agrees(if a.off == 0 && b.off == 0 {
        format!("its new faces and the file's lie on each other within {tolerance}")
    } else {
        format!(
            "its main faces lie on the file's within {tolerance}; only corner faces differ: {what}"
        )
    }))
}

/// The new faces of one side, each with the shape to measure it against.
struct NewFaces<'s, S> {
    faces: Vec<(mitcad_model::NewFace, &'s S)>,
}

impl<'s, S> NewFaces<'s, S> {
    /// The faces of `after` that the shapes `before` do not have, to be
    /// measured against `other`.
    fn add<K: Kernel<Shape = S>>(
        &mut self,
        kernel: &K,
        before: &[&S],
        after: &S,
        other: &'s S,
    ) -> Result<(), KernelError> {
        let faces = kernel.new_faces(before, after, POINTS)?;
        crate::tick();
        self.faces.extend(faces.into_iter().map(|f| (f, other)));
        Ok(())
    }

    /// Each face with the largest distance of its points from its other
    /// shape's faces, measuring at most [`ALL_POINTS`] points (at least
    /// [`FEWEST`] a face). The largest faces first, and no further than
    /// a main face off, corner patches off beyond [`CORNERS`] or one off
    /// beyond [`CORNER_DISTANCE`]: the faces left then count as on the
    /// other side's (each point measured off a face takes the kernel
    /// milliseconds on a large body).
    fn measure<K: Kernel<Shape = S>>(mut self, kernel: &K) -> Result<Vec<Deviation>, KernelError> {
        self.faces.sort_by(|a, b| b.0.area.total_cmp(&a.0.area));
        let total: f64 = self
            .faces
            .iter()
            .filter(|(f, _)| !f.points.is_empty())
            .map(|(f, _)| f.area)
            .sum();
        let each = (ALL_POINTS / self.faces.len().max(1)).clamp(FEWEST, POINTS);
        let mut out = Vec::new();
        let mut off_area = 0.0;
        let mut decided = false;
        for (f, other) in &self.faces {
            let n = f.points.len();
            if n == 0 || decided {
                let largest = (n > 0).then_some(0.0);
                out.push(Deviation {
                    area: f.area,
                    largest,
                });
                continue;
            }
            let take = n.min(each);
            let points: Vec<[f64; 3]> = (0..take).map(|k| f.points[k * n / take]).collect();
            // (A distance that could not be measured is infinite.)
            let largest = kernel
                .boundary_distances(other, &points)?
                .into_iter()
                .map(|d| if d.is_nan() { f64::INFINITY } else { d })
                .fold(0.0, f64::max);
            crate::tick();
            let face = Deviation {
                area: f.area,
                largest: Some(largest),
            };
            if face.off() {
                off_area += f.area;
                decided = f.area > CORNER * total
                    || off_area > CORNERS * total
                    || largest > CORNER_DISTANCE;
            }
            out.push(face);
        }
        Ok(out)
    }
}

/// A body for the check: its uid in the replay (None for the file's), its
/// signature and its shape.
pub(crate) type Body<S> = (Option<BodyUid>, Sig, S);

/// The bodies of `a` that `b` does not have as they are (the same uid,
/// where both have one, and the same solid, [`Sig::same`]), each of `b`
/// standing for one.
pub(crate) fn changed<'b, S>(a: &'b [Body<S>], b: &[Body<S>]) -> Vec<&'b Body<S>> {
    let mut used = vec![false; b.len()];
    a.iter()
        .filter(|(uid, sig, _)| {
            let same = (0..b.len()).find(|&j| {
                !used[j]
                    && (uid.is_none() || b[j].0.is_none() || *uid == b[j].0)
                    && b[j].1.same(sig)
            });
            match same {
                Some(j) => {
                    used[j] = true;
                    false
                }
                None => true,
            }
        })
        .collect()
}

/// The body of `among` nearest to `sig` ([`Sig::distance`]).
fn nearest<'b, S>(sig: &Sig, among: &[&'b Body<S>]) -> Option<&'b Body<S>> {
    among
        .iter()
        .min_by(|p, q| sig.distance(&p.1).total_cmp(&sig.distance(&q.1)))
        .copied()
}

/// The new faces of one side measured against the other: those of each
/// body of `changed` that the bodies `gone` (which it replaced) do not
/// have, against the nearest body of `others` (of the other side's changed
/// bodies, else of all of its bodies).
fn side<K: Kernel>(
    kernel: &K,
    changed: &[&Body<K::Shape>],
    gone: &[&Body<K::Shape>],
    others: &[&Body<K::Shape>],
    all_others: &[&Body<K::Shape>],
) -> Result<Vec<Deviation>, KernelError> {
    let clock = std::time::Instant::now();
    let gone: Vec<&K::Shape> = gone.iter().map(|b| &b.2).collect();
    let mut faces = NewFaces { faces: Vec::new() };
    for body in changed {
        let pool = if others.is_empty() {
            all_others
        } else {
            others
        };
        let Some(other) = nearest(&body.1, pool) else {
            continue;
        };
        faces.add(kernel, &gone, &body.2, &other.2)?;
    }
    let (count, listed) = (faces.faces.len(), clock.elapsed().as_secs_f64());
    let measured = faces.measure(kernel)?;
    trace!(
        "  geometric check: {count} new faces in {listed:.2} s, measured in {:.2} s",
        clock.elapsed().as_secs_f64() - listed
    );
    Ok(measured)
}

/// The two sides of an item's change: the replay's bodies before it
/// (`before`) and after it (`after`), the file's state the bodies before it
/// stand for (`reference`, empty: no bodies) and its state after it
/// (`target`). The deviations of the result's new faces from the target and
/// of the target's new faces from the result.
pub(crate) fn measure<K: Kernel>(
    kernel: &K,
    before: &[Body<K::Shape>],
    after: &[Body<K::Shape>],
    reference: &[Body<K::Shape>],
    target: &[Body<K::Shape>],
) -> Result<(Vec<Deviation>, Vec<Deviation>), KernelError> {
    let ours = changed(after, before);
    let gone = changed(before, after);
    let theirs = changed(target, reference);
    let replaced = changed(reference, target);
    let all_after: Vec<&Body<K::Shape>> = after.iter().collect();
    let all_target: Vec<&Body<K::Shape>> = target.iter().collect();
    let a = side(kernel, &ours, &gone, &theirs, &all_target)?;
    // (Not the file's faces when a main face of the result is off.)
    if !Side::of(&a).corners_only() {
        return Ok((a, Vec::new()));
    }
    let b = side(kernel, &theirs, &replaced, &ours, &all_after)?;
    Ok((a, b))
}

/// The file's states an item's change is checked against: the state the
/// bodies before it stand for (None: no bodies) and the item's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct States {
    pub before: Option<usize>,
    pub after: usize,
}

/// How an item's result was taken by its change ([`crate::Importer::change_check`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Taken {
    /// Its change's difference from the file's ([`history::own_difference`]).
    pub off: Option<f64>,
    /// The faces' verdict, where they were asked.
    pub faces: Option<String>,
}

impl<K: crate::ImportKernel> crate::Importer<'_, K> {
    /// Whether the item's result, in the document now (its bodies
    /// `current`), is taken by the volume it adds or removes (`own`, the
    /// file's change for it, against its state `target`): within
    /// [`history::UNSETTLED`] of the file's change it is, beyond
    /// [`history::DIFFERENT`] it is not; between them its faces decide
    /// ([`crate::Importer::geometric`], with the replay's bodies `before` it
    /// and the file's `states`), and where they cannot be asked, the
    /// volumes within [`history::CHANGE`]. Err: why it is not.
    pub(crate) fn change_check(
        &mut self,
        name: &str,
        own: Option<history::Change>,
        target: &[Sig],
        current: &[Sig],
        before: &[(BodyUid, K::Shape)],
        states: Option<States>,
    ) -> Result<Taken, String> {
        let off = history::own_difference(own, target, current);
        let (Some(o), Some(change)) = (off, own) else {
            return Ok(Taken { off, faces: None });
        };
        if o <= history::UNSETTLED {
            return Ok(Taken { off, faces: None });
        }
        if o > history::DIFFERENT {
            return Err(crate::changes_otherwise(&change, current));
        }
        let volume = change.difference_volume(current);
        let verdict = states.and_then(|s| self.geometric(name, before, s, volume));
        match verdict {
            Some(Verdict::Agrees(why)) => Ok(Taken {
                off,
                faces: Some(why),
            }),
            Some(Verdict::Differs(why)) => Err(format!(
                "{}; {why}",
                crate::changes_otherwise(&change, current)
            )),
            None if o <= history::CHANGE => Ok(Taken { off, faces: None }),
            None => Err(crate::changes_otherwise(&change, current)),
        }
    }

    /// How the item at `at` in the report was taken by its change.
    pub(crate) fn record_taken(&mut self, at: usize, taken: Taken) {
        let item = &mut self.report.items[at];
        item.change_difference = taken.off;
        item.geometric_check = taken.faces;
    }

    /// The replay's bodies now, for [`crate::Importer::geometric`] (their
    /// shapes are shared, not copied).
    pub(crate) fn body_shapes(&self) -> Vec<(BodyUid, K::Shape)> {
        self.doc
            .bodies()
            .iter()
            .map(|b| (b.uid, b.shape.clone()))
            .collect()
    }

    /// The faces' verdict on the item's result (the document's bodies now)
    /// against its state: the result's new faces against the state, the
    /// state's against the result (`before`: the replay's bodies before the
    /// item, [`crate::Importer::body_shapes`]; `volume`: how far its change
    /// is from the file's, mm³, [`judge`]). None when it cannot be told (a
    /// state not rebuilt, a kernel that does not list new faces, nothing to
    /// measure).
    pub(crate) fn geometric(
        &mut self,
        name: &str,
        before: &[(BodyUid, K::Shape)],
        states: States,
        volume: f64,
    ) -> Option<Verdict> {
        let clock = std::time::Instant::now();
        let kernel = self.doc.kernel();
        let measured = |uid: BodyUid, shape: &K::Shape| {
            Sig::of(kernel, shape).map(|sig| (Some(uid), sig, shape.clone()))
        };
        let before: Vec<Body<K::Shape>> =
            before.iter().filter_map(|(u, s)| measured(*u, s)).collect();
        let after: Vec<Body<K::Shape>> = self
            .doc
            .bodies()
            .iter()
            .filter_map(|b| measured(b.uid, b.shape))
            .collect();
        let mut state = |index: usize| -> Option<Vec<Body<K::Shape>>> {
            let bodies = self.oracle.state(kernel, index).ok()?;
            Some(
                bodies
                    .iter()
                    .map(|(b, sig)| (None, *sig, b.shape.clone()))
                    .collect(),
            )
        };
        let reference = match states.before {
            Some(s) => state(s)?,
            None => Vec::new(),
        };
        let target = state(states.after)?;
        let measured = measure(kernel, &before, &after, &reference, &target);
        let (ours, theirs) = match measured {
            Ok(m) => m,
            Err(e) => {
                trace!("{name}: no geometric check: {e}");
                return None;
            }
        };
        let verdict = judge(&ours, &theirs, volume);
        if tracing() {
            // The new faces' area and the faces off (area:distance), the
            // largest first.
            let list = |faces: &[Deviation]| {
                let mut faces = faces.to_vec();
                faces.sort_by(|a, b| b.area.total_cmp(&a.area));
                let area: f64 = faces
                    .iter()
                    .filter(|f| f.largest.is_some())
                    .map(|f| f.area)
                    .sum();
                let off: Vec<String> = faces
                    .iter()
                    .filter(|f| f.off())
                    .map(|f| format!("{:.4}:{:.1e}", f.area, f.largest.unwrap_or(f64::NAN)))
                    .collect();
                format!(
                    "{} faces, {area:.4} mm², off [{}], span {:.4} mm³",
                    faces.len(),
                    off.join(" "),
                    Side::of(&faces).span
                )
            };
            trace!(
                "{name}: geometric check in {:.2} s, the change {volume:.4} mm³ off: {verdict:?}; \
                 its new faces: {}; the file's: {}",
                clock.elapsed().as_secs_f64(),
                list(&ours),
                list(&theirs)
            );
        }
        verdict
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mitcad_model::NewFace;
    use mitcad_model::testing::MockShape;

    fn face(area: f64, largest: f64) -> Deviation {
        Deviation {
            area,
            largest: Some(largest),
        }
    }

    #[test]
    fn main_faces_decide() {
        // An approximation layer: every face within the tolerance.
        let ours = [face(10.0, 1e-4), face(4.0, 0.0)];
        let theirs = [face(10.0, 2e-4), face(4.0, 1e-5)];
        assert!(matches!(
            judge(&ours, &theirs, 0.0),
            Some(Verdict::Agrees(_))
        ));
        // Corner patches off, the rounding on the file's.
        let ours = [face(100.0, 0.001), face(2.0, 0.04), face(2.0, 0.05)];
        let theirs = [face(100.0, 0.002), face(1.5, 0.03), face(1.5, 0.03)];
        assert!(matches!(
            judge(&ours, &theirs, 0.0),
            Some(Verdict::Agrees(_))
        ));
        // A wrong rounding: its main face off.
        let ours = [face(100.0, 0.15), face(2.0, 0.0)];
        let theirs = [face(100.0, 0.15), face(2.0, 0.0)];
        match judge(&ours, &theirs, 0.0) {
            Some(Verdict::Differs(why)) => assert!(why.contains("0.150 mm"), "{why}"),
            v => panic!("{v:?}"),
        }
        // A rounding the result lacks: only the file's faces show it.
        let ours = [face(100.0, 0.0)];
        let theirs = [face(100.0, 0.0), face(30.0, 0.4)];
        assert!(matches!(
            judge(&ours, &theirs, 0.0),
            Some(Verdict::Differs(_))
        ));
        // Many small faces off add up to a main part.
        let mut ours: Vec<Deviation> = (0..100).map(|_| face(1.0, 0.05)).collect();
        ours.push(face(1000.0, 0.0));
        assert!(matches!(judge(&ours, &[], 0.0), Some(Verdict::Differs(_))));
        // An extrusion's wall 0.1 mm off: 9 % of the new area in one face,
        // although together with the file's side the faces off hold less
        // than a tenth (mitcad#138's first traces accepted it).
        let ours = [
            face(425.6, 0.0),
            face(138.7, 0.0),
            face(138.7, 0.0),
            face(74.1, 0.1),
            face(20.7, 0.0),
            face(20.0, 0.0),
            face(7.8, 0.0),
            face(7.2, 0.0),
        ];
        let theirs = [face(425.6, 0.0), face(138.7, 0.0), face(74.1, 0.1)];
        match judge(&ours, &theirs, 0.0) {
            Some(Verdict::Differs(why)) => assert!(why.contains("0.100 mm"), "{why}"),
            v => panic!("{v:?}"),
        }
        // Every point measured on the faces, but the bodies 1.4 mm³ apart:
        // a part of a face the points missed (a face of 40 mm² 0.001 mm
        // off holds 0.04 mm³, twice that may pass).
        let ours = [face(60.0, 0.0), face(40.0, 0.0)];
        let theirs = [face(60.0, 0.0), face(40.0, 0.001)];
        assert!(matches!(
            judge(&ours, &theirs, 0.05),
            Some(Verdict::Agrees(_))
        ));
        assert!(matches!(
            judge(&ours, &theirs, 0.15),
            Some(Verdict::Differs(_))
        ));
        match judge(&ours, &theirs, 1.4) {
            Some(Verdict::Differs(why)) => assert!(why.contains("1.4000 mm³"), "{why}"),
            v => panic!("{v:?}"),
        }
        // A corner patch small enough but too far off: another corner (a
        // rounding whose file corner lay 2.5 mm off), and the cap's edge.
        let ours = [face(400.0, 0.0), face(2.0, 0.0)];
        let theirs = [face(400.0, 0.0), face(2.4, 2.5)];
        match judge(&ours, &theirs, 0.0) {
            Some(Verdict::Differs(why)) => assert!(why.contains("2.500 mm"), "{why}"),
            v => panic!("{v:?}"),
        }
        let theirs = [face(400.0, 0.0), face(2.4, 0.25)];
        assert!(matches!(
            judge(&ours, &theirs, 0.0),
            Some(Verdict::Agrees(_))
        ));
        // A face that could not be measured counts as off.
        let ours = [face(10.0, f64::INFINITY)];
        assert!(matches!(judge(&ours, &[], 0.0), Some(Verdict::Differs(_))));
        // Faces too thin for points do not count; nothing to measure.
        let thin = Deviation {
            area: 1.0,
            largest: None,
        };
        assert_eq!(judge(&[thin], &[], 0.0), None);
        assert_eq!(judge(&[], &[], 0.0), None);
    }

    /// A kernel whose shapes' faces are given by name in their history
    /// (`a,b:2,c`, with the area after a colon, else 10 mm²): a face is new
    /// where `before` has no face of that name. Its points lie on the faces
    /// of the same name; on a face whose name differs only by a leading
    /// `x` (the same face built otherwise) they lie a hundredth of a
    /// millimetre off for each letter of the name; else 1 mm off.
    struct Faces;

    fn stem(name: &str) -> &str {
        name.split(':').next().unwrap_or("").trim_start_matches('x')
    }

    fn point(name: &str) -> [f64; 3] {
        let n = stem(name)
            .bytes()
            .fold(0u64, |h, b| h.wrapping_mul(31) + u64::from(b));
        let other = if name.starts_with('x') { 1.0 } else { 0.0 };
        [(n % 1_000_000) as f64, other, 0.0]
    }

    fn names(shape: &MockShape) -> Vec<&str> {
        shape.history.split(',').filter(|s| !s.is_empty()).collect()
    }

    fn area(name: &str) -> f64 {
        name.split(':')
            .nth(1)
            .and_then(|a| a.parse().ok())
            .unwrap_or(10.0)
    }

    fn shape(faces: &str) -> MockShape {
        MockShape {
            history: faces.to_owned(),
            faces: Vec::new(),
            edges: Vec::new(),
            bounds: None,
            parts: Vec::new(),
        }
    }

    impl Kernel for Faces {
        type Shape = MockShape;

        fn extrude(&self, _: &mitcad_model::ExtrudeSpec<'_>) -> Result<MockShape, KernelError> {
            Err(KernelError::Unsupported("extrude"))
        }

        fn solids(&self, shape: &MockShape) -> Result<Vec<MockShape>, KernelError> {
            Ok(vec![shape.clone()])
        }

        fn boolean(
            &self,
            _: mitcad_model::BooleanOp,
            _: &[&MockShape],
            _: &MockShape,
        ) -> Result<mitcad_model::BooleanOutput<MockShape>, KernelError> {
            Err(KernelError::Unsupported("boolean"))
        }

        fn fillet(
            &self,
            _: mitcad_model::FeatureUid,
            _: &MockShape,
            _: &[mitcad_model::FilletSet<'_>],
            _: bool,
        ) -> Result<MockShape, KernelError> {
            Err(KernelError::Unsupported("fillet"))
        }

        fn count_edges(&self, _: &MockShape, _: &mitcad_model::EdgeName) -> usize {
            0
        }

        fn new_faces(
            &self,
            before: &[&MockShape],
            after: &MockShape,
            count: usize,
        ) -> Result<Vec<NewFace>, KernelError> {
            Ok(names(after)
                .into_iter()
                .filter(|n| !before.iter().any(|b| names(b).contains(n)))
                .map(|n| NewFace {
                    area: area(n),
                    points: vec![point(n); count.min(3)],
                })
                .collect())
        }

        fn boundary_distances(
            &self,
            shape: &MockShape,
            points: &[[f64; 3]],
        ) -> Result<Vec<f64>, KernelError> {
            Ok(points
                .iter()
                .map(|p| {
                    let faces = names(shape);
                    if faces.iter().any(|n| point(n) == *p) {
                        0.0
                    } else if let Some(n) = faces.iter().find(|n| point(n)[0] == p[0]) {
                        stem(n).len() as f64 / 100.0
                    } else {
                        1.0
                    }
                })
                .collect())
        }
    }

    fn uid(index: u32) -> BodyUid {
        BodyUid {
            feature: mitcad_model::FeatureUid(1),
            index,
        }
    }

    fn body(index: Option<u32>, volume: f64, faces: &str) -> Body<MockShape> {
        let shape = shape(faces);
        (
            index.map(uid),
            Sig {
                volume,
                area: 1.0,
                center: [0.0; 3],
                faces: names(&shape).len(),
            },
            shape,
        )
    }

    #[test]
    fn measures_the_new_faces_of_both_sides() {
        // The replay rounds the block's edge; the file's state has the same
        // rounding, with a corner of its own.
        let before = [body(Some(1), 100.0, "top:50,side:50,end")];
        let after = [body(
            Some(1),
            99.0,
            "top1:45,side1:45,end,round:20,corner:1",
        )];
        let reference = [body(None, 100.0, "top:50,side:50,end")];
        let target = [body(None, 99.01, "top1:45,side1:45,end,round:20,xcorner:1")];
        let (ours, theirs) =
            measure(&Faces, &before, &after, &reference, &target).expect("measured");
        // The new faces: top1, side1 and round on the other side's; the
        // corners 0.06 mm off each other.
        for side in [&ours, &theirs] {
            assert_eq!(side.len(), 4);
            let off: Vec<_> = side.iter().filter(|d| d.off()).collect();
            assert_eq!(off.len(), 1);
            assert!((off[0].largest.unwrap() - 0.06).abs() < 1e-12);
        }
        // The corner is small next to the rest: the result agrees.
        match judge(&ours, &theirs, 0.0) {
            Some(Verdict::Agrees(why)) => assert!(why.contains("corner"), "{why}"),
            v => panic!("{v:?}"),
        }

        // A body the item leaves as it is has no new faces.
        let before = [body(Some(1), 100.0, "a,b"), body(Some(2), 50.0, "c,d")];
        let after = [
            body(Some(1), 100.0, "a,b"),
            body(Some(2), 49.0, "c,xbigface"),
        ];
        let reference = [body(None, 100.0, "a,b"), body(None, 50.0, "c,d")];
        let target = [body(None, 100.0, "a,b"), body(None, 49.2, "c,bigface")];
        let (ours, theirs) =
            measure(&Faces, &before, &after, &reference, &target).expect("measured");
        assert_eq!(ours.len(), 1);
        // (Its main face off settles it: the file's are not measured.)
        assert!(theirs.is_empty());
        match judge(&ours, &theirs, 0.0) {
            Some(Verdict::Differs(why)) => assert!(why.contains("0.070 mm"), "{why}"),
            v => panic!("{v:?}"),
        }
    }

    #[test]
    fn measuring_stops_at_a_main_face_off() {
        // The largest face is measured first; off, it settles the result,
        // and the faces after it are not measured (they count as on).
        let before = [body(Some(1), 100.0, "a")];
        let after = [body(Some(1), 99.0, "xbig:100,small1:1,small2:1")];
        let reference = [body(None, 100.0, "a")];
        let target = [body(None, 99.5, "big:100,xsmall1:1,xsmall2:1")];
        let (ours, theirs) =
            measure(&Faces, &before, &after, &reference, &target).expect("measured");
        assert!(theirs.is_empty());
        assert_eq!(ours.len(), 3);
        assert_eq!(ours[0].area, 100.0);
        assert!(ours[0].off());
        assert!(ours[1..].iter().all(|d| d.largest == Some(0.0)));
    }

    #[test]
    fn changed_bodies_pair_each_once() {
        let a = [body(Some(1), 10.0, "a"), body(Some(2), 10.0, "a")];
        let b = [body(Some(1), 10.0, "a")];
        let c = changed(&a, &b);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].0, Some(uid(2)));
        // The file's bodies have no uids: the same solid is enough.
        let a = [body(None, 10.0, "a"), body(None, 20.0, "b")];
        let b = [body(None, 20.0, "b")];
        assert_eq!(changed(&a, &b).len(), 1);
    }
}

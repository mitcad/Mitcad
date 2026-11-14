// SPDX-License-Identifier: MIT
//! Entries: how the tool goes down into the material at the start of a
//! cut (docs/cam.md, "Entries, tabs and leads"). A straight plunge needs
//! a center-cutting end mill; a ramp goes down along the cut back and
//! forth at a shallow angle, a helix goes down round a circle beside the
//! cut's start, so that the end mill cuts with its side. Where the helix
//! does not fit the ramp is taken, and where the ramp has no room either
//! (a cut shorter than the tool's radius) the plunge.

use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};

use super::path::Path;
use crate::CamError;
use crate::toolpath::PathBuilder;

/// How the tool goes down into the material.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    /// Straight down at the plunge feed.
    #[default]
    Plunge,
    /// Down along the cut, back and forth, at `angle` degrees from the
    /// horizontal.
    Ramp { angle: f64 },
    /// Down round a helix of the tool center's `radius` (mm) at `angle`
    /// degrees, ending at the start of the cut; then once round at the
    /// depth.
    Helix { radius: f64, angle: f64 },
}

fn angle_ok(a: f64) -> bool {
    a.is_finite() && a > 0.0 && a < 90.0
}

impl Entry {
    pub fn validate(&self) -> Result<(), CamError> {
        match *self {
            Entry::Plunge => Ok(()),
            Entry::Ramp { angle } => {
                if angle_ok(angle) {
                    Ok(())
                } else {
                    Err(CamError::new(
                        "The ramp angle must be more than 0 and less than 90 degrees.",
                    ))
                }
            }
            Entry::Helix { radius, angle } => {
                if !(radius.is_finite() && radius > 0.0) {
                    return Err(CamError::new("The helix radius must be more than zero."));
                }
                if !angle_ok(angle) {
                    return Err(CamError::new(
                        "The helix angle must be more than 0 and less than 90 degrees.",
                    ));
                }
                Ok(())
            }
        }
    }
}

/// The entry an operation made in the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryUsed {
    Plunge,
    Ramp,
    Helix,
}

/// Where an entry may go.
pub(crate) struct EntrySpace<'a> {
    /// Whether the tool center may run round the circle (center, radius)
    /// without cutting what must stay.
    pub helix_fits: &'a dyn Fn([f64; 2], f64) -> bool,
    /// The shortest stretch of the cut a ramp may run along (mm).
    pub min_ramp: f64,
}

/// Takes the tool from where it is, above the start of `path` and at or
/// above `top` (the material's top there), down to `z` at the start of
/// `path`, at `feed`. A plunge is one straight move; a ramp or a helix
/// first comes down to `top` in a straight line.
pub(crate) fn enter(
    b: &mut PathBuilder,
    entry: &Entry,
    path: &Path,
    top: f64,
    z: f64,
    feed: f64,
    space: &EntrySpace,
) -> EntryUsed {
    let here = b.at().expect("the tool is placed");
    let start = path.start().unwrap_or([here[0], here[1]]);
    let depth = top - z;
    if depth <= 1e-9 {
        b.line([start[0], start[1], z], feed);
        return EntryUsed::Plunge;
    }
    let down_to_top = |b: &mut PathBuilder| {
        if here[2] > top + 1e-9 {
            b.line([start[0], start[1], top], feed);
        }
    };
    let ramp_angle = match *entry {
        Entry::Plunge => None,
        Entry::Ramp { angle } => Some(angle),
        Entry::Helix { radius, angle } => {
            if let Some((center, ccw)) = helix_center(path, start, radius, space) {
                Helix {
                    center,
                    radius,
                    angle,
                    ccw,
                }
                .cut(b, start, top, z, feed);
                return EntryUsed::Helix;
            }
            Some(angle)
        }
    };
    if let Some(angle) = ramp_angle {
        let total = path.length();
        let horizontal = depth / angle.to_radians().tan();
        let leg = total.min(horizontal / 2.0);
        if leg >= space.min_ramp && leg > 1e-6 {
            down_to_top(b);
            let legs = ((horizontal / (2.0 * leg)) - 1e-9).ceil().max(1.0) as usize;
            let forward = path.slice(0.0, leg);
            let back = forward.reversed();
            for i in 0..legs {
                let z0 = top - depth * (2 * i) as f64 / (2 * legs) as f64;
                let z1 = top - depth * (2 * i + 1) as f64 / (2 * legs) as f64;
                let z2 = if i + 1 == legs {
                    z
                } else {
                    top - depth * (2 * i + 2) as f64 / (2 * legs) as f64
                };
                forward.cut(b, z0, z1, feed);
                back.cut(b, z1, z2, feed);
            }
            return EntryUsed::Ramp;
        }
    }
    b.line([start[0], start[1], z], feed);
    EntryUsed::Plunge
}

/// A helix of the tool center round `center`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Helix {
    pub center: [f64; 2],
    pub radius: f64,
    /// Degrees from the horizontal.
    pub angle: f64,
    pub ccw: bool,
}

impl Helix {
    /// From where the tool is, above `start` (a point of the circle):
    /// straight down to `top`, round and down to `z` in whole turns no
    /// steeper than the angle, then once round at `z` (the turns leave a
    /// slope on the floor).
    pub(crate) fn cut(&self, b: &mut PathBuilder, start: [f64; 2], top: f64, z: f64, feed: f64) {
        let here = b.at().expect("the tool is placed");
        if here[2] > top + 1e-9 {
            b.line([start[0], start[1], top], feed);
        }
        let depth = (top - z).max(0.0);
        let pitch = TAU * self.radius * self.angle.to_radians().tan();
        let turns = ((depth / pitch) - 1e-9).ceil().max(1.0) as usize;
        for i in 1..=turns {
            let zi = if i == turns {
                z
            } else {
                top - depth * i as f64 / turns as f64
            };
            b.arc([start[0], start[1], zi], self.center, self.ccw, feed);
        }
        b.arc([start[0], start[1], z], self.center, self.ccw, feed);
    }
}

/// The center of a helix through `start`, tangent to the path there, on
/// whichever side it fits; with whether it runs counter-clockwise.
fn helix_center(
    path: &Path,
    start: [f64; 2],
    radius: f64,
    space: &EntrySpace,
) -> Option<([f64; 2], bool)> {
    let t = path.tangent_at(0.0);
    let left = [-t[1], t[0]];
    for (n, ccw) in [(left, true), ([-left[0], -left[1]], false)] {
        let c = [start[0] + radius * n[0], start[1] + radius * n[1]];
        if (space.helix_fits)(c, radius) {
            return Some((c, ccw));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Contour;
    use crate::toolpath::{Move, Toolpath};

    fn run(entry: Entry, path: &Path, fits: bool) -> (EntryUsed, Toolpath) {
        let mut b = PathBuilder::new();
        let s = path.start().unwrap();
        b.rapid([s[0], s[1], 2.0]);
        let f = move |_: [f64; 2], _: f64| fits;
        let space = EntrySpace {
            helix_fits: &f,
            min_ramp: 3.0,
        };
        let used = enter(&mut b, &entry, path, 0.0, -3.0, 300.0, &space);
        (used, b.finish())
    }

    #[test]
    fn a_ramp_goes_back_and_forth_along_the_cut_and_ends_at_its_start() {
        let path = Path::of(&Contour::rectangle([0.0, 0.0], [40.0, 20.0]));
        let (used, p) = run(Entry::Ramp { angle: 5.0 }, &path, false);
        assert_eq!(used, EntryUsed::Ramp);
        let end = p.moves.last().unwrap().to();
        assert!((end[0]).abs() < 1e-9 && end[1].abs() < 1e-9 && (end[2] + 3.0).abs() < 1e-12);
        // Never steeper than the angle: Z falls at most tan(5 deg) per mm.
        let mut at = p.moves[1].to();
        assert_eq!(at, [0.0, 0.0, 0.0]);
        for m in &p.moves[2..] {
            let to = m.to();
            let run = (to[0] - at[0]).hypot(to[1] - at[1]);
            assert!(at[2] - to[2] <= run * 5f64.to_radians().tan() + 1e-9);
            at = to;
        }
    }

    #[test]
    fn a_helix_turns_down_to_the_depth_and_once_round_there() {
        let path = Path::of(&Contour::rectangle([0.0, 0.0], [40.0, 20.0]));
        let (used, p) = run(
            Entry::Helix {
                radius: 2.0,
                angle: 3.0,
            },
            &path,
            true,
        );
        assert_eq!(used, EntryUsed::Helix);
        let arcs: Vec<&Move> = p
            .moves
            .iter()
            .filter(|m| matches!(m, Move::Arc { .. }))
            .collect();
        // 3 mm at 2*pi*2*tan(3 deg) = 0.659 mm a turn: five turns and the
        // flat one.
        assert_eq!(arcs.len(), 6);
        for m in &arcs {
            let Move::Arc { to, center, .. } = **m else {
                unreachable!()
            };
            assert_eq!([to[0], to[1]], [0.0, 0.0]);
            assert!((center[0].hypot(center[1]) - 2.0).abs() < 1e-12);
        }
        assert_eq!(arcs[5].to()[2], -3.0);
        assert_eq!(arcs[4].to()[2], -3.0);
    }

    #[test]
    fn a_helix_that_does_not_fit_ramps_and_a_short_cut_plunges() {
        let path = Path::of(&Contour::rectangle([0.0, 0.0], [40.0, 20.0]));
        let helix = Entry::Helix {
            radius: 2.0,
            angle: 3.0,
        };
        assert_eq!(run(helix, &path, false).0, EntryUsed::Ramp);
        let short = Path::polyline(&[[0.0, 0.0], [1.0, 0.0]]);
        let (used, p) = run(helix, &short, false);
        assert_eq!(used, EntryUsed::Plunge);
        assert_eq!(
            p.moves.last(),
            Some(&Move::Line {
                to: [0.0, 0.0, -3.0],
                feed: 300.0
            })
        );
    }

    #[test]
    fn bad_angles_are_refused() {
        assert!(Entry::Ramp { angle: 0.0 }.validate().is_err());
        assert!(
            Entry::Helix {
                radius: 0.0,
                angle: 2.0
            }
            .validate()
            .is_err()
        );
        assert!(Entry::Ramp { angle: 3.0 }.validate().is_ok());
    }
}

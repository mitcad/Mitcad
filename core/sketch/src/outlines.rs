// SPDX-License-Identifier: MIT
//! The outline set of a sketch (mitcad#2194): the areas Extrude and
//! Revolve start with and a whole sketch picked for their Profiles gives,
//! and the areas Show profiles fills.
//!
//! When no two of the sketch's own curves cross through each other on the
//! boundary of its areas ([`Layout::crossings`]), it is every region the
//! sketch's own curves bound at an even nesting depth, the depth being how
//! many holes of the other such regions it lies in: a plate with holes is
//! the ring, an island inside a hole is added, separate outlines are each
//! one, and adjacent regions (a rectangle split by a line) are all, the
//! whole rectangle. Regions bounded in part by the supporting face's edges
//! (the face's leftover region about the drawn shapes, mitcad#1226) are
//! not part of it, and their holes do not count. When curves cross so (two
//! overlapping circles), the sketch has none: which of the areas are meant
//! is not clear, so they are picked. On a face sketch only the own curves
//! cross: the face's edges and the outline projected when the sketch was
//! made never do (mitcad#2233, [`super::regions::own_area_regions`]).

use super::geometry::{P2, polygon_contains};
use super::regions::{Layout, Region};
use super::svg::interior_point;

/// A sketch's outline set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutlineSet {
    /// The indices of its regions in the sketch's regions, ascending.
    pub regions: Vec<usize>,
    /// Two of the sketch's closed curves cross through each other: no
    /// outline set (`regions` empty).
    pub crossing: bool,
}

/// The outline set of `regions` lying as `layout` says.
pub fn outline_set(regions: &[Region], layout: &Layout) -> OutlineSet {
    if !layout.crossings().is_empty() {
        return OutlineSet {
            regions: Vec::new(),
            crossing: true,
        };
    }
    let own: Vec<usize> = (0..regions.len())
        .filter(|&i| !regions[i].profile.face_bounded())
        .collect();
    // The holes of the regions the sketch's own curves bound, each with its
    // region and its bounds.
    let holes: Vec<(usize, &[P2], P2, P2)> = own
        .iter()
        .flat_map(|&i| {
            regions[i].holes.iter().map(move |ring| {
                let (lo, hi) = bounds(ring);
                (i, ring.as_slice(), lo, hi)
            })
        })
        .collect();
    let members = own
        .into_iter()
        .filter(|&i| {
            let region = &regions[i];
            let Some(p) = interior_point(&region.outline, &region.holes, region.centroid) else {
                return false; // nothing inside it to tell its depth by
            };
            let depth = holes
                .iter()
                .filter(|&&(owner, ring, lo, hi)| {
                    owner != i
                        && p[0] >= lo[0]
                        && p[0] <= hi[0]
                        && p[1] >= lo[1]
                        && p[1] <= hi[1]
                        && polygon_contains(ring, p)
                })
                .count();
            depth % 2 == 0
        })
        .collect();
    OutlineSet {
        regions: members,
        crossing: false,
    }
}

/// The bounds (minimum, maximum) of a ring of points.
fn bounds(ring: &[P2]) -> (P2, P2) {
    ring.iter().fold(
        ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
        |(lo, hi), p| {
            (
                [lo[0].min(p[0]), lo[1].min(p[1])],
                [hi[0].max(p[0]), hi[1].max(p[1])],
            )
        },
    )
}

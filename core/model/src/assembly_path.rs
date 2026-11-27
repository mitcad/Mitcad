// SPDX-License-Identifier: MIT
//! Assembly paths (mitcad#1737): a body slid along a direction or turned
//! about an axis towards its place, and where on that path it first runs
//! into other bodies. The `assembly_path` query (`model-api/src/
//! assembly_path.rs`) measures the exact shapes with the kernel at
//! positions along the path; the search here decides which positions to
//! measure, so it is tested without a kernel.
//!
//! A position on the path is a number `t` from 0 to the path's length: mm
//! for a slide, radians for a turn. At each position the caller gives a
//! [`Probe`]: the clearance (the smallest distance from the moving body to
//! the others, mm) and whether the body overlaps one of them (a contact
//! with volume; bodies that only touch do not overlap).
//!
//! - **Stepping.** The path is cut into `steps` equal steps and the body is
//!   measured at each. Between two positions the clearance changes at most
//!   by `speed` times the distance along the path (`speed` is 1 for a
//!   slide; for a turn, the farthest any point of the body lies from the
//!   axis), so when the clearances at both ends add up to at least that,
//!   no contact can lie between them. Otherwise the span is halved, at
//!   most [`MOST_HALVINGS`] times per step; a span still not ruled out is
//!   counted in [`PathResult::certified`] (a wall thinner than the span
//!   could be passed over between the samples).
//! - **Contacts (mitcad#2577).** The bodies the moving body touches before
//!   it moves without overlapping them (a bracket flush against its
//!   neighbours, a part resting on a face) are its contacts, measured apart
//!   from the others ([`Probe::contact`]). Their clearance is 0 wherever the
//!   body still touches them, so no distance can rule a span out for them:
//!   a span where it touches its contacts at both ends is not halved for
//!   them (the overlap measured at each position is what finds it running
//!   into one) and counts as not ruled out in [`PathResult::certified`],
//!   as a span halved to its limit does. The others are ruled out and
//!   halved as above.
//! - **Limits (mitcad#2577).** [`PathSettings::most_probes`] and
//!   [`PathSettings::time_limit`] stop the search before the next
//!   position past them; the result is then partial
//!   ([`PathResult::stopped`]): what was measured up to there, never
//!   certified.
//! - **The first contact.** Between the last position clear of the others
//!   and the first that overlaps, the contact is found by bisection until
//!   the two lie within the tolerance of each other (`tolerance` mm of
//!   motion of the body's farthest point).
//! - **Deeper than the tolerance.** A contact is a hit only when the body
//!   still overlaps after moving the tolerance further: a graze shallower
//!   than that is passed (the path goes on after it), and a contact within
//!   the tolerance of the path's end is the body reaching its seat
//!   ([`PathResult::touches_at_end`]), not a hit.
//! - **Minimum clearance.** The smallest clearance measured, refined
//!   between the positions beside it by a golden-section search.

use std::time::{Duration, Instant};

use crate::datum::Vec3;
use crate::transform::Transform;
use mitcad_vec3::{dot, norm, scale, sub, unit};

/// The most a step is halved where the clearances cannot rule out a
/// contact: a step is measured at 2³ + 1 positions at most.
pub const MOST_HALVINGS: u32 = 3;

/// How the body moves along its path, in the design's coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    /// Along a unit direction by a distance (mm, > 0).
    Slide { direction: Vec3, distance: f64 },
    /// About the axis through `origin` along a unit direction by an angle
    /// (radians, right-hand rule, either sign).
    Turn {
        origin: Vec3,
        direction: Vec3,
        angle: f64,
    },
}

impl Motion {
    /// A slide; a negative distance slides against the direction.
    pub fn slide(direction: Vec3, distance: f64) -> Result<Self, String> {
        let direction = unit(direction).ok_or("the slide's direction has no length")?;
        if !distance.is_finite() || distance == 0.0 {
            return Err(format!(
                "the slide's distance must not be zero, got {distance}"
            ));
        }
        Ok(Self::Slide {
            direction: scale(direction, distance.signum()),
            distance: distance.abs(),
        })
    }

    pub fn turn(origin: Vec3, direction: Vec3, angle: f64) -> Result<Self, String> {
        let direction = unit(direction).ok_or("the turn's axis has no direction")?;
        if !angle.is_finite() || angle == 0.0 {
            return Err(format!("the turn's angle must not be zero, got {angle}"));
        }
        Ok(Self::Turn {
            origin,
            direction,
            angle,
        })
    }

    /// The path's length: mm for a slide, radians for a turn.
    pub fn length(&self) -> f64 {
        match self {
            Self::Slide { distance, .. } => *distance,
            Self::Turn { angle, .. } => angle.abs(),
        }
    }

    /// The motion `t` along the path (mm or radians from the start).
    pub fn at(&self, t: f64) -> Transform {
        match *self {
            Self::Slide { direction, .. } => Transform::translation(scale(direction, t)),
            Self::Turn {
                origin,
                direction,
                angle,
            } => Transform::rotation(origin, direction, t * angle.signum())
                .unwrap_or(Transform::IDENTITY),
        }
    }

    /// mm a point within `corners`' box (their convex hull) moves at most
    /// per unit of the path: 1 for a slide, for a turn the farthest corner's
    /// distance from the axis. None without corners for a turn.
    pub fn speed(&self, corners: impl IntoIterator<Item = Vec3>) -> Option<f64> {
        match *self {
            Self::Slide { .. } => Some(1.0),
            Self::Turn {
                origin, direction, ..
            } => corners
                .into_iter()
                .map(|corner| {
                    let from = sub(corner, origin);
                    norm(sub(from, scale(direction, dot(from, direction))))
                })
                .reduce(f64::max),
        }
    }
}

/// A clearance under this (mm) is a contact: the caller tests the overlap.
pub const TOUCHING: f64 = 1e-6;

/// What the caller measured at a position of the path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Probe {
    /// The smallest distance from the moving body to the others that are
    /// not its contacts, mm; 0 where they touch or overlap; infinite when
    /// every other body is a contact.
    pub clearance: f64,
    /// The moving body overlaps another (a contact with volume), a contact
    /// or not.
    pub overlap: bool,
    /// The smallest distance to its contacts (mitcad#2577): the bodies it
    /// touched without overlapping them before it moved, mm; infinite
    /// without contacts.
    pub contact: f64,
}

impl Probe {
    /// A position measured without contacts.
    pub fn new(clearance: f64, overlap: bool) -> Self {
        Self {
            clearance,
            overlap,
            contact: f64::INFINITY,
        }
    }

    /// The smallest distance to any other body, contacts included: the
    /// clearance the result reports.
    pub fn nearest(&self) -> f64 {
        self.clearance.min(self.contact)
    }
}

/// How the path is searched.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathSettings {
    /// The path's length: mm for a slide, radians for a turn (> 0).
    pub length: f64,
    /// Equal steps the path is measured at (≥ 1).
    pub steps: usize,
    /// mm of motion of the body's farthest point: the accuracy of the
    /// contact and how deep a contact must be to count (> 0).
    pub tolerance: f64,
    /// mm a point of the body moves at most per unit of the path: 1 for a
    /// slide, the farthest distance of the body from the axis for a turn
    /// (> 0).
    pub speed: f64,
    /// The most positions measured (≥ 1; None: no limit). The search stops
    /// before measuring one more and its result is partial
    /// ([`PathResult::stopped`]).
    pub most_probes: Option<usize>,
    /// How long the search may measure (None: no limit): past it, it stops
    /// before measuring the next position, as at `most_probes`. The start
    /// is always measured.
    pub time_limit: Option<Duration>,
}

impl PathSettings {
    /// The tolerance in path units.
    pub fn resolution(&self) -> f64 {
        self.tolerance / self.speed
    }

    /// The position of step `k`.
    pub fn step_at(&self, k: usize) -> f64 {
        if k >= self.steps {
            self.length
        } else {
            self.length * k as f64 / self.steps as f64
        }
    }

    /// Why the settings cannot be searched, if they cannot.
    pub fn check(&self) -> Result<(), String> {
        let positive = |value: f64| value.is_finite() && value > 0.0;
        if !positive(self.length) {
            return Err(format!(
                "the path must be longer than zero, got {}",
                self.length
            ));
        }
        if self.steps == 0 {
            return Err("steps must be at least 1".to_owned());
        }
        if !positive(self.tolerance) {
            return Err(format!(
                "the tolerance must be greater than zero, got {}",
                self.tolerance
            ));
        }
        if !positive(self.speed) {
            return Err("the body has no extent away from the axis".to_owned());
        }
        if self.most_probes == Some(0) {
            return Err("the most positions to measure must be at least 1".to_owned());
        }
        Ok(())
    }
}

/// A position measured clear of the others.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// The position on the path.
    pub at: f64,
    /// The clearance there, mm.
    pub clearance: f64,
}

/// Where the body first runs into another, deeper than the tolerance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// The furthest position found clear before the contact: the body can
    /// move this far. Within [`PathSettings::resolution`] of the contact.
    pub clear: f64,
    /// The first position found overlapping.
    pub contact: f64,
    /// A position the tolerance past the contact, still overlapping.
    pub deeper: f64,
    /// The first step at or past `deeper` (0 when the body overlaps at the
    /// start).
    pub step: usize,
    /// The body overlaps another before it moves.
    pub at_start: bool,
}

/// What the search found.
#[derive(Debug, Clone, PartialEq)]
pub struct PathResult {
    pub hit: Option<Hit>,
    /// The clearance at each step from the start up to the hit (or the
    /// path's end); 0 at a step inside a graze.
    pub steps: Vec<Sample>,
    /// The smallest clearance before the hit (0 with a hit or a contact
    /// at the end); None when the body overlaps at the start.
    pub min_clearance: Option<Sample>,
    /// The body touches another within the tolerance of the path's end
    /// (its seat), which is no hit.
    pub touches_at_end: bool,
    /// Every span between the positions measured was ruled out of
    /// contact by the clearances (or is shorter than the resolution);
    /// false for a partial result.
    pub certified: bool,
    /// How many positions were measured.
    pub probes: usize,
    /// The search stopped at a limit before it was done (mitcad#2577):
    /// `hit` and `touches_at_end` are then of the part checked only (none),
    /// `steps` end where it got to and `min_clearance` is the smallest
    /// measured up to there, not refined.
    pub stopped: Option<Stopped>,
}

/// Which limit stopped a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// [`PathSettings::most_probes`].
    Probes,
    /// [`PathSettings::time_limit`].
    Time,
}

/// Where a search stopped at a limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stopped {
    pub limit: Limit,
    /// The path is checked from the start up to this position: the body
    /// was found clear there (or past a graze), every span before it
    /// walked. 0 when only the start was measured.
    pub checked_to: f64,
}

/// A measured position.
#[derive(Debug, Clone, Copy)]
struct Point {
    at: f64,
    probe: Probe,
}

/// The walk over one span.
enum Walk {
    /// Clear up to the span's end, measured there.
    Clear(Point),
    /// Clear at `clear`, overlapping at `inside`.
    Contact { clear: Point, inside: Point },
}

/// Why the search ends before its answer.
enum Halt<E> {
    /// The caller's probe failed.
    Failed(E),
    /// A limit of the settings was reached (mitcad#2577).
    Limit(Limit),
}

impl<E> From<E> for Halt<E> {
    fn from(error: E) -> Self {
        Self::Failed(error)
    }
}

struct Search<'a, E> {
    settings: &'a PathSettings,
    probe: &'a mut dyn FnMut(f64) -> Result<Probe, E>,
    probes: usize,
    certified: bool,
    /// Every clear position measured, for the minimum.
    clear: Vec<Sample>,
    /// The clearance at each step walked so far.
    steps: Vec<Sample>,
    /// The path is checked from the start up to here.
    reached: f64,
    started: Instant,
}

impl<E> Search<'_, E> {
    fn measure(&mut self, at: f64) -> Result<Point, Halt<E>> {
        // The limits (mitcad#2577): never before the start is measured.
        if self.probes > 0 {
            if self
                .settings
                .most_probes
                .is_some_and(|most| self.probes >= most)
            {
                return Err(Halt::Limit(Limit::Probes));
            }
            if self
                .settings
                .time_limit
                .is_some_and(|limit| self.started.elapsed() >= limit)
            {
                return Err(Halt::Limit(Limit::Time));
            }
        }
        let at = at.clamp(0.0, self.settings.length);
        let mut probe = (self.probe)(at)?;
        self.probes += 1;
        if probe.overlap {
            probe.clearance = 0.0;
            probe.contact = 0.0;
        } else {
            self.clear.push(Sample {
                at,
                clearance: probe.nearest(),
            });
        }
        Ok(Point { at, probe })
    }

    /// From a clear position to `to`: clear there, or where the contact
    /// lies between.
    fn walk(&mut self, from: Point, to: f64) -> Result<Walk, Halt<E>> {
        let end = self.measure(to)?;
        self.span(from, end, 0)
    }

    fn span(&mut self, a: Point, b: Point, halvings: u32) -> Result<Walk, Halt<E>> {
        if b.probe.overlap {
            return Ok(Walk::Contact {
                clear: a,
                inside: b,
            });
        }
        let gap = b.at - a.at;
        let reach = self.settings.speed * gap;
        // The others, and the contacts (mitcad#2577), each ruled out by
        // their own clearances (both infinite without contacts).
        let others_apart = a.probe.clearance + b.probe.clearance >= reach;
        let contacts_apart = a.probe.contact + b.probe.contact >= reach;
        if others_apart && contacts_apart {
            return Ok(Walk::Clear(b));
        }
        // A contact shorter than the resolution is no hit.
        if gap <= self.settings.resolution() {
            return Ok(Walk::Clear(b));
        }
        // Touching its contacts at both ends (flush against them, sliding
        // along them): no distance rules the span out for them and halving
        // would not either; the overlap measured at each position finds
        // the body running into one. Not ruled out.
        let along_contacts = a.probe.contact < TOUCHING && b.probe.contact < TOUCHING;
        if others_apart && along_contacts {
            self.certified = false;
            return Ok(Walk::Clear(b));
        }
        if halvings >= MOST_HALVINGS {
            self.certified = false;
            return Ok(Walk::Clear(b));
        }
        let middle = self.measure(a.at + gap / 2.0)?;
        match self.span(a, middle, halvings + 1)? {
            Walk::Clear(_) => self.span(middle, b, halvings + 1),
            contact => Ok(contact),
        }
    }

    /// The contact between a clear and an overlapping position, to the
    /// resolution.
    fn bisect(&mut self, mut clear: Point, mut inside: Point) -> Result<(Point, Point), Halt<E>> {
        let resolution = self.settings.resolution();
        while inside.at - clear.at > resolution {
            let middle = self.measure((clear.at + inside.at) / 2.0)?;
            if middle.probe.overlap {
                inside = middle;
            } else {
                clear = middle;
            }
        }
        Ok((clear, inside))
    }

    /// The smallest clearance among the clear positions measured up to
    /// `until`, refined between its neighbours.
    fn minimum(&mut self, until: f64) -> Result<Option<Sample>, Halt<E>> {
        let found = self.measured(until);
        let Some((index, best)) = found
            .iter()
            .copied()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.clearance.total_cmp(&b.clearance))
        else {
            return Ok(None);
        };
        if best.clearance < TOUCHING {
            return Ok(Some(best));
        }
        let low = if index > 0 {
            found[index - 1].at
        } else {
            best.at
        };
        let high = found.get(index + 1).map_or(best.at, |s| s.at);
        self.golden(low, high, best)
    }

    /// The clear positions measured up to `until`, in their order along
    /// the path.
    fn measured(&self, until: f64) -> Vec<Sample> {
        let mut found: Vec<Sample> = self
            .clear
            .iter()
            .copied()
            .filter(|s| s.at <= until)
            .collect();
        found.sort_by(|a, b| a.at.total_cmp(&b.at));
        found
    }

    /// A golden-section search for the smallest clearance in
    /// `low..=high`, starting from `best`.
    fn golden(
        &mut self,
        mut low: f64,
        mut high: f64,
        mut best: Sample,
    ) -> Result<Option<Sample>, Halt<E>> {
        const RATIO: f64 = 0.618_033_988_749_894_9;
        let resolution = self.settings.resolution();
        if high - low <= resolution {
            return Ok(Some(best));
        }
        let mut x1 = high - RATIO * (high - low);
        let mut x2 = low + RATIO * (high - low);
        let mut f1 = self.clearance_at(x1, &mut best)?;
        let mut f2 = self.clearance_at(x2, &mut best)?;
        while high - low > resolution && best.clearance >= TOUCHING {
            if f1 <= f2 {
                high = x2;
                x2 = x1;
                f2 = f1;
                x1 = high - RATIO * (high - low);
                f1 = self.clearance_at(x1, &mut best)?;
            } else {
                low = x1;
                x1 = x2;
                f1 = f2;
                x2 = low + RATIO * (high - low);
                f2 = self.clearance_at(x2, &mut best)?;
            }
        }
        Ok(Some(best))
    }

    /// The clearance at a position; `best` follows the smallest.
    fn clearance_at(&mut self, at: f64, best: &mut Sample) -> Result<f64, Halt<E>> {
        let point = self.measure(at)?;
        let clearance = point.probe.nearest();
        if clearance < best.clearance {
            *best = Sample {
                at: point.at,
                clearance,
            };
        }
        Ok(clearance)
    }
}

/// Searches the path: `probe` measures the body at a position (mm or
/// radians from the start); its first call is the start, 0. Errors of
/// `probe` stop the search; a limit of the settings ends it with a
/// partial result ([`PathResult::stopped`]).
pub fn search<E>(
    settings: &PathSettings,
    mut probe: impl FnMut(f64) -> Result<Probe, E>,
) -> Result<PathResult, E> {
    let mut search = Search {
        settings,
        probe: &mut probe,
        probes: 0,
        certified: true,
        clear: Vec::new(),
        steps: Vec::new(),
        reached: 0.0,
        started: Instant::now(),
    };
    match run(&mut search) {
        Ok(found) => Ok(found),
        Err(Halt::Failed(error)) => Err(error),
        Err(Halt::Limit(limit)) => Ok(partial(search, limit)),
    }
}

/// What a search stopped at a limit found up to there (mitcad#2577).
fn partial<E>(search: Search<'_, E>, limit: Limit) -> PathResult {
    let checked_to = search.reached;
    let min_clearance = search
        .measured(checked_to)
        .into_iter()
        .min_by(|a, b| a.clearance.total_cmp(&b.clearance));
    PathResult {
        hit: None,
        steps: search.steps,
        min_clearance,
        touches_at_end: false,
        certified: false,
        probes: search.probes,
        stopped: Some(Stopped { limit, checked_to }),
    }
}

fn run<E>(search: &mut Search<'_, E>) -> Result<PathResult, Halt<E>> {
    let settings = search.settings;
    let start = search.measure(0.0)?;
    if start.probe.overlap {
        return Ok(PathResult {
            hit: Some(Hit {
                clear: 0.0,
                contact: 0.0,
                deeper: 0.0,
                step: 0,
                at_start: true,
            }),
            steps: Vec::new(),
            min_clearance: None,
            touches_at_end: false,
            certified: true,
            probes: search.probes,
            stopped: None,
        });
    }
    let length = settings.length;
    let resolution = settings.resolution();
    search.steps.push(Sample {
        at: 0.0,
        clearance: start.probe.nearest(),
    });
    let mut from = start;
    let mut hit = None;
    let mut touches_at_end = false;
    'steps: for k in 1..=settings.steps {
        let target = settings.step_at(k);
        while from.at < target {
            match search.walk(from, target)? {
                Walk::Clear(point) => from = point,
                Walk::Contact { clear, inside } => {
                    let (clear, inside) = search.bisect(clear, inside)?;
                    let deeper_at = inside.at + resolution;
                    if deeper_at > length {
                        // The body reaches another within the tolerance of
                        // its path's end (the contact lies before the
                        // target, so only steps that close to the end are
                        // left): its seat.
                        touches_at_end = true;
                        search.steps.push(Sample {
                            at: length,
                            clearance: 0.0,
                        });
                        search.reached = length;
                        break 'steps;
                    }
                    let deeper = search.measure(deeper_at)?;
                    if deeper.probe.overlap {
                        let step = (k..=settings.steps)
                            .find(|&j| settings.step_at(j) >= deeper.at - resolution * 1e-6)
                            .unwrap_or(settings.steps);
                        hit = Some(Hit {
                            clear: clear.at,
                            contact: inside.at,
                            deeper: deeper.at,
                            step,
                            at_start: false,
                        });
                        break 'steps;
                    }
                    // A graze: on from past it.
                    from = deeper;
                }
            }
            search.reached = from.at;
        }
        let clearance = if from.at == target {
            from.probe.nearest()
        } else {
            // The step lies inside a graze passed over.
            0.0
        };
        search.steps.push(Sample {
            at: target,
            clearance,
        });
    }
    let min_clearance = match hit {
        Some(hit) => Some(Sample {
            at: hit.clear,
            clearance: 0.0,
        }),
        None if touches_at_end => Some(Sample {
            at: length,
            clearance: 0.0,
        }),
        None => search.minimum(length)?,
    };
    Ok(PathResult {
        hit,
        steps: std::mem::take(&mut search.steps),
        min_clearance,
        touches_at_end,
        certified: search.certified,
        probes: search.probes,
        stopped: None,
    })
}

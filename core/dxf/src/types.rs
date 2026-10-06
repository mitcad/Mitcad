// SPDX-License-Identifier: MIT
use std::f64::consts::TAU;
use std::ops::{Add, Mul, Neg, Sub};

use crate::math::Affine;
use crate::units::Units;

/// A point or a vector in the drawing plane.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// z component of the cross product.
    pub fn cross(self, other: Self) -> f64 {
        self.x * other.y - self.y * other.x
    }

    /// The vector rotated 90 degrees counter-clockwise.
    pub fn perp(self) -> Self {
        Self::new(-self.y, self.x)
    }

    /// Angle of the vector from the x axis, in (-pi, pi].
    pub fn angle(self) -> f64 {
        self.y.atan2(self.x)
    }

    pub fn distance(self, other: Self) -> f64 {
        (other - self).length()
    }

    /// Unit vector at `angle` radians from the x axis.
    pub fn polar(angle: f64) -> Self {
        Self::new(angle.cos(), angle.sin())
    }
}

impl Add for Point2 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Point2 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

impl Mul<f64> for Point2 {
    type Output = Self;
    fn mul(self, factor: f64) -> Self {
        Self::new(self.x * factor, self.y * factor)
    }
}

impl Neg for Point2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

/// A NURBS curve, or a curve given only by points it passes through.
///
/// A spline has control points and knots (`control_points.len() + degree + 1`
/// knots), or only fit points when the file did not store the control
/// polygon; the sketch then interpolates the fit points.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Spline {
    pub degree: u32,
    pub control_points: Vec<Point2>,
    pub knots: Vec<f64>,
    /// One weight per control point; empty for a non-rational spline.
    pub weights: Vec<f64>,
    pub fit_points: Vec<Point2>,
    /// The curve closes on itself (DXF flag 1).
    pub closed: bool,
    /// Periodic knot vector (DXF flag 2).
    pub periodic: bool,
}

impl Spline {
    pub fn is_rational(&self) -> bool {
        self.weights.iter().any(|&w| w != 1.0)
    }

    /// Weight of control point `i` (1 when non-rational).
    pub fn weight(&self, i: usize) -> f64 {
        self.weights.get(i).copied().unwrap_or(1.0)
    }

    /// Parameter range of a spline with control points.
    pub fn domain(&self) -> Option<(f64, f64)> {
        let p = self.degree as usize;
        let n = self.control_points.len();
        if n == 0 || self.knots.len() != n + p + 1 {
            return None;
        }
        Some((self.knots[p], self.knots[n]))
    }

    /// Point at parameter `t` (de Boor's algorithm, rational when weighted).
    /// `None` when the spline has no valid control polygon.
    pub fn point_at(&self, t: f64) -> Option<Point2> {
        let p = self.degree as usize;
        let n = self.control_points.len();
        let (start, end) = self.domain()?;
        let t = t.clamp(start, end);
        // Knot span k with knots[k] <= t < knots[k + 1], within [p, n - 1].
        let mut k = p;
        while k + 1 < n && self.knots[k + 1] <= t {
            k += 1;
        }
        // Homogeneous coordinates (w x, w y, w).
        let mut d: Vec<[f64; 3]> = (0..=p)
            .map(|j| {
                let i = j + k - p;
                let w = self.weight(i);
                let c = self.control_points[i];
                [c.x * w, c.y * w, w]
            })
            .collect();
        for r in 1..=p {
            for j in (r..=p).rev() {
                let i = j + k - p;
                let denominator = self.knots[i + p + 1 - r] - self.knots[i];
                let alpha = if denominator == 0.0 {
                    0.0
                } else {
                    (t - self.knots[i]) / denominator
                };
                let previous = d[j - 1];
                for (value, before) in d[j].iter_mut().zip(previous) {
                    *value = (1.0 - alpha) * before + alpha * *value;
                }
            }
        }
        let [x, y, w] = d[p];
        if w == 0.0 {
            return None;
        }
        Some(Point2::new(x / w, y / w))
    }
}

/// Single-line (`TEXT`) or multi-line (`MTEXT`) text, as plain text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Text {
    /// Insertion point.
    pub position: Point2,
    pub height: f64,
    /// Direction of the baseline, radians from the x axis.
    pub rotation: f64,
    /// The text without formatting codes; lines separated by `\n`.
    pub content: String,
}

/// Neutral 2D geometry of one entity. Angles are radians, counter-clockwise.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    Point(Point2),
    Line {
        start: Point2,
        end: Point2,
    },
    /// Counter-clockwise from `start_angle` to `end_angle`, with
    /// `start_angle` in [0, 2pi) and `end_angle` in (`start_angle`,
    /// `start_angle` + 2pi].
    Arc {
        center: Point2,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
    },
    Circle {
        center: Point2,
        radius: f64,
    },
    /// Ellipse or elliptical arc: the point at parameter t is
    /// `center + major_axis * cos(t) + minor_axis * sin(t)`, where the minor
    /// axis is `major_axis.perp() * ratio`. The parameters run
    /// counter-clockwise like arc angles; a full ellipse runs from 0 to 2pi.
    Ellipse {
        center: Point2,
        /// From the centre to the end of the major axis.
        major_axis: Point2,
        /// Minor to major axis length, in (0, 1].
        ratio: f64,
        start_param: f64,
        end_param: f64,
    },
    Spline(Spline),
    Text(Text),
}

impl Geometry {
    /// An arc with normalised angles (see [`Geometry::Arc`]).
    pub fn arc(center: Point2, radius: f64, start_angle: f64, end_angle: f64) -> Self {
        let (start_angle, end_angle) = normalize_range(start_angle, end_angle);
        Self::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        }
    }

    /// Points whose bounding box is the geometry's (closely sampled for
    /// arcs, ellipses and splines; a text's insertion point and height).
    pub fn extent_points(&self) -> Vec<Point2> {
        const SAMPLES: usize = 64;
        let sample = |from: f64, to: f64, at: &dyn Fn(f64) -> Option<Point2>| -> Vec<Point2> {
            (0..=SAMPLES)
                .filter_map(|i| at(from + (to - from) * i as f64 / SAMPLES as f64))
                .collect()
        };
        match self {
            Self::Point(p) => vec![*p],
            Self::Line { start, end } => vec![*start, *end],
            Self::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => sample(*start_angle, *end_angle, &|a| {
                Some(*center + Point2::polar(a) * *radius)
            }),
            Self::Circle { center, radius } => vec![
                *center - Point2::new(*radius, *radius),
                *center + Point2::new(*radius, *radius),
            ],
            Self::Ellipse {
                center,
                major_axis,
                ratio,
                start_param,
                end_param,
            } => {
                let minor = major_axis.perp() * *ratio;
                sample(*start_param, *end_param, &|t| {
                    Some(*center + *major_axis * t.cos() + minor * t.sin())
                })
            }
            Self::Spline(spline) => match spline.domain() {
                Some((from, to)) => sample(from, to, &|t| spline.point_at(t)),
                None => spline.fit_points.clone(),
            },
            Self::Text(text) => vec![
                text.position,
                text.position + Point2::polar(text.rotation).perp() * text.height,
            ],
        }
    }

    /// True for an ellipse that is a whole closed curve.
    pub fn is_full_ellipse(&self) -> bool {
        matches!(self, Self::Ellipse { start_param, end_param, .. }
            if (end_param - start_param - TAU).abs() < 1e-12)
    }

    /// The geometry mapped by an affine transform: circles and arcs become
    /// ellipses under a non-uniform scale, and mirroring keeps arcs
    /// counter-clockwise.
    pub fn transformed(&self, transform: &Affine) -> Self {
        transform.apply_geometry(self)
    }
}

/// Normalises an angle range to start in [0, 2pi) and end in (start, start + 2pi].
pub(crate) fn normalize_range(start: f64, end: f64) -> (f64, f64) {
    let mut start_n = start.rem_euclid(TAU);
    if start_n > TAU - 1e-12 {
        // A start a rounding error below zero (or 2pi) is 0, not 2pi.
        start_n = 0.0;
    }
    let mut sweep = (end - start).rem_euclid(TAU);
    if sweep <= 1e-12 {
        // Equal angles mean a full turn, as in DXF arcs and ellipses.
        sweep = TAU;
    }
    (start_n, start_n + sweep)
}

/// A drawing entity with its layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub layer: String,
    pub geometry: Geometry,
}

impl Entity {
    pub fn new(layer: impl Into<String>, geometry: Geometry) -> Self {
        Self {
            layer: layer.into(),
            geometry,
        }
    }
}

/// A layer of the drawing (`LAYER` table entry).
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: String,
    /// DXF colour index 1-255 (7 is white/black).
    pub color: i16,
    /// False when the layer is off (negative colour in the file).
    pub visible: bool,
    pub frozen: bool,
    pub linetype: String,
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            color: 7,
            visible: true,
            frozen: false,
            linetype: "CONTINUOUS".into(),
        }
    }
}

/// The 2D content of a DXF file: model space entities with block references
/// expanded.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Drawing {
    /// `$ACADVER` of a read file (`AC1009` = R12, `AC1015` = R2000, ...).
    pub version: Option<String>,
    pub units: Units,
    pub layers: Vec<Layer>,
    pub entities: Vec<Entity>,
    /// Things the reader skipped or approximated, for an import report.
    pub warnings: Vec<String>,
}

impl Drawing {
    pub fn new(units: Units) -> Self {
        Self {
            units,
            ..Self::default()
        }
    }

    pub fn push(&mut self, layer: impl Into<String>, geometry: Geometry) {
        self.entities.push(Entity::new(layer, geometry));
    }

    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|layer| layer.name == name)
    }

    /// The lower left and upper right corners of the entities (in drawing
    /// units); `None` without entities.
    pub fn bounds(&self) -> Option<(Point2, Point2)> {
        let mut points = self
            .entities
            .iter()
            .flat_map(|entity| entity.geometry.extent_points())
            .filter(|p| p.x.is_finite() && p.y.is_finite());
        let first = points.next()?;
        Some(points.fold((first, first), |(lo, hi), p| {
            (
                Point2::new(lo.x.min(p.x), lo.y.min(p.y)),
                Point2::new(hi.x.max(p.x), hi.y.max(p.y)),
            )
        }))
    }

    /// The layers that hold entities, in the order of the layer table (then
    /// of first use), with how many entities each holds.
    pub fn used_layers(&self) -> Vec<(String, usize)> {
        let mut used: Vec<(String, usize)> = Vec::new();
        let mut count = |name: &str| match used.iter_mut().find(|(n, _)| n == name) {
            Some((_, n)) => *n += 1,
            None => used.push((name.to_owned(), 1)),
        };
        for entity in &self.entities {
            count(&entity.layer);
        }
        let position = |name: &str| {
            self.layers
                .iter()
                .position(|layer| layer.name == name)
                .unwrap_or(usize::MAX)
        };
        // Stable: layers missing from the table keep their order at the end.
        used.sort_by_key(|(name, _)| position(name));
        used
    }

    /// A copy with only the entities on the given layers.
    pub fn on_layers(&self, layers: &[String]) -> Self {
        Self {
            entities: self
                .entities
                .iter()
                .filter(|entity| layers.contains(&entity.layer))
                .cloned()
                .collect(),
            ..self.clone()
        }
    }

    /// A copy with every entity mapped by `transform`.
    pub fn transformed(&self, transform: &Affine) -> Self {
        Self {
            entities: self
                .entities
                .iter()
                .map(|entity| {
                    Entity::new(entity.layer.clone(), entity.geometry.transformed(transform))
                })
                .collect(),
            ..self.clone()
        }
    }

    /// A copy in millimetres. A unitless drawing is taken to be in
    /// `unitless_as` (millimetres when that is unitless too).
    pub fn to_millimeters(&self, unitless_as: Units) -> Self {
        let factor = self
            .units
            .millimeters()
            .or_else(|| unitless_as.millimeters())
            .unwrap_or(1.0);
        let mut drawing = if factor == 1.0 {
            self.clone()
        } else {
            self.transformed(&Affine::scale(factor, factor))
        };
        drawing.units = Units::Millimeters;
        drawing
    }
}

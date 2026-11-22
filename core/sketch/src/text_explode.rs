// SPDX-License-Identifier: MIT
//! A text's outlines as plain sketch geometry (mitcad#2163): Explode Text
//! and the outlines a sketch's DXF export writes.
//!
//! - The pieces of a glyph contour are straight lines and quadratic or
//!   cubic Bézier curves ([`super::text`]); each is exactly a sketch `Line`
//!   or a `Spline` of degree 2 or 3 by its control points with the default
//!   clamped knots on [0, 1] ([`Piece`], [`piece`]), so nothing is lost.
//! - [`SketchEdit::explode_text`] replaces a text (or a QR code) by those
//!   curves where it is laid out now: one curve per piece, contour by
//!   contour, piece by piece (piece k of a contour is its k-th new curve);
//!   the pieces of a contour share their end points, so each contour stays a
//!   closed chain and bounds the same profiles. The curves are free (no
//!   constraints) and not construction; a text's frame rectangle or path
//!   curve stays as it is, and a text parameter it followed stays in the
//!   design.

use super::edit::SketchEdit;
use super::geometry::{Curve2, P2, dist};
use super::text::TextOutline;
use super::{Entity, EntityKind};
use crate::ids::EntityUid;
use crate::profile::SegmentGeometry;

/// A piece of a glyph contour as sketch geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Piece<'a> {
    /// A straight piece from its start to its end.
    Line(P2, P2),
    /// A quadratic or cubic Bézier curve by its control points (three or
    /// four), a clamped spline of their count less one on [0, 1].
    Bezier(&'a [P2]),
}

impl Piece<'_> {
    /// Where the piece starts and ends.
    pub fn ends(&self) -> (P2, P2) {
        match self {
            Self::Line(a, b) => (*a, *b),
            Self::Bezier(control) => (control[0], control[control.len() - 1]),
        }
    }

    /// The piece as profile geometry, as a DXF export writes it: a line,
    /// or a B-spline of degree 2 or 3 with the knots 0 and 1.
    pub fn geometry(&self) -> SegmentGeometry {
        match self {
            Self::Line(a, b) => SegmentGeometry::Line { start: *a, end: *b },
            Self::Bezier(control) => SegmentGeometry::BSpline {
                degree: (control.len() - 1) as u32,
                poles: control.to_vec(),
                weights: Vec::new(),
                knots: vec![0.0, 1.0],
                multiplicities: vec![control.len() as u32; 2],
                periodic: false,
            },
        }
    }
}

/// A piece of a laid out glyph contour as sketch geometry: a line, or a
/// non-rational Bézier curve of degree 2 or 3 on [0, 1]. None for anything
/// else (a contour's pieces are only these).
pub fn piece(part: &Curve2) -> Option<Piece<'_>> {
    match part {
        Curve2::Line { a, b } => Some(Piece::Line(*a, *b)),
        Curve2::Nurbs(n)
            if (2..=3).contains(&n.degree)
                && n.control.len() == n.degree + 1
                && n.weights.iter().all(|w| (*w - 1.0).abs() <= 1e-12) =>
        {
            Some(Piece::Bezier(&n.control))
        }
        _ => None,
    }
}

/// Every piece of a text's outline in order, contour by contour, named
/// `t5.g2.c1#k` (piece k of the contour), with its geometry. A piece that
/// is no line or Bézier curve is left out.
pub fn outline_pieces(outline: &TextOutline) -> Vec<(String, Piece<'_>)> {
    outline
        .contours
        .iter()
        .flat_map(|c| {
            c.parts
                .iter()
                .enumerate()
                .filter_map(move |(k, part)| Some((format!("{}#{k}", c.id), piece(part)?)))
        })
        .collect()
}

/// What Explode Text made.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExplodedText {
    /// The new curves in order: contour by contour, piece by piece.
    pub curves: Vec<EntityUid>,
    /// The new curves of each contour of the outline, in its order.
    pub contours: Vec<Vec<EntityUid>>,
    /// The contours left out: their pieces do not join end to end (the
    /// outline's `dropped`, which bound no profile either).
    pub dropped: usize,
}

/// Two points one within the closing tolerance of a glyph contour
/// ([`super::text::composite`]).
fn same_point(a: P2, b: P2) -> bool {
    dist(a, b) <= 1e-9 * (1.0 + b[0].abs() + b[1].abs())
}

impl SketchEdit<'_> {
    /// Explode Text (mitcad#2163): text `id`, laid out as `outline` (the
    /// sketch's evaluation of it), becomes plain sketch curves in that
    /// place, one per piece of its contours (see the module
    /// documentation). The text goes, with its link to a text parameter;
    /// the parameter stays.
    pub fn explode_text(
        &mut self,
        id: EntityUid,
        outline: &TextOutline,
    ) -> Result<ExplodedText, String> {
        if !self.def.texts.iter().any(|t| t.id == id) {
            return Err(format!("text t{} does not exist", id.0));
        }
        if outline.id != id {
            return Err(format!("the outline given is not that of text t{}", id.0));
        }
        let pieces: Vec<Vec<Piece<'_>>> = outline
            .contours
            .iter()
            .map(|c| c.parts.iter().map(piece).collect::<Option<Vec<_>>>())
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                format!(
                    "text t{} has an outline piece that is neither a line nor a Bézier curve",
                    id.0
                )
            })?;
        if pieces.iter().all(Vec::is_empty) {
            return Err(super::said(
                "the text has no outlines to explode",
                format!("text t{} has no outlines to explode", id.0),
            ));
        }
        let mut made = ExplodedText {
            dropped: outline.dropped.len(),
            ..ExplodedText::default()
        };
        for contour in &pieces {
            let mut curves = Vec::with_capacity(contour.len());
            // The contour's first point, which its last piece ends on.
            let mut first: Option<(EntityUid, P2)> = None;
            let mut last: Option<(EntityUid, P2)> = None;
            for (k, part) in contour.iter().enumerate() {
                let (a, b) = part.ends();
                let start = match last {
                    Some((p, at)) if same_point(at, a) => p,
                    _ => self.add_point(a),
                };
                if k == 0 {
                    first = Some((start, a));
                }
                let inner: Vec<EntityUid> = match part {
                    Piece::Line(..) => Vec::new(),
                    Piece::Bezier(control) => control[1..control.len() - 1]
                        .iter()
                        .map(|at| self.add_point(*at))
                        .collect(),
                };
                let end = match first {
                    Some((p, at)) if k + 1 == contour.len() && same_point(at, b) => p,
                    _ => self.add_point(b),
                };
                let kind = match part {
                    Piece::Line(..) => EntityKind::Line {
                        start,
                        end,
                        centerline: false,
                    },
                    Piece::Bezier(control) => EntityKind::Spline {
                        degree: (control.len() - 1) as u32,
                        control: std::iter::once(start)
                            .chain(inner)
                            .chain(std::iter::once(end))
                            .collect(),
                        weights: Vec::new(),
                        knots: Vec::new(),
                    },
                };
                let curve = self.id();
                self.push(Entity::new(curve, kind));
                curves.push(curve);
                last = Some((end, b));
            }
            made.curves.extend(&curves);
            made.contours.push(curves);
        }
        self.def.texts.retain(|t| t.id != id);
        // Its text parameter's link goes; the parameter stays (mitcad#664).
        self.def.text_values.retain(|v| v.text != id);
        self.report.removed.push(format!("t{}", id.0));
        Ok(made)
    }
}

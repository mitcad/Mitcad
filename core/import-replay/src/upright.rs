// SPDX-License-Identifier: MIT
//! A design modelled with another up direction stood on Z up (mitcad#1937).
//!
//! MitCad models with Z up: its Front view looks along +Y with +Z up. A
//! design file says how it was modelled by its standard `FRONT` named view
//! (`document.named_views`, read by the `.f3d` decoder): a design modelled
//! with Y up has a Front view that looks along -Z with +Y up, so in
//! MitCad's views it lay on its side. After the replay the import turns
//! such a design by the quarter turns that take its Front view onto
//! MitCad's (Y up: 90 degrees about X), with the model's Reorient Design
//! ([`mitcad_model::Document::reorient_design`]): a rewrite of the
//! definitions, so the design stays parametric, its sketches stay on
//! their (turned) origin planes and its coordinates, exports and views
//! agree. The turn is part of the import's undo step.
//!
//! A Front view that is not along the model axes, or a design without
//! named views, is left as it is. Where Reorient Design refuses the turn,
//! the design stays as modelled and the report says why
//! ([`UprightReport::error`], and a warning).

use mitcad_dump::Dump;
use mitcad_model::reorient::{Turn, TurnAxis};
use mitcad_model::{Document, Kernel};
use serde::Serialize;

use crate::report::DesignReport;

/// How the import stood a design on Z up.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct UprightReport {
    /// The design's up direction as modelled (`+Y`), from its Front view.
    pub up: String,
    /// The turn's rotation matrix (rows), in model coordinates.
    pub matrix: [[f64; 3]; 3],
    /// Whether the design was turned.
    pub turned: bool,
    /// Why it was not, in words (Reorient Design's refusal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The design's Front view: the direction it looks along and its up
/// vector, each a signed model axis; `None` without a Front view or for one
/// that is not along the axes.
pub fn front_view(dump: &Dump) -> Option<([f64; 3], [f64; 3])> {
    let views = dump.document.as_ref()?.named_views.as_ref()?;
    let front = views.iter().find(|v| {
        v.name
            .as_deref()
            .is_some_and(|n| n.eq_ignore_ascii_case("FRONT"))
    })?;
    let (eye, target, up) = (front.eye?, front.target?, front.up?);
    let sight = axis_of([target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]])?;
    let up = axis_of(up)?;
    (sight != up && sight.iter().zip(up).all(|(a, b)| a * b == 0.0)).then_some((sight, up))
}

/// The signed model axis a vector runs along (within 1e-6 of its length),
/// exactly.
fn axis_of(v: [f64; 3]) -> Option<[f64; 3]> {
    let length = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if !length.is_finite() || length <= 0.0 {
        return None;
    }
    let mut axis = [0.0; 3];
    for (i, x) in v.iter().enumerate() {
        let x = x / length;
        if (x.abs() - 1.0).abs() <= 1e-6 {
            axis[i] = x.signum();
        } else if x.abs() > 1e-6 {
            return None;
        }
    }
    (axis.iter().filter(|x| **x != 0.0).count() == 1).then_some(axis)
}

/// MitCad's Front view: looking along +Y with +Z up.
const FRONT_SIGHT: [f64; 3] = [0.0, 1.0, 0.0];
const FRONT_UP: [f64; 3] = [0.0, 0.0, 1.0];

/// The turn that takes the design's Front view onto MitCad's: `None` for a
/// design modelled with Z up (no turn) or without a Front view along the
/// axes.
pub fn upright_turn(dump: &Dump) -> Option<(Turn, [f64; 3])> {
    let (sight, up) = front_view(dump)?;
    let quarters = |axis, n| Turn::about(axis, n);
    let turn = (0..4)
        .flat_map(|x| (0..4).flat_map(move |y| (0..4).map(move |z| (x, y, z))))
        .map(|(x, y, z)| {
            quarters(TurnAxis::Z, z)
                .after(&quarters(TurnAxis::Y, y).after(&quarters(TurnAxis::X, x)))
        })
        .find(|t| t.vector(sight) == FRONT_SIGHT && t.vector(up) == FRONT_UP)?;
    (!turn.is_identity()).then_some((turn, up))
}

/// The name of a signed model axis (`+Y`).
fn axis_name(v: [f64; 3]) -> String {
    let i = v.iter().position(|x| *x != 0.0).unwrap_or(2);
    format!(
        "{}{}",
        if v[i] < 0.0 { "-" } else { "+" },
        ["X", "Y", "Z"][i]
    )
}

/// Turns a design modelled with another up direction to stand on Z up
/// (see the module documentation), writing what it did into the report.
pub fn stand_upright<K: Kernel>(doc: &mut Document<K>, dump: &Dump, report: &mut DesignReport) {
    let Some((turn, up)) = upright_turn(dump) else {
        return;
    };
    let up = axis_name(up);
    let mut upright = UprightReport {
        up: up.clone(),
        matrix: turn.matrix(),
        ..UprightReport::default()
    };
    match doc.reorient_design(turn) {
        Ok(_) => upright.turned = true,
        Err(e) => {
            report.warnings.push(format!(
                "The design was modelled with {up} up and could not be turned to stand \
                 on Z up, so it lies as it was modelled: {e}"
            ));
            upright.error = Some(e.to_string());
        }
    }
    report.upright = Some(upright);
}

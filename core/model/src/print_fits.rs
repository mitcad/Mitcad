// SPDX-License-Identifier: MIT
//! Ordinary user parameters offered by the printed-part command panels.

use crate::expr::{Expr, ParamSpec, Unit};
use crate::parameters::{ParameterError, Parameters};

pub(crate) struct Fit {
    pub name: &'static str,
    pub field: &'static str,
    pub features: &'static [&'static str],
    pub millimetres: f64,
    pub comment: &'static str,
}

/// The modelled thread's flank fit: capped per thread at what its pitch
/// allows (`features::thread::fitted_clearance`, mitcad#691).
pub(crate) const THREAD_FLANK_CLEARANCE: &str = "thread_flank_clearance";

pub(crate) const FITS: &[Fit] = &[
    Fit {
        name: "thread_radial_clearance",
        field: "radial_clearance",
        features: &["thread"],
        millimetres: 0.1,
        comment: "Printed thread fit, radial; depends on dimensional accuracy",
    },
    Fit {
        name: THREAD_FLANK_CLEARANCE,
        field: "flank_clearance",
        features: &["thread"],
        millimetres: 0.1,
        comment: "Printed thread fit, flanks; depends on overhang quality",
    },
    Fit {
        name: "shaft_bore_clearance",
        field: "bore_clearance",
        features: &["gear", "pulley"],
        millimetres: 0.2,
        comment: "Printed shaft bore fit; depends on hole and shaft dimensional accuracy",
    },
    Fit {
        name: "gear_backlash",
        field: "backlash",
        features: &["gear"],
        millimetres: 0.2,
        comment: "Printed gear mesh backlash; depends on tooth accuracy and extrusion width",
    },
    Fit {
        name: "bearing_radial_clearance",
        field: "radial_clearance",
        features: &["printable_bearing"],
        millimetres: 0.4,
        comment: "Printed bearing radial gaps; depends on circle accuracy and ringing",
    },
    Fit {
        name: "bearing_axial_clearance",
        field: "axial_clearance",
        features: &["printable_bearing"],
        millimetres: 0.4,
        comment: "Printed bearing axial gaps; depends on overhang and layer accuracy",
    },
    Fit {
        name: "bearing_element_gap",
        field: "element_gap",
        features: &["printable_bearing"],
        millimetres: 0.6,
        comment: "Printed bearing element gaps; depends on circle accuracy and ringing",
    },
    Fit {
        name: "bearing_fit_clearance",
        field: "fit_clearance",
        features: &["printable_bearing"],
        millimetres: 0.5,
        comment: "Printed bearing fit on shafts or in holes; depends on dimensional accuracy",
    },
    // A press-fit hole's flats or crush ribs under its pin (mitcad#1517).
    Fit {
        name: "press_fit_interference",
        field: "interference",
        features: &["hole"],
        millimetres: 0.1,
        comment: "Printed press fit on the diameter, flats or crush rib tips under the pin; \
                  depends on dimensional accuracy",
    },
];

/// The existing parameter always wins, including its unit and comment.
fn add(params: &mut Parameters, fit: &Fit) -> Result<bool, ParameterError> {
    if params.find(fit.name).is_some() {
        return Ok(false);
    }
    params.add(
        ParamSpec::user(fit.name, &format!("{} mm", fit.millimetres), Unit::MM)
            .with_comment(fit.comment),
        None,
    )?;
    Ok(true)
}

/// Called on the transaction's definition state, before resolving a field.
/// Parse references rather than matching substrings or treating omitted
/// fields as a request for parameters.
pub(crate) fn ensure(
    params: &mut Parameters,
    expression: &str,
) -> Result<Vec<String>, ParameterError> {
    let Ok(parsed) = Expr::parse(expression) else {
        return Ok(Vec::new());
    };
    let names = parsed.references();
    let mut created = Vec::new();
    for fit in FITS {
        if names.contains(&fit.name) && add(params, fit)? {
            created.push(fit.name.to_owned());
        }
    }
    Ok(created)
}

/// Expression hints may evaluate a not-yet-created fit in a private copy.
/// This does not add parameters to a design, its undo stack or its file.
pub fn evaluation_parameters(params: &Parameters) -> Result<Parameters, ParameterError> {
    let mut copy = params.clone();
    for fit in FITS {
        add(&mut copy, fit)?;
    }
    Ok(copy)
}

pub fn json() -> serde_json::Value {
    serde_json::Value::Array(
        FITS.iter()
            .map(|fit| {
                serde_json::json!({
                    "name": fit.name, "field": fit.field, "features": fit.features,
                    "value": fit.millimetres, "unit": "mm", "comment": fit.comment,
                })
            })
            .collect(),
    )
}

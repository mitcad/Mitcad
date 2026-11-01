// SPDX-License-Identifier: MIT
//! The project's units (IfcUnitAssignment of IfcProject.UnitsInContext):
//! SI units with prefixes and conversion-based units (feet, inches,
//! degrees), as Mitcad's millimetres and radians.

use serde::Serialize;

use crate::{Entity, IfcFile, Value};

/// The units lengths, angles, areas and volumes of the file are in.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Units {
    /// Millimetres per length unit.
    pub length: f64,
    /// Radians per plane angle unit.
    pub angle: f64,
    /// Cubic millimetres per volume unit.
    pub volume: f64,
    /// Square millimetres per area unit.
    pub area: f64,
    /// The units' names as the file gives them (`MILLI METRE`, `FOOT`,
    /// `degree`).
    pub length_name: String,
    pub angle_name: String,
    pub volume_name: String,
    /// What was assumed (no unit assignment, a unit not understood).
    pub notes: Vec<String>,
}

impl Default for Units {
    /// Metres and radians (what a file without units means).
    fn default() -> Self {
        Self {
            length: 1000.0,
            angle: 1.0,
            volume: 1e9,
            area: 1e6,
            length_name: "METRE".to_owned(),
            angle_name: "RADIAN".to_owned(),
            volume_name: "CUBIC_METRE".to_owned(),
            notes: Vec::new(),
        }
    }
}

/// An SI prefix's factor.
fn prefix(name: &str) -> Option<f64> {
    Some(match name {
        "EXA" => 1e18,
        "PETA" => 1e15,
        "TERA" => 1e12,
        "GIGA" => 1e9,
        "MEGA" => 1e6,
        "KILO" => 1e3,
        "HECTO" => 1e2,
        "DECA" => 1e1,
        "DECI" => 1e-1,
        "CENTI" => 1e-2,
        "MILLI" => 1e-3,
        "MICRO" => 1e-6,
        "NANO" => 1e-9,
        "PICO" => 1e-12,
        "FEMTO" => 1e-15,
        "ATTO" => 1e-18,
        _ => return None,
    })
}

/// A unit's kind (`LENGTHUNIT`) and its size in Mitcad's units (mm, rad,
/// mm², mm³), with its name; None for a unit not understood.
pub fn unit_value(file: &IfcFile, unit: &Entity, depth: usize) -> Option<(String, f64, String)> {
    if depth > 8 {
        return None;
    }
    if unit.is("IFCSIUNIT") {
        // (Dimensions, UnitType, Prefix, Name)
        let kind = unit.arg(1).as_enum()?.to_owned();
        let p = match unit.arg(2).as_enum() {
            Some(name) => prefix(name)?,
            None => 1.0,
        };
        let name = unit.arg(3).as_enum()?;
        let base = match name {
            "METRE" => 1000.0,
            "SQUARE_METRE" => 1e6,
            "CUBIC_METRE" => 1e9,
            "RADIAN" => 1.0,
            _ => 1.0,
        };
        // A prefix scales the unit (a square or cubic one by its power).
        let power = match name {
            "SQUARE_METRE" => 2,
            "CUBIC_METRE" => 3,
            _ => 1,
        };
        let label = match unit.arg(2).as_enum() {
            Some(pre) => format!("{pre} {name}"),
            None => name.to_owned(),
        };
        return Some((kind, base * p.powi(power), label));
    }
    if unit.is("IFCCONVERSIONBASEDUNIT") || unit.is("IFCCONVERSIONBASEDUNITWITHOFFSET") {
        // (Dimensions, UnitType, Name, ConversionFactor)
        let kind = unit.arg(1).as_enum()?.to_owned();
        let name = unit.arg(2).as_str().unwrap_or_default().to_owned();
        let factor = file.deref(unit.arg(3))?;
        // IfcMeasureWithUnit(ValueComponent, UnitComponent)
        let value = factor.arg(0).as_f64()?;
        let inner = file.deref(factor.arg(1))?;
        let (_, size, _) = unit_value(file, inner, depth + 1)?;
        return Some((kind, value * size, name));
    }
    None
}

impl Units {
    /// The units of the file's project; metres and radians, with a note,
    /// when it assigns none.
    pub fn of(file: &IfcFile) -> Units {
        let mut units = Units::default();
        let project = file.of_kind("IFCPROJECT");
        let Some(project) = project.first() else {
            units
                .notes
                .push("the file has no IfcProject: meters and radians assumed".to_owned());
            return units;
        };
        // IfcContext: (..., RepresentationContexts, UnitsInContext) at 7, 8.
        let Some(assignment) = file.deref(project.arg(8)) else {
            units
                .notes
                .push("the project assigns no units: meters and radians assumed".to_owned());
            return units;
        };
        let mut seen_length = false;
        for unit in assignment.arg(0).refs() {
            let Some(unit) = file.get(unit) else {
                continue;
            };
            match unit_value(file, unit, 0) {
                Some((kind, size, name)) => match kind.as_str() {
                    "LENGTHUNIT" => {
                        units.length = size;
                        units.length_name = name;
                        seen_length = true;
                    }
                    "PLANEANGLEUNIT" => {
                        units.angle = size;
                        units.angle_name = name;
                    }
                    "VOLUMEUNIT" => {
                        units.volume = size;
                        units.volume_name = name;
                    }
                    "AREAUNIT" => units.area = size,
                    _ => {}
                },
                None => {
                    if matches!(
                        unit.arg(1).as_enum(),
                        Some("LENGTHUNIT" | "PLANEANGLEUNIT" | "VOLUMEUNIT")
                    ) {
                        units.notes.push(format!(
                            "#{} ({}) is not understood: its default is used",
                            unit.id, unit.kind
                        ));
                    }
                }
            }
        }
        if !seen_length {
            units
                .notes
                .push("no length unit is assigned: meters assumed".to_owned());
        }
        units
    }

    /// The size of a quantity's own unit (IfcQuantityVolume.Unit) when it
    /// gives one, else the project's.
    pub fn volume_of(&self, file: &IfcFile, unit: &Value) -> f64 {
        file.deref(unit)
            .and_then(|u| unit_value(file, u, 0))
            .filter(|(kind, _, _)| kind == "VOLUMEUNIT")
            .map_or(self.volume, |(_, size, _)| size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_of_projects() {
        let file = IfcFile::parse(crate::testdata::BUILDING).expect("reads");
        let units = Units::of(&file);
        assert_eq!(units.length, 1.0);
        assert!((units.angle - std::f64::consts::PI / 180.0).abs() < 1e-9);
        assert_eq!(units.volume, 1e9);
        assert!(units.notes.is_empty(), "{:?}", units.notes);
        let feet = IfcFile::parse(
            "ISO-10303-21;HEADER;FILE_SCHEMA(('IFC2X3'));ENDSEC;DATA;\
             #1=IFCPROJECT('0123456789012345678901',$,$,$,$,$,$,$,#2);\
             #2=IFCUNITASSIGNMENT((#3,#6));\
             #3=IFCCONVERSIONBASEDUNIT(#4,.LENGTHUNIT.,'FOOT',#5);\
             #4=IFCDIMENSIONALEXPONENTS(1,0,0,0,0,0,0);\
             #5=IFCMEASUREWITHUNIT(IFCLENGTHMEASURE(0.3048),#7);\
             #6=IFCSIUNIT(*,.VOLUMEUNIT.,.CENTI.,.CUBIC_METRE.);\
             #7=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);ENDSEC;END-ISO-10303-21;",
        )
        .expect("reads");
        let units = Units::of(&feet);
        assert!((units.length - 304.8).abs() < 1e-9);
        assert_eq!(units.length_name, "FOOT");
        assert!((units.volume - 1e3).abs() < 1e-9);
        assert_eq!(units.angle, 1.0);
    }
}

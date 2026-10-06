// SPDX-License-Identifier: MIT

/// Drawing units, the `$INSUNITS` header variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Units {
    /// No units given (`$INSUNITS` 0 or missing).
    #[default]
    Unitless,
    Inches,
    Feet,
    Miles,
    Millimeters,
    Centimeters,
    Meters,
    Kilometers,
    Microinches,
    Mils,
    Yards,
    Angstroms,
    Nanometers,
    Microns,
    Decimeters,
    Decameters,
    Hectometers,
    Gigameters,
    AstronomicalUnits,
    LightYears,
    Parsecs,
    UsSurveyFeet,
    UsSurveyInches,
    UsSurveyYards,
    UsSurveyMiles,
}

const TABLE: [(Units, i32, Option<f64>); 25] = [
    (Units::Unitless, 0, None),
    (Units::Inches, 1, Some(25.4)),
    (Units::Feet, 2, Some(304.8)),
    (Units::Miles, 3, Some(1_609_344.0)),
    (Units::Millimeters, 4, Some(1.0)),
    (Units::Centimeters, 5, Some(10.0)),
    (Units::Meters, 6, Some(1000.0)),
    (Units::Kilometers, 7, Some(1.0e6)),
    (Units::Microinches, 8, Some(25.4e-6)),
    (Units::Mils, 9, Some(0.0254)),
    (Units::Yards, 10, Some(914.4)),
    (Units::Angstroms, 11, Some(1.0e-7)),
    (Units::Nanometers, 12, Some(1.0e-6)),
    (Units::Microns, 13, Some(1.0e-3)),
    (Units::Decimeters, 14, Some(100.0)),
    (Units::Decameters, 15, Some(1.0e4)),
    (Units::Hectometers, 16, Some(1.0e5)),
    (Units::Gigameters, 17, Some(1.0e12)),
    (Units::AstronomicalUnits, 18, Some(1.495_978_707e14)),
    (Units::LightYears, 19, Some(9.460_730_472_580_8e18)),
    (Units::Parsecs, 20, Some(3.085_677_581_491_367e19)),
    // US survey foot = 1200/3937 m.
    (Units::UsSurveyFeet, 21, Some(1_200_000.0 / 3937.0)),
    (Units::UsSurveyInches, 22, Some(100_000.0 / 3937.0)),
    (Units::UsSurveyYards, 23, Some(3_600_000.0 / 3937.0)),
    (Units::UsSurveyMiles, 24, Some(6_336_000_000.0 / 3937.0)),
];

impl Units {
    /// The unit with `$INSUNITS` code `code`, or `None` for an unknown code.
    pub fn from_code(code: i32) -> Option<Self> {
        TABLE.iter().find(|row| row.1 == code).map(|row| row.0)
    }

    /// The `$INSUNITS` code.
    pub fn code(self) -> i32 {
        self.row().1
    }

    /// Length of one unit in millimetres; `None` when unitless.
    pub fn millimeters(self) -> Option<f64> {
        self.row().2
    }

    /// True for metric units, which DXF marks with `$MEASUREMENT` 1.
    pub fn is_metric(self) -> bool {
        matches!(
            self,
            Self::Millimeters
                | Self::Centimeters
                | Self::Meters
                | Self::Kilometers
                | Self::Angstroms
                | Self::Nanometers
                | Self::Microns
                | Self::Decimeters
                | Self::Decameters
                | Self::Hectometers
                | Self::Gigameters
        )
    }

    fn row(self) -> &'static (Units, i32, Option<f64>) {
        TABLE
            .iter()
            .find(|row| row.0 == self)
            .expect("every unit is in the table")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for code in 0..=24 {
            let unit = Units::from_code(code).unwrap();
            assert_eq!(unit.code(), code);
        }
        assert_eq!(Units::from_code(25), None);
        assert_eq!(Units::from_code(-1), None);
    }

    #[test]
    fn lengths_in_millimeters() {
        assert_eq!(Units::Millimeters.millimeters(), Some(1.0));
        assert_eq!(Units::Inches.millimeters(), Some(25.4));
        assert_eq!(Units::Meters.millimeters(), Some(1000.0));
        assert_eq!(Units::Unitless.millimeters(), None);
        let survey = Units::UsSurveyFeet.millimeters().unwrap();
        assert!((survey - 304.800_609_6).abs() < 1e-6);
        assert!(Units::Millimeters.is_metric());
        assert!(!Units::Inches.is_metric());
    }
}

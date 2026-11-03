// SPDX-License-Identifier: MIT
//! Units of materials' properties (mitcad#1213). The model keeps a
//! material's values in one set of units: the density in g/cm³, stresses
//! (Young's modulus, the yield strength) in MPa, temperatures in °C and the
//! coefficient of thermal expansion in µm/(m·°C). They are shown in the
//! design's unit system: a metric design (millimetres, centimetres, metres)
//! in those, an inch design (inches, feet, mils) in psi, lb/in³ and
//! µin/(in·°F); temperatures in °C or °F as the user prefers, whatever the
//! design's system (the application's preference). The expansion
//! coefficient's degree follows the design's system, as its length does.
//!
//! Every unit here is the model's unit scaled and shifted: a shown value is
//! `value * scale + offset` ([`ShownUnit`]), so that the application
//! converts both ways without a table of its own. The exchange files of
//! materials name their units in the words [`parse_unit`] reads.

use std::sync::atomic::{AtomicU8, Ordering};

use crate::expr::LengthUnit;

/// Pascals in a pound-force per square inch: a pound-force (4.4482216152605
/// N) over a square inch (0.0254 m squared).
const PA_PER_PSI: f64 = 4.448_221_615_260_5 / (0.0254 * 0.0254);
/// MPa in a psi.
const MPA_PER_PSI: f64 = PA_PER_PSI * 1e-6;
/// g/cm³ in a lb/in³: a pound (453.59237 g) over a cubic inch (16.387064
/// cm³).
const G_CM3_PER_LB_IN3: f64 = 453.592_37 / 16.387_064;

/// A design's unit system for its materials' values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitSystem {
    /// MPa, g/cm³, µm/(m·°C).
    Metric,
    /// psi, lb/in³, µin/(in·°F).
    Imperial,
}

impl UnitSystem {
    /// The system of a design whose lengths are in `unit`: inches, feet,
    /// yards, miles and mils make it imperial, the others metric.
    pub fn of_length(unit: LengthUnit) -> UnitSystem {
        match unit {
            LengthUnit::Inch
            | LengthUnit::Foot
            | LengthUnit::Yard
            | LengthUnit::Mile
            | LengthUnit::Mil => UnitSystem::Imperial,
            _ => UnitSystem::Metric,
        }
    }

    /// Its name in commands and queries: `metric` or `imperial`.
    pub fn id(self) -> &'static str {
        match self {
            UnitSystem::Metric => "metric",
            UnitSystem::Imperial => "imperial",
        }
    }

    /// A system by its name, `metric` or `imperial` (any case).
    pub fn from_id(text: &str) -> Option<UnitSystem> {
        match text.trim().to_ascii_lowercase().as_str() {
            "metric" => Some(UnitSystem::Metric),
            "imperial" => Some(UnitSystem::Imperial),
            _ => None,
        }
    }
}

/// The unit temperatures are shown in: the user's preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TemperatureUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

impl TemperatureUnit {
    /// Its name in commands, queries and files: `C` or `F`.
    pub fn id(self) -> &'static str {
        match self {
            TemperatureUnit::Celsius => "C",
            TemperatureUnit::Fahrenheit => "F",
        }
    }

    /// A unit by its name: `C`, `°C`, `celsius`, `F`, `°F`, `fahrenheit`
    /// (any case).
    pub fn from_id(text: &str) -> Option<TemperatureUnit> {
        match normalized(text).as_str() {
            "c" | "celsius" => Some(TemperatureUnit::Celsius),
            "f" | "fahrenheit" => Some(TemperatureUnit::Fahrenheit),
            _ => None,
        }
    }
}

/// The temperature unit the user prefers, which every temperature shown
/// follows (mitcad#1213, the owner's decision 3: the material card,
/// Physical Properties, parts lists and bills of materials): set by the
/// application from its preference, °C until then.
static PREFERRED_TEMPERATURE: AtomicU8 = AtomicU8::new(0);

/// Sets the temperature unit the user prefers (the application, at start
/// and when the preference changes).
pub fn set_preferred_temperature(unit: TemperatureUnit) {
    let code = match unit {
        TemperatureUnit::Celsius => 0,
        TemperatureUnit::Fahrenheit => 1,
    };
    PREFERRED_TEMPERATURE.store(code, Ordering::Relaxed);
}

/// The temperature unit the user prefers: °C unless the application set
/// another.
pub fn preferred_temperature() -> TemperatureUnit {
    match PREFERRED_TEMPERATURE.load(Ordering::Relaxed) {
        1 => TemperatureUnit::Fahrenheit,
        _ => TemperatureUnit::Celsius,
    }
}

/// What a material's value measures, which decides its unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantity {
    /// A stress or a modulus: MPa in the model.
    Stress,
    /// g/cm³ in the model.
    Density,
    /// °C in the model.
    Temperature,
    /// A coefficient of thermal expansion: µm/(m·°C) in the model.
    Expansion,
    /// A number without a unit (Poisson's ratio).
    Ratio,
}

impl Quantity {
    /// Its name in an exchange file's units: `stress`, `density`,
    /// `temperature`, `expansion`; none for a ratio.
    pub fn id(self) -> &'static str {
        match self {
            Quantity::Stress => "stress",
            Quantity::Density => "density",
            Quantity::Temperature => "temperature",
            Quantity::Expansion => "expansion",
            Quantity::Ratio => "ratio",
        }
    }

    /// The quantities that have units, in the order files list them.
    pub const WITH_UNITS: [Quantity; 4] = [
        Quantity::Stress,
        Quantity::Density,
        Quantity::Temperature,
        Quantity::Expansion,
    ];
}

/// A unit of a quantity: its symbol and how a value in the model's unit
/// becomes one in it, `value * scale + offset`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShownUnit {
    pub symbol: &'static str,
    pub scale: f64,
    pub offset: f64,
}

impl ShownUnit {
    const fn new(symbol: &'static str, scale: f64) -> ShownUnit {
        ShownUnit {
            symbol,
            scale,
            offset: 0.0,
        }
    }

    /// A value in the model's unit in this one.
    pub fn from_model(self, value: f64) -> f64 {
        value * self.scale + self.offset
    }

    /// A value in this unit in the model's.
    pub fn to_model(self, value: f64) -> f64 {
        (value - self.offset) / self.scale
    }
}

const MPA: ShownUnit = ShownUnit::new("MPa", 1.0);
const GPA: ShownUnit = ShownUnit::new("GPa", 1e-3);
const PSI: ShownUnit = ShownUnit::new("psi", 1.0 / MPA_PER_PSI);
const KSI: ShownUnit = ShownUnit::new("ksi", 1e-3 / MPA_PER_PSI);
const G_CM3: ShownUnit = ShownUnit::new("g/cm³", 1.0);
const KG_M3: ShownUnit = ShownUnit::new("kg/m³", 1000.0);
const LB_IN3: ShownUnit = ShownUnit::new("lb/in³", 1.0 / G_CM3_PER_LB_IN3);
const CELSIUS: ShownUnit = ShownUnit::new("°C", 1.0);
const FAHRENHEIT: ShownUnit = ShownUnit {
    symbol: "°F",
    scale: 1.8,
    offset: 32.0,
};
const UM_M_C: ShownUnit = ShownUnit::new("µm/(m·°C)", 1.0);
const UIN_IN_F: ShownUnit = ShownUnit::new("µin/(in·°F)", 1.0 / 1.8);
const RATIO: ShownUnit = ShownUnit::new("", 1.0);

/// The unit a quantity is shown in: the design's system's, temperatures in
/// `temperature`.
pub fn shown_unit(
    quantity: Quantity,
    system: UnitSystem,
    temperature: TemperatureUnit,
) -> ShownUnit {
    match (quantity, system) {
        (Quantity::Stress, UnitSystem::Metric) => MPA,
        (Quantity::Stress, UnitSystem::Imperial) => PSI,
        (Quantity::Density, UnitSystem::Metric) => G_CM3,
        (Quantity::Density, UnitSystem::Imperial) => LB_IN3,
        (Quantity::Expansion, UnitSystem::Metric) => UM_M_C,
        (Quantity::Expansion, UnitSystem::Imperial) => UIN_IN_F,
        (Quantity::Temperature, _) => temperature_unit(temperature),
        (Quantity::Ratio, _) => RATIO,
    }
}

/// The shown unit of a temperature unit.
pub fn temperature_unit(unit: TemperatureUnit) -> ShownUnit {
    match unit {
        TemperatureUnit::Celsius => CELSIUS,
        TemperatureUnit::Fahrenheit => FAHRENHEIT,
    }
}

/// The model's unit of a quantity (MPa, g/cm³, °C, µm/(m·°C)).
pub fn model_unit(quantity: Quantity) -> ShownUnit {
    shown_unit(quantity, UnitSystem::Metric, TemperatureUnit::Celsius)
}

/// A unit's name in one form: lower case, no spaces, `µ` and `μ` as `u`,
/// `·` as `*`, `³` and `²` as digits, no degree sign.
fn normalized(text: &str) -> String {
    text.trim()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '°')
        .map(|c| match c {
            'µ' | 'μ' => 'u',
            '·' | '⋅' => '*',
            '³' => '3',
            '²' => '2',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

/// A unit of a quantity as an exchange file names it: stresses `MPa`,
/// `N/mm2`, `GPa`, `psi`, `ksi`; densities `g/cm3`, `kg/m3`, `lb/in3`;
/// temperatures `C`, `F`; expansion coefficients `um/(m*C)`, `um/(m*K)`,
/// `1e-6/K`, `uin/(in*F)`, `1e-6/F` (the symbols with `µ`, `·`, `°` and `³`
/// too, any case). None for a word it does not know.
pub fn parse_unit(quantity: Quantity, text: &str) -> Option<ShownUnit> {
    let word = normalized(text);
    let unit = match quantity {
        Quantity::Stress => match word.as_str() {
            "mpa" | "n/mm2" => MPA,
            "gpa" => GPA,
            "psi" => PSI,
            "ksi" => KSI,
            _ => return None,
        },
        Quantity::Density => match word.as_str() {
            "g/cm3" | "g/cc" => G_CM3,
            "kg/m3" => KG_M3,
            "lb/in3" => LB_IN3,
            _ => return None,
        },
        Quantity::Temperature => temperature_unit(TemperatureUnit::from_id(&word)?),
        Quantity::Expansion => match word.as_str() {
            "um/(m*c)" | "um/m/c" | "um/(m*k)" | "um/m/k" | "1e-6/c" | "1e-6/k" => UM_M_C,
            "uin/(in*f)" | "uin/in/f" | "1e-6/f" => UIN_IN_F,
            _ => return None,
        },
        Quantity::Ratio => RATIO,
    };
    Some(unit)
}

/// The words an exchange file writes for a shown unit: ASCII, which
/// [`parse_unit`] reads back (`g/cm3`, `um/(m*C)`, `F`).
pub fn file_unit(unit: ShownUnit) -> &'static str {
    match unit.symbol {
        "g/cm³" => "g/cm3",
        "kg/m³" => "kg/m3",
        "lb/in³" => "lb/in3",
        "°C" => "C",
        "°F" => "F",
        "µm/(m·°C)" => "um/(m*C)",
        "µin/(in·°F)" => "uin/(in*F)",
        other => other,
    }
}

/// A value of the model's unit as text in `unit`, with the unit's symbol
/// (`200 GPa` is never written: the stresses are shown in MPa or psi):
/// at most `digits` significant digits, no trailing zeros (`29007547.5`
/// psi as `29007500 psi` with 6).
pub fn value_text(value: f64, unit: ShownUnit, digits: usize) -> String {
    let number = significant(unit.from_model(value), digits);
    if unit.symbol.is_empty() {
        number
    } else {
        format!("{number} {}", unit.symbol)
    }
}

/// A number with at most `digits` significant digits, without trailing
/// zeros or a lone decimal point (`0.29`, `12.6`, `29007500`).
pub fn significant(value: f64, digits: usize) -> String {
    if value == 0.0 || !value.is_finite() {
        return format!("{}", if value == 0.0 { 0.0 } else { value });
    }
    let digits = digits.max(1) as i32;
    let magnitude = value.abs().log10().floor() as i32;
    let decimals = (digits - 1 - magnitude).max(0) as usize;
    let rounded = if magnitude >= digits {
        let step = 10f64.powi(magnitude + 1 - digits);
        (value / step).round() * step
    } else {
        value
    };
    let text = format!("{rounded:.decimals$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    };
    if text == "-0" { "0".to_owned() } else { text }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
    }

    #[test]
    fn metric_and_imperial_values_convert_both_ways() {
        let psi = shown_unit(
            Quantity::Stress,
            UnitSystem::Imperial,
            TemperatureUnit::Celsius,
        );
        assert_eq!(psi.symbol, "psi");
        // 1 psi is 6894.757 Pa; 200 GPa is 29 007 548 psi.
        assert!(close(psi.to_model(1.0), 0.006_894_757_293_168));
        assert!(close(psi.from_model(200_000.0), 29_007_547.546_6));
        assert!(close(psi.to_model(psi.from_model(250.0)), 250.0));
        let density = shown_unit(
            Quantity::Density,
            UnitSystem::Imperial,
            TemperatureUnit::Celsius,
        );
        assert_eq!(density.symbol, "lb/in³");
        // 0.284 lb/in³ is 7.861 g/cm³.
        assert!(close(density.to_model(0.284), 7.861_092_937_697_69));
        assert!(close(density.from_model(density.to_model(0.101)), 0.101));
        let metric = shown_unit(
            Quantity::Stress,
            UnitSystem::Metric,
            TemperatureUnit::Celsius,
        );
        assert_eq!((metric.symbol, metric.from_model(250.0)), ("MPa", 250.0));
    }

    #[test]
    fn temperatures_and_the_expansion_coefficient_convert() {
        let f = temperature_unit(TemperatureUnit::Fahrenheit);
        assert!(close(f.from_model(100.0), 212.0));
        assert!(close(f.to_model(32.0), 0.0));
        assert!(close(f.to_model(f.from_model(-40.0)), -40.0));
        let c = temperature_unit(TemperatureUnit::Celsius);
        assert_eq!(c.from_model(21.5), 21.5);
        // The expansion coefficient's degree follows the design's system:
        // 12 µm/(m·°C) is 6.667 µin/(in·°F).
        let imperial = shown_unit(
            Quantity::Expansion,
            UnitSystem::Imperial,
            TemperatureUnit::Celsius,
        );
        assert_eq!(imperial.symbol, "µin/(in·°F)");
        assert!(close(imperial.from_model(12.0), 12.0 / 1.8));
        assert!(close(imperial.to_model(6.5), 11.7));
        let metric = shown_unit(
            Quantity::Expansion,
            UnitSystem::Metric,
            TemperatureUnit::Fahrenheit,
        );
        assert_eq!(
            (metric.symbol, metric.from_model(12.0)),
            ("µm/(m·°C)", 12.0)
        );
    }

    #[test]
    fn the_preferred_temperature_unit_is_set_once_for_all() {
        assert_eq!(preferred_temperature(), TemperatureUnit::Celsius);
        set_preferred_temperature(TemperatureUnit::Fahrenheit);
        assert_eq!(preferred_temperature(), TemperatureUnit::Fahrenheit);
        set_preferred_temperature(TemperatureUnit::Celsius);
        assert_eq!(preferred_temperature(), TemperatureUnit::Celsius);
    }

    #[test]
    fn a_designs_length_gives_its_system() {
        assert_eq!(
            UnitSystem::of_length(LengthUnit::Millimetre),
            UnitSystem::Metric
        );
        assert_eq!(UnitSystem::of_length(LengthUnit::Metre), UnitSystem::Metric);
        assert_eq!(
            UnitSystem::of_length(LengthUnit::Inch),
            UnitSystem::Imperial
        );
        assert_eq!(UnitSystem::of_length(LengthUnit::Mil), UnitSystem::Imperial);
        assert_eq!(UnitSystem::from_id("Imperial"), Some(UnitSystem::Imperial));
        assert_eq!(UnitSystem::from_id("si"), None);
    }

    #[test]
    fn files_name_their_units_in_words_read_back() {
        assert_eq!(parse_unit(Quantity::Stress, "N/mm²"), Some(MPA));
        assert_eq!(parse_unit(Quantity::Stress, "PSI"), Some(PSI));
        assert_eq!(parse_unit(Quantity::Stress, "bar"), None);
        assert_eq!(parse_unit(Quantity::Density, "lb/in³"), Some(LB_IN3));
        assert_eq!(parse_unit(Quantity::Temperature, "°F"), Some(FAHRENHEIT));
        assert_eq!(parse_unit(Quantity::Expansion, "µm/(m·°C)"), Some(UM_M_C));
        assert_eq!(
            parse_unit(Quantity::Expansion, "uin/(in*F)"),
            Some(UIN_IN_F)
        );
        for system in [UnitSystem::Metric, UnitSystem::Imperial] {
            for temperature in [TemperatureUnit::Celsius, TemperatureUnit::Fahrenheit] {
                for quantity in Quantity::WITH_UNITS {
                    let unit = shown_unit(quantity, system, temperature);
                    assert_eq!(
                        parse_unit(quantity, file_unit(unit)),
                        Some(unit),
                        "{quantity:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn values_are_written_with_their_significant_digits() {
        assert_eq!(significant(29_007_547.546_6, 6), "29007500");
        assert_eq!(significant(0.29, 6), "0.29");
        assert_eq!(significant(6.666_666_7, 4), "6.667");
        assert_eq!(significant(-40.0, 6), "-40");
        assert_eq!(significant(0.0, 6), "0");
        let psi = shown_unit(
            Quantity::Stress,
            UnitSystem::Imperial,
            TemperatureUnit::Celsius,
        );
        assert_eq!(value_text(250.0, psi, 6), "36259.4 psi");
        assert_eq!(value_text(0.3, RATIO, 6), "0.3");
    }
}

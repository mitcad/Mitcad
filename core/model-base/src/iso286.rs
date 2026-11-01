// SPDX-License-Identifier: MIT
//! ISO 286 fit codes (mitcad#702): the deviations of a hole's (`H7`) or a
//! shaft's (`g6`) tolerance class from a nominal size, for an annotation's
//! tolerance.
//!
//! Covered: nominal sizes over 0 up to 500 mm; the standard tolerance
//! grades IT5 to IT11 (ISO 286-1, table 1); the fundamental deviations of
//! the holes C, D, E, F, G, H, JS, K, M, N, P, R, S and the shafts c, d, e,
//! f, g, h, js, k, m, n, p, r, s (ISO 286-1, tables 2 and 3), with the Δ
//! values of the holes K, M, N (up to IT8) and P, R, S (up to IT7) and the
//! exception of M6 of 250 to 315 mm (−9 µm). Those
//! give the common classes of ISO 286-2 (`H7`, `H8`, `G7`, `F8`, `K7`,
//! `N7`, `P7`, `g6`, `h6`, `js6`, `k6`, `m6`, `n6`, `p6`, `r6`, `s6`, `f7`,
//! `e8`, `d9`, `c11`, ...). Other letters (A, B, J, T to ZC), the grades
//! IT01 to IT4 and IT12 up, and sizes over 500 mm are refused with the
//! reason. Values in the tables are micrometres; the deviations given are
//! millimetres.

/// The upper bounds (mm) of the main size ranges: each range is "over the
/// bound before, up to and including this one".
const MAIN: [f64; 13] = [
    3.0, 6.0, 10.0, 18.0, 30.0, 50.0, 80.0, 120.0, 180.0, 250.0, 315.0, 400.0, 500.0,
];

/// The upper bounds (mm) of the intermediate size ranges of the
/// fundamental deviations c, r and s (and C, R, S).
const INTERMEDIATE: [f64; 23] = [
    3.0, 6.0, 10.0, 18.0, 30.0, 40.0, 50.0, 65.0, 80.0, 100.0, 120.0, 140.0, 160.0, 180.0, 200.0,
    225.0, 250.0, 280.0, 315.0, 355.0, 400.0, 450.0, 500.0,
];

/// A size this much (mm) over a range's bound is still in it: the baseline
/// tolerance of geometric comparisons, so that a measured Ø10.000000000002
/// is of the range up to 10.
const BOUND: f64 = 1e-6;

/// The lowest and the highest grade covered.
pub const GRADES: std::ops::RangeInclusive<u8> = 5..=11;

/// Standard tolerances IT4 to IT11 (µm) by main range; IT4 only for the Δ
/// of grade 5.
const IT: [[f64; 13]; 8] = [
    [3., 4., 4., 5., 6., 7., 8., 10., 12., 14., 16., 18., 20.],
    [4., 5., 6., 8., 9., 11., 13., 15., 18., 20., 23., 25., 27.],
    [6., 8., 9., 11., 13., 16., 19., 22., 25., 29., 32., 36., 40.],
    [
        10., 12., 15., 18., 21., 25., 30., 35., 40., 46., 52., 57., 63.,
    ],
    [
        14., 18., 22., 27., 33., 39., 46., 54., 63., 72., 81., 89., 97.,
    ],
    [
        25., 30., 36., 43., 52., 62., 74., 87., 100., 115., 130., 140., 155.,
    ],
    [
        40., 48., 58., 70., 84., 100., 120., 140., 160., 185., 210., 230., 250.,
    ],
    [
        60., 75., 90., 110., 130., 160., 190., 220., 250., 290., 320., 360., 400.,
    ],
];

/// Shafts' upper deviations es (µm) by main range: d, e, f, g (h is 0).
const D: [f64; 13] = [
    -20., -30., -40., -50., -65., -80., -100., -120., -145., -170., -190., -210., -230.,
];
const E: [f64; 13] = [
    -14., -20., -25., -32., -40., -50., -60., -72., -85., -100., -110., -125., -135.,
];
const F: [f64; 13] = [
    -6., -10., -13., -16., -20., -25., -30., -36., -43., -50., -56., -62., -68.,
];
const G: [f64; 13] = [
    -2., -4., -5., -6., -7., -9., -10., -12., -14., -15., -17., -18., -20.,
];
/// The shaft c's es (µm) by intermediate range.
const C: [f64; 23] = [
    -60., -70., -80., -95., -110., -120., -130., -140., -150., -170., -180., -200., -210., -230.,
    -240., -260., -280., -300., -330., -360., -400., -440., -480.,
];

/// Shafts' lower deviations ei (µm) by main range: k (grades 4 to 7; 0
/// otherwise), m, n, p.
const K: [f64; 13] = [0., 1., 1., 1., 2., 2., 2., 3., 3., 4., 4., 4., 5.];
const M: [f64; 13] = [2., 4., 6., 7., 8., 9., 11., 13., 15., 17., 20., 21., 23.];
const N: [f64; 13] = [
    4., 8., 10., 12., 15., 17., 20., 23., 27., 31., 34., 37., 40.,
];
const P: [f64; 13] = [
    6., 12., 15., 18., 22., 26., 32., 37., 43., 50., 56., 62., 68.,
];
/// The shafts r's and s's ei (µm) by intermediate range.
const R: [f64; 23] = [
    10., 15., 19., 23., 28., 34., 34., 41., 43., 51., 54., 63., 65., 68., 77., 80., 84., 94., 98.,
    108., 114., 126., 132.,
];
const S: [f64; 23] = [
    14., 19., 23., 28., 35., 43., 43., 53., 59., 71., 79., 92., 100., 108., 122., 130., 140., 158.,
    170., 190., 208., 232., 252.,
];

/// A tolerance class: its fundamental deviation's letters (`H`, `js`) and
/// its grade; upper case letters are a hole's, lower case a shaft's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FitCode {
    pub letters: String,
    pub grade: u8,
}

impl FitCode {
    /// Reads a code as `H7`, `g6`, `JS8` or `js6`.
    pub fn parse(code: &str) -> Result<Self, String> {
        let code = code.trim();
        let digits = code
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or(code.len());
        let (letters, grade) = code.split_at(digits);
        let known = [
            "C", "D", "E", "F", "G", "H", "JS", "K", "M", "N", "P", "R", "S", "c", "d", "e", "f",
            "g", "h", "js", "k", "m", "n", "p", "r", "s",
        ];
        if !known.contains(&letters) {
            return Err(format!(
                "{code} is no fit code covered: the holes C, D, E, F, G, H, JS, K, M, N, P, R, S \
                 and the shafts c to s with a grade, as H7 or g6"
            ));
        }
        let grade: u8 = grade
            .parse()
            .map_err(|_| format!("{code} has no grade: a fit code is as H7 or g6"))?;
        if !GRADES.contains(&grade) {
            return Err(format!(
                "{code}: the grades covered are IT{} to IT{}",
                GRADES.start(),
                GRADES.end()
            ));
        }
        Ok(Self {
            letters: letters.to_owned(),
            grade,
        })
    }

    /// Whether it is a hole's class.
    pub fn hole(&self) -> bool {
        self.letters.starts_with(|c: char| c.is_ascii_uppercase())
    }

    /// The upper and the lower deviation (mm) of a nominal size (mm).
    pub fn deviations(&self, size: f64) -> Result<(f64, f64), String> {
        if !(size > 0.0 && size <= MAIN[MAIN.len() - 1] + BOUND) {
            return Err(format!(
                "{}{}: the fit codes cover sizes over 0 up to {} mm",
                self.letters,
                self.grade,
                MAIN[MAIN.len() - 1]
            ));
        }
        let main = MAIN
            .iter()
            .position(|&b| size <= b + BOUND)
            .unwrap_or(MAIN.len() - 1);
        let fine = INTERMEDIATE
            .iter()
            .position(|&b| size <= b + BOUND)
            .unwrap_or(INTERMEDIATE.len() - 1);
        let it_of = |grade: u8| IT[usize::from(grade - 4)][main];
        let it = it_of(self.grade);
        // Δ of the holes K to ZC: IT(n) - IT(n-1), none up to 3 mm.
        let delta = if main == 0 {
            0.0
        } else {
            it - it_of(self.grade - 1)
        };
        let k = if (4..=7).contains(&self.grade) {
            K[main]
        } else {
            0.0
        };
        let (upper, lower) = match self.letters.as_str() {
            "c" => (C[fine], C[fine] - it),
            "d" => (D[main], D[main] - it),
            "e" => (E[main], E[main] - it),
            "f" => (F[main], F[main] - it),
            "g" => (G[main], G[main] - it),
            "h" => (0.0, -it),
            "js" | "JS" => (half(it, self.grade), -half(it, self.grade)),
            "k" => (k + it, k),
            "m" => (M[main] + it, M[main]),
            "n" => (N[main] + it, N[main]),
            "p" => (P[main] + it, P[main]),
            "r" => (R[fine] + it, R[fine]),
            "s" => (S[fine] + it, S[fine]),
            "C" => (-C[fine] + it, -C[fine]),
            "D" => (-D[main] + it, -D[main]),
            "E" => (-E[main] + it, -E[main]),
            "F" => (-F[main] + it, -F[main]),
            "G" => (-G[main] + it, -G[main]),
            "H" => (it, 0.0),
            letters => {
                let es = match letters {
                    "K" if self.grade <= 8 => -K[main] + delta,
                    "K" => 0.0,
                    // ISO 286-1's exception: M6 of 250 to 315 mm.
                    "M" if self.grade == 6 && main == 10 => -9.0,
                    "M" if self.grade <= 8 => -M[main] + delta,
                    "M" => -M[main],
                    "N" if self.grade <= 8 => -N[main] + delta,
                    "N" if main == 0 => -N[main],
                    "N" => 0.0,
                    "P" | "R" | "S" => {
                        let ei = match letters {
                            "P" => P[main],
                            "R" => R[fine],
                            _ => S[fine],
                        };
                        if self.grade <= 7 { -ei + delta } else { -ei }
                    }
                    _ => unreachable!("parse knows the letters"),
                };
                (es, es - it)
            }
        };
        Ok((upper / 1000.0 + 0.0, lower / 1000.0 + 0.0))
    }
}

/// Half a standard tolerance for JS and js: of the grades 7 to 11 an odd
/// number of micrometres rounded down to the even one below (ISO 286-1);
/// of the finer grades exactly half, a half micrometre where it is odd.
fn half(it: f64, grade: u8) -> f64 {
    // The tables' tolerances are whole micrometres.
    let whole = it.round();
    if (7..=11).contains(&grade) && (it - whole).abs() < 1e-9 && whole.rem_euclid(2.0) > 0.5 {
        (whole - 1.0) / 2.0
    } else {
        it / 2.0
    }
}

/// The upper and the lower deviation (mm) of a fit code at a nominal size
/// (mm).
pub fn deviations(code: &str, size: f64) -> Result<(f64, f64), String> {
    FitCode::parse(code)?.deviations(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The deviations in micrometres, rounded.
    fn um(code: &str, size: f64) -> (i64, i64) {
        let (upper, lower) = deviations(code, size).unwrap_or_else(|e| panic!("{e}"));
        (
            (upper * 1000.0).round() as i64,
            (lower * 1000.0).round() as i64,
        )
    }

    #[test]
    fn common_hole_and_shaft_classes_have_their_tabled_deviations() {
        // ISO 286-2's tables at 20 mm (over 18 up to 30).
        for (code, expected) in [
            ("H7", (21, 0)),
            ("H8", (33, 0)),
            ("H11", (130, 0)),
            ("G7", (28, 7)),
            ("F8", (53, 20)),
            ("E9", (92, 40)),
            ("D10", (149, 65)),
            ("C11", (240, 110)),
            ("JS7", (10, -10)),
            ("K7", (6, -15)),
            ("K8", (10, -23)),
            ("M6", (-4, -17)),
            ("M7", (0, -21)),
            ("M8", (4, -29)),
            ("N7", (-7, -28)),
            ("N9", (0, -52)),
            ("P7", (-14, -35)),
            ("R7", (-20, -41)),
            ("S7", (-27, -48)),
            ("h6", (0, -13)),
            ("h9", (0, -52)),
            ("g6", (-7, -20)),
            ("f7", (-20, -41)),
            ("e8", (-40, -73)),
            ("d9", (-65, -117)),
            ("c11", (-110, -240)),
            ("k6", (15, 2)),
            ("m6", (21, 8)),
            ("n6", (28, 15)),
            ("p6", (35, 22)),
            ("r6", (41, 28)),
            ("s6", (48, 35)),
        ] {
            assert_eq!(um(code, 20.0), expected, "{code} at 20 mm");
        }
    }

    #[test]
    fn ranges_are_up_to_and_including_their_bound() {
        assert_eq!(um("H7", 3.0), (10, 0));
        assert_eq!(um("H7", 3.001), (12, 0));
        assert_eq!(um("H7", 500.0), (63, 0));
        assert_eq!(um("h6", 0.5), (0, -6));
        // Intermediate ranges of c, r and s.
        assert_eq!(um("r6", 60.0), (60, 41));
        assert_eq!(um("r6", 70.0), (62, 43));
        assert_eq!(um("s6", 110.0), (101, 79));
        assert_eq!(um("c11", 45.0), (-130, -290));
        assert_eq!(um("C11", 35.0), (280, 120));
        // Up to 3 mm: no Δ, and M and N keep their own.
        assert_eq!(um("K7", 2.0), (0, -10));
        assert_eq!(um("M7", 2.0), (-2, -12));
        assert_eq!(um("N9", 2.0), (-4, -29));
        assert_eq!(um("P7", 2.0), (-6, -16));
        // JS of an odd tolerance: rounded down to the even number.
        assert_eq!(um("JS7", 40.0), (12, -12));
        assert_eq!(um("js9", 40.0), (31, -31));
        // JS and js of the finer grades: exactly half, a half micrometre.
        let (upper, lower) = deviations("js6", 20.0).unwrap();
        assert!((upper - 0.0065).abs() < 1e-12 && (lower + 0.0065).abs() < 1e-12);
        // M6 of 250 to 315 mm by ISO 286-1's exception.
        assert_eq!(um("M6", 280.0), (-9, -41));
        assert_eq!(um("M7", 280.0), (0, -52));
        // K8 by its own rule, not the shaft k's.
        assert_eq!(um("K8", 15.0), (8, -19));
        // A measured size a hair over a bound stays in its range.
        assert_eq!(um("H7", 10.000_000_000_002), (15, 0));
        // k beyond grade 7 starts at 0.
        assert_eq!(um("k8", 20.0), (33, 0));
    }

    #[test]
    fn codes_and_sizes_not_covered_are_refused() {
        for code in [
            "", "H", "7", "Q7", "Js7", "H4", "H12", "h01", "H7/g6", "zc8",
        ] {
            assert!(FitCode::parse(code).is_err(), "{code}");
        }
        assert!(FitCode::parse(" H7 ").unwrap().hole());
        assert!(!FitCode::parse("js6").unwrap().hole());
        assert!(deviations("H7", 0.0).is_err());
        assert!(deviations("H7", 500.5).unwrap_err().contains("500 mm"));
    }
}

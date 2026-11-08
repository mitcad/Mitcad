// SPDX-License-Identifier: MIT
//! Tests across the expression module: syntax, units, functions, the
//! parameter table, printing and .f3d examples.

use std::collections::{BTreeMap, HashMap};
use std::f64::consts::{E, PI};

use super::*;

fn ctx() -> EvalContext {
    EvalContext::default()
}

fn parse(text: &str) -> Expr {
    Expr::parse(text).unwrap_or_else(|e| panic!("{text:?}: {e}"))
}

fn try_eval(text: &str, vars: &[(&str, Quantity)]) -> Result<Quantity, EvalError> {
    let vars: HashMap<String, Quantity> = vars.iter().map(|(n, q)| ((*n).to_owned(), *q)).collect();
    parse(text).eval(&vars, &ctx())
}

fn eval(text: &str) -> Quantity {
    try_eval(text, &[]).unwrap_or_else(|e| panic!("{text:?}: {e}"))
}

/// Value of `text` as a plain number.
fn num(text: &str) -> f64 {
    let q = eval(text);
    assert_eq!(q.dims, Dims::NONE, "{text:?}");
    q.value
}

fn eval_as(text: &str, vars: &[(&str, Quantity)], unit: Unit) -> Result<Quantity, EvalError> {
    let vars: BTreeMap<String, Quantity> =
        vars.iter().map(|(n, q)| ((*n).to_owned(), *q)).collect();
    parse(text).eval_as(&vars, &ctx(), unit)
}

fn eval_err(text: &str) -> EvalError {
    try_eval(text, &[]).expect_err(text)
}

fn parse_err(text: &str) -> ParseError {
    Expr::parse(text).expect_err(text)
}

#[track_caller]
fn assert_close(actual: f64, expected: f64) {
    let tolerance = 1e-12 * actual.abs().max(expected.abs()).max(1.0);
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} is not close to {expected}"
    );
}

fn len(mm: f64) -> Quantity {
    Quantity::length(mm)
}

// ---------------------------------------------------------------- syntax

#[test]
fn numbers() {
    assert_eq!(num("1e-3"), 0.001);
    assert_eq!(num(".5"), 0.5);
    assert_eq!(num("5."), 5.0);
    assert_eq!(num("1E3"), 1000.0);
    assert_eq!(num("2.5e+2"), 250.0);
    assert_eq!(num("  42  "), 42.0);
}

#[test]
fn decimal_commas() {
    assert_eq!(num("1,5"), 1.5);
    assert_eq!(num(",5"), 0.5);
    assert_eq!(num("5,"), 5.0);
    assert_eq!(num("2,5e3"), 2500.0);
    assert_eq!(eval("1,5mm"), len(1.5));
    assert_eq!(eval("0,25 mm"), len(0.25));
    assert_eq!(eval("1,5 mm"), eval("1.5 mm"));
    let vars = [("d1", len(10.0))];
    assert_eq!(eval_as("d1 * 1,5", &vars, Unit::MM), Ok(len(15.0)));
    // Other commas separate arguments; `;` always does.
    assert_eq!(num("max(1, 5)"), 5.0);
    assert_eq!(num("max(1;5)"), 5.0);
    assert_eq!(num("max(1,5; 2)"), 2.0);
    assert_eq!(num("max(1,5, 2)"), 2.0);
    assert_eq!(num("max(1.5,5)"), 5.0);
    assert_eq!(num("max(1,-5)"), 1.0);
    assert_eq!(eval_as("max(d1,5)", &vars, Unit::MM), Ok(len(10.0)));
    // One decimal separator, no thousands separators.
    assert_eq!(num("1,000"), 1.0);
    assert!(Expr::parse("1 000").is_err());
    assert!(Expr::parse("1,5,5").is_err());
}

#[test]
fn decimal_comma_arguments_get_a_suggestion() {
    assert_eq!(
        parse_err("max(1,5)").to_string(),
        "max takes at least 2 arguments, found 1; separate them with ';' or a space \
         after the comma: “max(1; 5)” at byte 0"
    );
    let suggestion = |text: &str| match parse_err(text).kind {
        ParseErrorKind::WrongArgumentCount { suggestion, .. } => suggestion,
        kind => panic!("{text:?}: {kind:?}"),
    };
    assert_eq!(suggestion("pow(2,5)").as_deref(), Some("pow(2; 5)"));
    assert_eq!(suggestion("if(1,2,3)").as_deref(), Some("if(1; 2; 3)"));
    assert_eq!(
        suggestion("atan2(1,5 mm)").as_deref(),
        Some("atan2(1; 5 mm)")
    );
    // Only where a decimal comma could have been a separator.
    assert_eq!(suggestion("max(sin(1,5))"), None);
    assert_eq!(suggestion("pow(2)"), None);
    assert_eq!(suggestion("sin(1,5; 2)"), None);
}

#[test]
fn decimal_points_in_text() {
    assert_eq!(with_decimal_points("1,5 mm"), "1.5 mm");
    assert_eq!(
        with_decimal_points("d1 * 1,5 + max(a, b; ,5)"),
        "d1 * 1.5 + max(a, b; .5)"
    );
    assert!(matches!(
        with_decimal_points("max(1, 5)"),
        std::borrow::Cow::Borrowed("max(1, 5)")
    ));
    // Text that does not lex stays as it is.
    assert_eq!(with_decimal_points("1,5 $"), "1,5 $");
    // Printing uses points.
    assert_eq!(parse("1,5 mm").to_string(), "1.5 mm");
    assert_eq!(parse("max(1,5; 2,25)").to_string(), "max(1.5; 2.25)");
}

#[test]
fn precedence_and_associativity() {
    // `^` is right-associative.
    assert_eq!(num("2^3^2"), 512.0);
    assert_eq!(num("(2^3)^2"), 64.0);
    // Negation binds looser than `^` (the documented order).
    assert_eq!(num("-2^2"), -4.0);
    assert_eq!(num("(-2)^2"), 4.0);
    assert_eq!(num("2^-1"), 0.5);
    assert_eq!(num("-2^-2"), -0.25);
    assert_eq!(num("2 + 3 * 4"), 14.0);
    assert_eq!(num("(2 + 3) * 4"), 20.0);
    assert_eq!(num("10 - 4 - 3"), 3.0);
    assert_eq!(num("100 / 10 / 5"), 2.0);
    assert_eq!(num("2 * 3 % 4"), 2.0);
    assert_eq!(num("7 % 4 * 2"), 6.0);
    assert_eq!(num("-3 * 2"), -6.0);
    assert_eq!(num("2 * -3"), -6.0);
    assert_eq!(num("- -3"), 3.0);
    assert_eq!(num("+5"), 5.0);
    assert_eq!(num("2 * 3 ^ 2"), 18.0);
    assert_eq!(num("1 + 2 < 4"), 1.0);
    assert_eq!(num("1 < 2 == 1"), 1.0);

    let tree = parse("-2^2");
    let NodeKind::Neg(inner) = &tree.root().kind else {
        panic!("{tree:?}");
    };
    assert!(matches!(
        inner.kind,
        NodeKind::Binary {
            op: BinaryOp::Pow,
            ..
        }
    ));
    let tree = parse("2^3^2");
    let NodeKind::Binary { op, rhs, .. } = &tree.root().kind else {
        panic!("{tree:?}");
    };
    assert_eq!(*op, BinaryOp::Pow);
    assert!(matches!(
        rhs.kind,
        NodeKind::Binary {
            op: BinaryOp::Pow,
            ..
        }
    ));
}

#[test]
fn unit_suffix_binds_tightest() {
    // `-5 mm` is `-(5 mm)`.
    assert!(matches!(parse("-5 mm").root().kind, NodeKind::Neg(_)));
    // `2 ^ 3 mm` is `2 ^ (3 mm)`: an exponent with units.
    assert!(matches!(
        eval_err("2 ^ 3 mm"),
        EvalError::NotDimensionless {
            operation: "exponent",
            ..
        }
    ));
    // A suffix power belongs to the unit; parentheses square the value.
    assert_eq!(eval("10 mm^2"), Quantity::new(10.0, Dims::AREA));
    assert_eq!(eval("10 mm ^ 2"), Quantity::new(10.0, Dims::AREA));
    assert_eq!(eval("(10 mm)^2"), Quantity::new(100.0, Dims::AREA));
    assert_eq!(eval("10 mm^0.5").dims.length, Ratio::new(1, 2).unwrap());
    assert_eq!(eval("2 cm^-1").value, 0.2);
    assert_eq!(eval("10mm"), len(10.0));
}

#[track_caller]
fn assert_length(text: &str, mm: f64) {
    let q = eval(text);
    assert_eq!(q.dims, Dims::LENGTH, "{text:?}");
    assert_close(q.value, mm);
}

#[test]
fn inch_and_foot_marks_after_numbers() {
    // mitcad#1531: `"` (or `''`) right after a number is in, `'` is ft.
    assert_length(".875\"", 22.225);
    assert_length("0.875\"", 22.225);
    assert_length("2.5 \"", 63.5);
    assert_length("2''", 50.8);
    assert_length("3'", 914.4);
    assert_length("3 '", 914.4);
    // Feet and inches add up and bind as one value.
    assert_length("3' 6\"", 1066.8);
    assert_length("3'6\"", 1066.8);
    assert_length("3' 6.5''", 1079.5);
    assert_length("-3' 6\"", -1066.8);
    assert_length("2 * 3' 6\"", 2133.6);
    assert_length("3' 6\" / 2", 533.4);
    assert_length("3' + 6\"", 1066.8);
    assert_length("1\" + 1 mm", 26.4);
    // A mark takes a unit power like a unit name: two square inches.
    let area = eval("2\"^2");
    assert_eq!(area.dims, Dims::AREA);
    assert_close(area.value, 1290.32);
    // Printed with the unit names; the printing parses back.
    for (text, printed) in [
        (".875\"", "0.875 in"),
        ("3'", "3 ft"),
        ("3' 6\"", "3 ft + 6 in"),
        ("2 * 3' 6\"", "2 * (3 ft + 6 in)"),
        ("-3' 6''", "-(3 ft + 6 in)"),
    ] {
        let e = parse(text);
        assert_eq!(e.to_string(), printed, "{text:?}");
        assert_eq!(parse(printed), e, "{text:?}");
    }
    // A mark names no parameter.
    assert_eq!(parse("3' 6\" + d1").references(), ["d1"]);
    // Texts in quotes still parse where a value starts: after an operator,
    // a separator or a parenthesis, never right after a number.
    assert!(parse("'6qt'").is_constant());
    assert!(parse("\"Box\" & 'x'").is_constant());
    assert!(Expr::parse("if(1 > 2; 3'; 'a')").is_ok());
    assert!(Expr::parse("value(d1; \"in\")").is_ok());
    assert!(Expr::parse("max(5,'a')").is_ok());
    // A quote after anything but a number starts a text.
    assert_eq!(parse_err("d1\"").kind, ParseErrorKind::UnterminatedText);
    assert_eq!(parse_err("(3)'").kind, ParseErrorKind::UnterminatedText);
}

#[test]
fn equality_ignores_positions() {
    assert_eq!(parse("1+2"), parse("  1 +   2 "));
    assert_ne!(parse("1 + 2"), parse("2 + 1"));
    assert_eq!("d1 * 2".parse::<Expr>(), Ok(parse("d1*2")));
}

#[test]
fn names_are_case_sensitive() {
    // Parameter names and units are; function names and PI are not
    // (mitcad#663), and E is (`e` is a name).
    assert_eq!(num("pi"), PI);
    assert_close(num("Sin(30)"), 0.5);
    assert_eq!(parse("e").references(), ["e"]);
    let vars = [("d1", len(1.0)), ("D1", len(2.0))];
    assert_eq!(try_eval("D1 - d1", &vars), Ok(len(1.0)));
    assert_eq!(
        parse_err("2 MM").kind,
        ParseErrorKind::UnknownUnit("MM".into())
    );
}

#[test]
fn valid_names() {
    for name in ["d1", "width", "Wall_Thickness", "_x", "äänes", "x2_b"] {
        assert!(is_valid_name(name), "{name}");
    }
    for name in [
        "", "1a", "a b", "a-b", "a.b", "mm", "in", "m", "deg", "sin", "if", "PI", "E", "random",
        "Gravity", "°",
    ] {
        assert!(!is_valid_name(name), "{name}");
    }
}

#[test]
fn function_and_constant_names_are_reserved_in_any_case() {
    for name in [
        "Max", "MAX", "Sin", "IF", "pi", "Pi", "SUM", "value", "Value", "Let", "iferror", "RANDOM",
        "gravity",
    ] {
        assert!(is_reserved_name(name), "{name}");
        assert!(!is_valid_name(name), "{name}");
    }
    // Units and E only as written.
    for name in ["MM", "Deg", "IN", "e", "maximum", "sums", "values", "d1"] {
        assert!(is_valid_name(name), "{name}");
    }
}

#[test]
fn references() {
    let e = parse("a + b * a + sin(c) + ((d)) mm");
    assert_eq!(e.references(), ["a", "b", "c", "d"]);
    let spans: Vec<(&str, Span)> = e.reference_spans();
    assert_eq!(spans.len(), 5);
    // Parentheses are not part of a reference.
    assert_eq!(spans[4], ("d", Span::new(23, 24)));
    assert_eq!(spans[0], ("a", Span::new(0, 1)));
    assert_eq!(spans[2], ("a", Span::new(8, 9)));
    assert_eq!(spans[3], ("c", Span::new(16, 17)));
    assert!(parse("2 * PI * 3 mm").is_constant());
}

#[test]
fn plain_numbers_are_numbers_as_typed() {
    for text in [
        "90",
        "7.5",
        "-90",
        "+90",
        "450 deg",
        "1.5 rad",
        "-7.5 deg",
        "(-7.5) deg",
        "(90)",
    ] {
        assert!(parse(text).is_plain_number(), "{text}");
    }
    for text in [
        "d1",
        "-d1",
        "d1 + 30 deg",
        "360 deg + 90 deg",
        "2 * 90",
        "--90",
        "PI",
        "abs(-90)",
        "(d1) deg",
        "'90'",
    ] {
        assert!(!parse(text).is_plain_number(), "{text}");
    }
}

#[test]
fn lookup_implementations() {
    let e = parse("x * 2");
    let mut hash = HashMap::new();
    hash.insert("x".to_owned(), len(3.0));
    let mut tree = BTreeMap::new();
    tree.insert("x".to_owned(), len(3.0));
    let closure = |name: &str| (name == "x").then_some(len(3.0));
    for lookup in [&hash as &dyn Lookup, &tree, &closure] {
        assert_eq!(e.eval(lookup, &ctx()), Ok(len(6.0)));
    }
    assert!(matches!(
        e.eval(&(), &ctx()),
        Err(EvalError::UnknownReference { .. })
    ));
}

// ---------------------------------------------------------------- errors

#[test]
fn parse_errors_have_byte_positions() {
    let cases: [(&str, ParseErrorKind, Span); 21] = [
        ("", ParseErrorKind::Empty, Span::new(0, 0)),
        ("   ", ParseErrorKind::Empty, Span::new(0, 3)),
        (
            "1 +",
            ParseErrorKind::UnexpectedEnd {
                expected: "a value",
            },
            Span::new(3, 3),
        ),
        (
            "(1 + 2",
            ParseErrorKind::UnexpectedEnd { expected: "')'" },
            Span::new(6, 6),
        ),
        (
            "1 + 2)",
            ParseErrorKind::Unexpected {
                found: ")".into(),
                expected: "an operator or the end of the expression",
            },
            Span::new(5, 6),
        ),
        (
            "1 * / 2",
            ParseErrorKind::Unexpected {
                found: "/".into(),
                expected: "a value",
            },
            Span::new(4, 5),
        ),
        ("2 ** 3", ParseErrorKind::PowerOperator, Span::new(2, 4)),
        ("1 = 2", ParseErrorKind::SingleEquals, Span::new(2, 3)),
        ("3 $", ParseErrorKind::UnexpectedChar('$'), Span::new(2, 3)),
        (
            "5 xyz",
            ParseErrorKind::UnknownUnit("xyz".into()),
            Span::new(2, 5),
        ),
        (
            "d1 d2",
            ParseErrorKind::UnknownUnit("d2".into()),
            Span::new(3, 5),
        ),
        (
            "2 sin(3)",
            ParseErrorKind::Unexpected {
                found: "sin".into(),
                expected: "an operator",
            },
            Span::new(2, 5),
        ),
        (
            "mm * 2",
            ParseErrorKind::UnitWithoutValue("mm".into()),
            Span::new(0, 2),
        ),
        (
            "foo(1)",
            ParseErrorKind::UnknownFunction("foo".into()),
            Span::new(0, 3),
        ),
        (
            "1 + sin",
            ParseErrorKind::MissingArguments("sin".into()),
            Span::new(4, 7),
        ),
        (
            "sin(1; 2)",
            ParseErrorKind::WrongArgumentCount {
                function: "sin",
                expected: Arity::Exact(1),
                found: 2,
                suggestion: None,
            },
            Span::new(0, 9),
        ),
        (
            "max(1)",
            ParseErrorKind::WrongArgumentCount {
                function: "max",
                expected: Arity::AtLeast(2),
                found: 1,
                suggestion: None,
            },
            Span::new(0, 6),
        ),
        (
            "2 * max(1,5)",
            ParseErrorKind::WrongArgumentCount {
                function: "max",
                expected: Arity::AtLeast(2),
                found: 1,
                suggestion: Some("max(1; 5)".into()),
            },
            Span::new(4, 12),
        ),
        (
            "random()",
            ParseErrorKind::Unsupported("random".into()),
            Span::new(0, 6),
        ),
        (
            "2 * Gravity",
            ParseErrorKind::Unsupported("Gravity".into()),
            Span::new(4, 11),
        ),
        ("5 mm^0", ParseErrorKind::InvalidUnitPower, Span::new(4, 6)),
    ];
    for (text, kind, span) in cases {
        let e = parse_err(text);
        assert_eq!((&e.kind, e.span), (&kind, span), "{text:?}");
    }
    assert_eq!(parse_err("1e999").kind, ParseErrorKind::InvalidNumber);
    // Positions count bytes, not characters.
    assert_eq!(parse_err("ä + )").span, Span::new(5, 6));
}

#[test]
fn error_messages() {
    assert_eq!(parse_err("5 xyz").to_string(), "unknown unit xyz at byte 2");
    assert_eq!(
        parse_err("2 ** 3").to_string(),
        "use '^' for powers, not '**' at byte 2"
    );
    assert_eq!(
        eval_err("1 mm + 2 deg").to_string(),
        "cannot apply “+” to length and angle at byte 0"
    );
    assert_eq!(
        eval_as("d * d", &[("d", len(1.0))], Unit::MM)
            .unwrap_err()
            .to_string(),
        "expected length, found length^2"
    );
}

#[test]
fn deep_nesting_is_rejected() {
    let deep = format!("{}1{}", "(".repeat(200), ")".repeat(200));
    assert_eq!(parse_err(&deep).kind, ParseErrorKind::TooComplex);
    let long_sum = vec!["1"; 1000].join(" + ");
    assert_eq!(parse_err(&long_sum).kind, ParseErrorKind::TooComplex);
    let powers = vec!["2"; 300].join("^");
    assert_eq!(parse_err(&powers).kind, ParseErrorKind::TooComplex);
    let negations = format!("{}1", "-".repeat(300));
    assert_eq!(parse_err(&negations).kind, ParseErrorKind::TooComplex);
    // Moderate nesting is fine.
    let ok = format!("{}1{}", "(".repeat(50), ")".repeat(50));
    assert_eq!(num(&ok), 1.0);
}

#[test]
fn eval_errors_have_positions() {
    assert_eq!(
        try_eval("1 + foo * 2", &[]),
        Err(EvalError::UnknownReference {
            name: "foo".into(),
            span: Span::new(4, 7)
        })
    );
    assert_eq!(
        eval_err("1 + (2 mm + 3 deg)"),
        EvalError::DimensionMismatch {
            operation: "+",
            left: Dims::LENGTH,
            right: Dims::ANGLE,
            span: Span::new(5, 17)
        }
    );
    // Enclosing nodes cover the parentheses.
    assert!(matches!(
        eval_err("(1 mm) + (2 deg)"),
        EvalError::DimensionMismatch {
            span: Span { start: 0, end: 16 },
            ..
        }
    ));
    assert_eq!(
        eval_err("sqrt(-4)"),
        EvalError::Domain {
            operation: "sqrt",
            span: Span::new(5, 7)
        }
    );
    assert_eq!(
        eval_err("2 / (1 - 1)"),
        EvalError::DivisionByZero {
            span: Span::new(0, 11)
        }
    );
    assert_eq!(
        eval_err("10 % 0"),
        EvalError::DivisionByZero {
            span: Span::new(0, 6)
        }
    );
    assert!(matches!(eval_err("10 ^ 400"), EvalError::NotFinite { .. }));
    assert!(matches!(eval_err("1e305 km"), EvalError::NotFinite { .. }));
    assert!(matches!(
        eval_err("(-8) ^ 0.5"),
        EvalError::Domain { operation: "^", .. }
    ));
}

// ---------------------------------------------------------------- units

#[test]
fn unit_conversions_are_exact_definitions() {
    assert_eq!(eval("1 in").value, 25.4);
    assert_eq!(eval("1 ft").value, 304.8);
    assert_eq!(eval("1 yd").value, 914.4);
    assert_eq!(eval("1 mi").value, 1_609_344.0);
    assert_eq!(eval("1 mil").value, 0.0254);
    assert_eq!(eval("1 m").value, 1000.0);
    assert_eq!(eval("1 km").value, 1e6);
    assert_eq!(eval("1 cm").value, 10.0);
    assert_eq!(eval("1 um"), eval("1 micron"));
    assert_eq!(eval("1 um").value, 0.001);
    assert_eq!(eval("0.5 in").value, 12.7);
    assert_eq!(eval("1 inch"), eval("1 in"));
    assert_eq!(eval("1 foot"), eval("1 ft"));
    assert_eq!(eval("1 rad"), Quantity::angle(1.0));
    assert_eq!(eval("30 deg"), Quantity::angle(30.0 * (PI / 180.0)));
    assert_eq!(eval("30°"), eval("30 deg"));
    assert_close(eval("200 grad").value, PI);
    assert_eq!(len(25.4).in_unit(Unit::IN), Ok(1.0));
    assert_eq!(len(304.8).in_unit(Unit::FT), Ok(1.0));
    assert_close(eval("180 deg").in_unit(Unit::RAD).unwrap(), PI);
    assert_eq!(num("12 in == 1 ft"), 1.0);
    assert_eq!(num("1 in == 2.54 cm"), 1.0);
}

#[test]
fn unit_arithmetic() {
    assert_close(eval("10 mm + 1 in").value, 35.4);
    assert_close(eval("1 in + 1 mm").value, 26.4);
    assert_close(eval("1 ft - 12 in").value, 0.0);
    assert_eq!(eval("2 * 1 in"), len(50.8));
    assert_close(eval("( 13 / 3 ) * 1 mm").value, 13.0 / 3.0);
    assert_eq!(
        try_eval("(a + 2) mm", &[("a", Quantity::unitless(3.0))]),
        Ok(len(5.0))
    );
    assert_eq!(eval("10 mm * 2 mm"), Quantity::new(20.0, Dims::AREA));
    assert_eq!(eval("10 mm / 2 mm"), Quantity::unitless(5.0));
    assert_eq!(
        eval("1 / 2 mm"),
        Quantity::new(0.5, Dims::new(Ratio::integer(-1), Ratio::ZERO))
    );
    assert_eq!(eval("(2 mm)^3"), Quantity::new(8.0, Dims::VOLUME));
    assert_eq!(eval("(4 mm^2)^0.5"), len(2.0));
    assert_eq!(eval("sqrt(4 mm * 9 mm)"), len(6.0));
    let root = eval("sqrt(4 mm)");
    assert_eq!(root.dims.length, Ratio::new(1, 2).unwrap());
    assert_eq!(eval("sqrt(4 mm) * sqrt(4 mm)"), len(4.0));
    assert_eq!(eval("(8 mm^3)^(1/3)").dims, Dims::LENGTH);
    assert!(matches!(
        eval_err("(2 mm)^PI"),
        EvalError::InvalidExponent { .. }
    ));
    assert_eq!(eval("30 deg * 2"), eval("60 deg"));
    assert_eq!(eval("10 mm % 3 mm"), len(1.0));
    assert_eq!(eval("-10 mm % 3 mm"), len(-1.0));
    assert_eq!(
        eval("10 mm * 1 deg").dims,
        Dims::new(Ratio::ONE, Ratio::ONE)
    );
}

#[test]
fn dimension_mismatches() {
    for text in [
        "10 mm + 30 deg",
        "10 mm - 1 mm^2",
        "1 mm < 1 deg",
        "1 mm % 1 deg",
        "min(1 mm; 1 deg)",
        "max(1 mm; 2; 1 deg)",
        "atan2(1 mm; 1 deg)",
        "if(1; 1 mm; 1 deg)",
    ] {
        assert!(
            matches!(eval_err(text), EvalError::DimensionMismatch { .. }),
            "{text}"
        );
    }
    let vars = [("d1", len(10.0)), ("d2", len(2.0))];
    assert!(matches!(
        try_eval("d1 + d1 * d2", &vars),
        Err(EvalError::DimensionMismatch {
            left: Dims::LENGTH,
            right: Dims::AREA,
            ..
        })
    ));
}

#[test]
fn bare_numbers_take_the_default_unit() {
    let vars = [("d1", len(10.0)), ("d2", len(4.0))];
    // A plain result is in the parameter's unit.
    assert_eq!(eval_as("10", &[], Unit::MM), Ok(len(10.0)));
    assert_eq!(eval_as("10 / 2", &[], Unit::MM), Ok(len(5.0)));
    assert_eq!(eval_as("10 * 10", &[], Unit::MM), Ok(len(100.0)));
    assert_eq!(eval_as("10", &[], Unit::IN), Ok(len(254.0)));
    assert_eq!(eval_as("30", &[], Unit::DEG), Ok(eval("30 deg")));
    assert_eq!(eval_as("2", &[], Unit::RAD), Ok(Quantity::angle(2.0)));
    assert_eq!(
        eval_as("100", &[], Unit::parse("mm^2").unwrap()),
        Ok(Quantity::new(100.0, Dims::AREA))
    );
    // Scale factors.
    assert_eq!(eval_as("d1 * 2", &vars, Unit::MM), Ok(len(20.0)));
    assert_eq!(eval_as("d1 / 2", &vars, Unit::MM), Ok(len(5.0)));
    assert_eq!(eval_as("2 * d1", &vars, Unit::MM), Ok(len(20.0)));
    // Promotion next to a length, in the parameter's unit.
    assert_eq!(eval_as("d1 + 5", &vars, Unit::MM), Ok(len(15.0)));
    assert_eq!(eval_as("5 + d1", &vars, Unit::MM), Ok(len(15.0)));
    assert_eq!(eval_as("(d1 + 5) / 2", &vars, Unit::MM), Ok(len(7.5)));
    assert_eq!(eval_as("d1 + 5", &vars, Unit::IN), Ok(len(10.0 + 127.0)));
    assert_close(
        eval_as("30 deg + 5", &[], Unit::DEG).unwrap().value,
        eval("35 deg").value,
    );
    // Ratios.
    assert_eq!(
        eval_as("d1 / d2", &vars, Unit::NONE),
        Ok(Quantity::unitless(2.5))
    );
    assert_eq!(eval_as("d1 / d2", &vars, Unit::MM), Ok(len(2.5)));
    assert_eq!(eval_as("d1 / d2 * 1 mm", &vars, Unit::MM), Ok(len(2.5)));
    // Wrong dimensions.
    assert_eq!(
        eval_as("d1 * d2", &vars, Unit::MM),
        Err(EvalError::WrongDimension {
            expected: Dims::LENGTH,
            found: Dims::AREA
        })
    );
    // A unitless value that is a length or an angle says how to write a
    // number (mitcad#662).
    assert_eq!(
        eval_as("5 mm", &[], Unit::NONE),
        Err(EvalError::NotUnitless {
            found: Dims::LENGTH,
            hint: "the result is a length (5 mm); for a count write `5`".to_owned()
        })
    );
    assert_eq!(
        eval_as("d1 / 3", &[("d1", len(15.0))], Unit::NONE)
            .unwrap_err()
            .to_string(),
        "the result is a length (5 mm); for a count write `d1 / 3 mm`"
    );
    assert_eq!(
        eval_as("d1 / 3 mm", &[("d1", len(15.0))], Unit::NONE),
        Ok(Quantity::unitless(5.0))
    );
    assert_eq!(
        eval_as("d1 * 2", &[("d1", len(15.0))], Unit::NONE)
            .unwrap_err()
            .to_string(),
        "the result is a length (30 mm); for a count write `d1 * 2 / 1 mm`"
    );
    assert_eq!(
        eval_as("90 deg / 3", &[], Unit::NONE)
            .unwrap_err()
            .to_string(),
        "the result is an angle (30 deg); for a count write `90 deg / 3 deg`"
    );
    assert!(matches!(
        eval_as("d1 * d1", &[("d1", len(15.0))], Unit::NONE),
        Err(EvalError::WrongDimension { .. })
    ));
    assert!(eval_as("30 deg", &[], Unit::MM).is_err());
    // The document's unit applies when the parameter's unit has no length.
    let inch_doc = EvalContext::new(LengthUnit::Inch, AngleUnit::Degree);
    let e = parse("(d1 + 1) / d1");
    let lookup = |name: &str| (name == "d1").then_some(len(25.4));
    assert_eq!(
        e.eval_as(&lookup, &inch_doc, Unit::NONE),
        Ok(Quantity::unitless(2.0))
    );
    assert_close(
        e.eval_as(&lookup, &ctx(), Unit::NONE).unwrap().value,
        26.4 / 25.4,
    );
    // The parameter's own unit wins over the document's.
    assert_eq!(parse("10").eval_as(&(), &inch_doc, Unit::MM), Ok(len(10.0)));
    assert_eq!(
        parse("10").eval_as(&(), &inch_doc, Unit::NONE),
        Ok(Quantity::unitless(10.0))
    );
}

#[test]
fn for_unit_context() {
    let doc = EvalContext::default();
    assert_eq!(doc.for_unit(Unit::IN).default_length_unit, LengthUnit::Inch);
    assert_eq!(doc.for_unit(Unit::IN).default_angle_unit, AngleUnit::Degree);
    assert_eq!(
        doc.for_unit(Unit::RAD).default_angle_unit,
        AngleUnit::Radian
    );
    assert_eq!(doc.for_unit(Unit::NONE), doc);
    let inch = EvalContext::new(LengthUnit::Inch, AngleUnit::Degree);
    assert_eq!(inch.default_factor(Dims::AREA), 25.4 * 25.4);
    assert_eq!(inch.default_factor(Dims::NONE), 1.0);
}

// ---------------------------------------------------------------- functions

#[test]
fn trigonometry_uses_degrees_for_plain_numbers() {
    assert_close(num("sin(30)"), 0.5);
    assert_close(num("sin(30 deg)"), 0.5);
    assert_close(num("sin((PI / 6) rad)"), 0.5);
    assert_close(num("sin(PI rad / 6)"), 0.5);
    // The unit binds to the 6: `PI / (6 rad)` is not an angle.
    assert!(matches!(
        eval_err("sin(PI / 6 rad)"),
        EvalError::NotAngle { .. }
    ));
    assert_close(num("cos(60)"), 0.5);
    assert_close(num("tan(45)"), 1.0);
    // A plain argument is in degrees, even PI / 2.
    assert_close(num("sin(PI / 2)"), (PI / 2.0).to_radians().sin());
    let rad = EvalContext::new(LengthUnit::Millimetre, AngleUnit::Radian);
    assert_close(parse("sin(PI / 2)").eval(&(), &rad).unwrap().value, 1.0);
    assert!(matches!(
        eval_err("sin(10 mm)"),
        EvalError::NotAngle {
            function: "sin",
            found: Dims::LENGTH,
            ..
        }
    ));
    // Inverse functions return angles.
    assert_close(eval("asin(0.5)").in_unit(Unit::DEG).unwrap(), 30.0);
    assert_close(
        eval_as("acos(0.5)", &[], Unit::DEG).unwrap().value,
        PI / 3.0,
    );
    assert_close(eval("atan(1)").in_unit(Unit::DEG).unwrap(), 45.0);
    assert_eq!(eval("asin(1.0000000000001)"), Quantity::angle(PI / 2.0));
    assert!(matches!(eval_err("acos(2)"), EvalError::Domain { .. }));
    assert!(matches!(
        eval_err("asin(1 mm)"),
        EvalError::NotDimensionless { .. }
    ));
    assert_close(eval("atan2(1; 1)").in_unit(Unit::DEG).unwrap(), 45.0);
    assert_close(
        eval("atan2(1 in; 25.4 mm)").in_unit(Unit::DEG).unwrap(),
        45.0,
    );
    assert_close(eval("atan2(-1 mm; 0)").in_unit(Unit::DEG).unwrap(), -90.0);
    let vars = [("h", len(10.0)), ("w", len(10.0))];
    assert_close(
        eval_as("atan(h / w)", &vars, Unit::DEG).unwrap().value,
        PI / 4.0,
    );
    assert_close(eval_as("w * sin(30)", &vars, Unit::MM).unwrap().value, 5.0);
}

#[test]
fn hyperbolic_functions_take_unitless_arguments_in_degrees() {
    assert_eq!(num("sinh(0)"), 0.0);
    assert_eq!(num("cosh(0)"), 1.0);
    assert_close(num("tanh(1 rad)"), 1f64.tanh());
    assert_close(num("sinh(1)"), 1f64.to_radians().sinh());
    assert_eq!(eval("asinh(0)"), Quantity::angle(0.0));
    assert_eq!(eval("acosh(1)"), Quantity::angle(0.0));
    assert_close(eval("atanh(0.5)").value, 0.5f64.atanh());
    assert!(matches!(eval_err("acosh(0.5)"), EvalError::Domain { .. }));
    assert!(matches!(eval_err("atanh(1)"), EvalError::Domain { .. }));
}

#[test]
fn math_functions() {
    assert_eq!(num("sqrt(16)"), 4.0);
    assert_eq!(num("abs(-3)"), 3.0);
    assert_eq!(eval("abs(-3 mm)"), len(3.0));
    assert_close(num("exp(1)"), E);
    assert_close(num("ln(E)"), 1.0);
    assert_close(num("log(1000)"), 3.0);
    assert!(matches!(
        eval_err("ln(0)"),
        EvalError::Domain {
            operation: "ln",
            ..
        }
    ));
    assert!(matches!(eval_err("log(-1)"), EvalError::Domain { .. }));
    assert!(matches!(
        eval_err("exp(1 mm)"),
        EvalError::NotDimensionless {
            operation: "exp",
            ..
        }
    ));
    assert_eq!(num("pow(2; 10)"), 1024.0);
    assert_eq!(eval("pow(3 mm; 2)"), Quantity::new(9.0, Dims::AREA));
    assert_eq!(num("PI"), PI);
    assert_eq!(num("E"), E);
    assert_close(eval("2 * PI * 10 mm").value, 20.0 * PI);
}

#[test]
fn sign_is_zero_below_zero_and_one_from_zero_on() {
    assert_eq!(num("sign(-2)"), 0.0);
    assert_eq!(num("sign(0)"), 1.0);
    assert_eq!(num("sign(3)"), 1.0);
    assert_eq!(eval("sign(-5 mm)"), Quantity::unitless(0.0));
}

#[test]
fn rounding_functions() {
    assert_eq!(num("floor(2.7)"), 2.0);
    assert_eq!(num("ceil(2.1)"), 3.0);
    assert_eq!(num("round(2.5)"), 3.0);
    assert_eq!(num("round(-2.5)"), -3.0);
    assert_eq!(num("floor(-2.5)"), -3.0);
    assert_eq!(eval("round(12.7 mm)"), len(13.0));
    assert_eq!(eval("floor(1.5 in)"), len(38.0));
    assert_close(eval("round(29.6 deg)").in_unit(Unit::DEG).unwrap(), 30.0);
    // Rounding happens in the default unit of the dimension.
    let inch = EvalContext::new(LengthUnit::Inch, AngleUnit::Degree);
    assert_eq!(parse("round(12.7 mm)").eval(&(), &inch), Ok(len(25.4)));
    assert_eq!(
        parse("ceil(1 mm^2)").eval(&(), &inch),
        Ok(Quantity::new(25.4 * 25.4, Dims::AREA))
    );
}

#[test]
fn min_and_max() {
    assert_eq!(num("min(3; 1; 2)"), 1.0);
    assert_eq!(num("max(1, 2)"), 2.0);
    assert_eq!(eval("max(1 mm; 2)"), len(2.0));
    assert_eq!(eval("max(1 in; 30 mm)"), len(30.0));
    assert_eq!(eval("min(1 in; 30 mm; 2)"), len(2.0));
}

#[test]
fn comparisons_and_logic() {
    assert_eq!(num("1 in == 25.4 mm"), 1.0);
    assert_eq!(num("0.1 + 0.2 == 0.3"), 1.0);
    assert_eq!(num("1 <> 2"), 1.0);
    assert_eq!(num("1 != 1"), 0.0);
    assert_eq!(num("2 >= 2"), 1.0);
    assert_eq!(num("2 > 2"), 0.0);
    assert_eq!(num("2 <= 1"), 0.0);
    assert_eq!(num("0.3 < 0.1 + 0.2"), 0.0);
    assert_eq!(num("1 mm < 2"), 1.0);
    assert_eq!(num("and(1; 1; 0)"), 0.0);
    assert_eq!(num("and(1; 2)"), 1.0);
    assert_eq!(num("or(0; 1)"), 1.0);
    assert_eq!(num("or(0)"), 0.0);
    assert_eq!(num("not(0)"), 1.0);
    assert_eq!(num("not(3)"), 0.0);
    assert!(matches!(
        eval_err("and(1 mm)"),
        EvalError::NotDimensionless {
            operation: "and",
            ..
        }
    ));
}

#[test]
fn if_function() {
    assert_eq!(eval("if(1 > 2; 10 mm; 20 mm)"), len(20.0));
    assert_eq!(eval("if(1 < 2; 10; 20 mm)"), len(10.0));
    let vars = [("d1", len(10.0))];
    assert_eq!(eval_as("if(d1 > 5; d1; 5)", &vars, Unit::MM), Ok(len(10.0)));
    assert_eq!(eval_as("if(d1 > 50; d1; 5)", &vars, Unit::MM), Ok(len(5.0)));
    // A value error in the branch not taken is ignored.
    assert_eq!(num("if(0; 1 / 0; 5)"), 5.0);
    assert_eq!(num("if(1; 2; sqrt(-1))"), 2.0);
    assert!(matches!(
        eval_err("if(1; 1 / 0; 5)"),
        EvalError::DivisionByZero { .. }
    ));
    // Structural errors are not.
    assert!(matches!(
        eval_err("if(1; 2; nothing)"),
        EvalError::UnknownReference { .. }
    ));
    assert!(matches!(
        eval_err("if(1 mm; 1; 2)"),
        EvalError::NotDimensionless {
            operation: "if",
            ..
        }
    ));
}

// ---------------------------------------------------------------- printing

#[test]
fn canonical_printing() {
    let cases = [
        ("( 13 / 3 ) * 1 mm", "13 / 3 * 1 mm"),
        ("d1*2+5mm", "d1 * 2 + 5 mm"),
        ("2^3^2", "2 ^ 3 ^ 2"),
        ("(2^3)^2", "(2 ^ 3) ^ 2"),
        ("-2^2", "-2 ^ 2"),
        ("(-2)^2", "(-2) ^ 2"),
        ("2^-x", "2 ^ -x"),
        ("a-(b-c)", "a - (b - c)"),
        ("(a-b)-c", "a - b - c"),
        ("a/(b*c)", "a / (b * c)"),
        ("a*(b/c)", "a * (b / c)"),
        ("(a*b)/c", "a * b / c"),
        ("(10 mm)^2", "(10 mm) ^ 2"),
        ("10 mm^2", "10 mm^2"),
        ("10mm^-1", "10 mm^-1"),
        ("(a + 2) mm", "(a + 2) mm"),
        ("(-5) mm", "(-5) mm"),
        ("max(a,2)", "max(a; 2)"),
        ("max(1, 2)", "max(1; 2)"),
        ("if(a>b;1;2)", "if(a > b; 1; 2)"),
        ("1 != 2", "1 <> 2"),
        ("(a < b) + 1", "(a < b) + 1"),
        ("- - x", "-(-x)"),
        ("-(a*b)", "-(a * b)"),
        ("2 * -3", "2 * -3"),
        ("+5", "5"),
        ("0.0 deg", "0 deg"),
        ("360.0 deg", "360 deg"),
        ("1e-3", "0.001"),
        ("1e20", "1e20"),
        ("30°", "30 deg"),
        ("1 inch", "1 in"),
        ("2 * PI * r", "2 * PI * r"),
        ("sqrt(x)^2", "sqrt(x) ^ 2"),
    ];
    for (input, expected) in cases {
        let e = parse(input);
        let printed = e.to_string();
        assert_eq!(printed, expected, "{input:?}");
        assert_eq!(parse(&printed), e, "{input:?}");
        assert_eq!(parse(&printed).to_string(), printed, "{input:?}");
    }
}

/// Small deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let r = self.0.wrapping_mul(0x2545_f491_4f6c_dd1d);
        (r >> 33) as usize % n
    }

    fn pick<T: Clone>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())].clone()
    }
}

fn random_node(rng: &mut Rng, depth: usize) -> Node {
    let kind = if depth == 0 || rng.below(4) == 0 {
        match rng.below(3) {
            0 => NodeKind::Number(rng.pick(&[0.0, 1.0, 2.5, 0.001, 1e-7, 1e20, 13.0, 360.0])),
            1 => NodeKind::Reference(rng.pick(&["d1", "width", "Wall_Thickness", "x"]).to_owned()),
            _ => NodeKind::Constant(rng.pick(&[Constant::Pi, Constant::E])),
        }
    } else {
        let child = |rng: &mut Rng| Box::new(random_node(rng, depth - 1));
        match rng.below(6) {
            0 => NodeKind::Neg(child(rng)),
            1 => NodeKind::WithUnit {
                value: child(rng),
                unit: rng.pick(&[
                    Unit::MM,
                    Unit::IN,
                    Unit::DEG,
                    Unit::MM.powi(2).unwrap(),
                    Unit::CM.powi(-1).unwrap(),
                ]),
            },
            2..=4 => NodeKind::Binary {
                op: rng.pick(&[
                    BinaryOp::Add,
                    BinaryOp::Sub,
                    BinaryOp::Mul,
                    BinaryOp::Div,
                    BinaryOp::Rem,
                    BinaryOp::Pow,
                    BinaryOp::Lt,
                    BinaryOp::Le,
                    BinaryOp::Gt,
                    BinaryOp::Ge,
                    BinaryOp::Eq,
                    BinaryOp::Ne,
                ]),
                lhs: child(rng),
                rhs: child(rng),
            },
            _ => {
                let function = rng.pick(&[
                    Function::Sin,
                    Function::Atan2,
                    Function::Sqrt,
                    Function::Max,
                    Function::If,
                    Function::And,
                ]);
                let count = match function.arity() {
                    Arity::Exact(n) => n,
                    Arity::AtLeast(n) => n + rng.below(2),
                    Arity::Range(min, max) => min + rng.below(max - min + 1),
                };
                NodeKind::Call {
                    function,
                    args: (0..count).map(|_| random_node(rng, depth - 1)).collect(),
                }
            }
        }
    };
    Node::new(kind, Span::default())
}

#[test]
fn printing_round_trips_random_trees() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..3000 {
        let e = Expr::from_node(random_node(&mut rng, 5));
        let text = e.to_string();
        let parsed = Expr::parse(&text).unwrap_or_else(|err| panic!("{text}: {err}"));
        assert_eq!(parsed, e, "{text}");
        assert_eq!(parsed.to_string(), text);
    }
}

#[test]
fn formatting_values() {
    assert_eq!(format_value(12.5, Unit::MM, 3), "12.5 mm");
    assert_eq!(format_value(eval("30 deg").value, Unit::DEG, 2), "30 deg");
    assert_eq!(eval("12.5 mm").to_string(), "12.5 mm");
    assert_eq!(eval("30 deg").to_string(), "30 deg");
    assert_eq!(eval("1 / 3").to_string(), "0.333333");
    assert_eq!(format_value(eval("1 in").value, Unit::IN, 4), "1 in");
    assert_eq!(format_number(2.0 / 3.0, 3), "0.667");
    // Values written as expressions evaluate back exactly.
    for (value, unit) in [(12.5, Unit::MM), (1.0 / 3.0, Unit::IN), (0.7, Unit::DEG)] {
        let canonical = unit.to_canonical(value);
        let text = value_to_expression(canonical, unit);
        assert_eq!(
            evaluate(&text, &(), &ctx(), unit).unwrap().value,
            canonical,
            "{text}"
        );
    }
}

// ---------------------------------------------------------------- table

fn spec(name: &str, expression: &str, unit: Unit) -> ParamSpec {
    ParamSpec::user(name, expression, unit)
}

fn names(table: &ParameterTable, ids: impl IntoIterator<Item = ParamId>) -> Vec<String> {
    ids.into_iter()
        .map(|id| table.get(id).unwrap().name().to_owned())
        .collect()
}

fn sample_table() -> ParameterTable {
    ParameterTable::build(
        ctx(),
        [
            spec("width", "50 mm", Unit::MM),
            spec("height", "width / 2", Unit::MM),
            spec("area", "width * height", Unit::parse("mm^2").unwrap()),
            spec("other", "7", Unit::MM),
        ],
    )
    .unwrap()
}

#[test]
fn table_evaluates_in_dependency_order() {
    let t = sample_table();
    let area = t.find("area").unwrap();
    assert_eq!(t.value(area), 1250.0);
    assert_eq!(t.quantity(area).dims, Dims::AREA);
    assert_eq!(t.value(t.find("other").unwrap()), 7.0);
    let order = names(&t, t.evaluation_order());
    assert_eq!(order, ["width", "height", "area", "other"]);

    // Forward references are fine when loading.
    let t = ParameterTable::build(
        ctx(),
        [
            spec("a", "b * 2", Unit::MM),
            spec("b", "c + 1 mm", Unit::MM),
            spec("c", "3", Unit::MM),
        ],
    )
    .unwrap();
    assert_eq!(t.value(t.find("a").unwrap()), 8.0);
    assert_eq!(names(&t, t.evaluation_order()), ["c", "b", "a"]);
    // Iteration stays in creation order.
    let created: Vec<&str> = t.iter().map(|(_, p)| p.name()).collect();
    assert_eq!(created, ["a", "b", "c"]);
}

#[test]
fn table_reports_unknown_names() {
    let mut t = sample_table();
    let error = t.add(spec("x", "width + depth", Unit::MM)).unwrap_err();
    assert_eq!(
        error,
        TableError::Eval {
            parameter: "x".into(),
            error: EvalError::UnknownReference {
                name: "depth".into(),
                span: Span::new(8, 13)
            }
        }
    );
    assert_eq!(
        error.to_string(),
        "parameter x: unknown parameter depth at byte 8"
    );
    assert!(t.find("x").is_none());
    let error = ParameterTable::build(ctx(), [spec("a", "b", Unit::MM)]).unwrap_err();
    assert!(matches!(error, TableError::Eval { .. }));
}

#[test]
fn table_change_re_evaluates_dependents() {
    let mut t = sample_table();
    let width = t.find("width").unwrap();
    let changed = t.set_expression(width, "60 mm").unwrap();
    assert_eq!(names(&t, changed), ["width", "height", "area"]);
    assert_eq!(t.value(t.find("area").unwrap()), 1800.0);
    assert_eq!(t.get(width).unwrap().expression(), "60 mm");

    // Same value, different text: nothing changed.
    assert!(t.set_expression(width, "30 mm * 2").unwrap().is_empty());
    assert_eq!(t.get(width).unwrap().expression(), "30 mm * 2");

    // A change that only affects one parameter.
    let height = t.find("height").unwrap();
    let changed = t.set_expression(height, "10").unwrap();
    assert_eq!(names(&t, changed), ["height", "area"]);
    assert_eq!(t.value(t.find("area").unwrap()), 600.0);

    // A new reference to a later parameter.
    let other = t.find("other").unwrap();
    let changed = t.set_expression(width, "other * 2").unwrap();
    assert_eq!(names(&t, changed), ["width", "area"]);
    assert_eq!(t.value(width), 14.0);
    assert_eq!(
        names(&t, t.evaluation_order()),
        ["other", "width", "height", "area"]
    );
    let changed = t.set_expression(other, "1 in").unwrap();
    assert_eq!(names(&t, changed), ["width", "area", "other"]);
    assert_eq!(t.value(width), 50.8);
    assert_eq!(t.dependents(other), [width]);
    assert_eq!(t.get(width).unwrap().dependencies(), [other]);
}

#[test]
fn table_detects_cycles() {
    let mut t = ParameterTable::build(
        ctx(),
        [
            spec("a", "1", Unit::MM),
            spec("b", "a", Unit::MM),
            spec("c", "b + a", Unit::MM),
        ],
    )
    .unwrap();
    let a = t.find("a").unwrap();
    let before = t.clone();
    assert_eq!(
        t.set_expression(a, "c + 1"),
        Err(TableError::Cycle {
            path: vec!["a".into(), "c".into(), "b".into(), "a".into()]
        })
    );
    assert_eq!(
        t.set_expression(a, "a + 1"),
        Err(TableError::Cycle {
            path: vec!["a".into(), "a".into()]
        })
    );
    // Nothing changed.
    assert_eq!(t.get(a), before.get(a));
    assert_eq!(t.get(a).unwrap().expression(), "1");
    assert_eq!(
        t.add(spec("x", "x * 2", Unit::MM)),
        Err(TableError::Cycle {
            path: vec!["x".into(), "x".into()]
        })
    );
    let error = ParameterTable::build(
        ctx(),
        [
            spec("p", "q", Unit::MM),
            spec("q", "r", Unit::MM),
            spec("r", "p + 1", Unit::MM),
            spec("s", "1", Unit::MM),
        ],
    )
    .unwrap_err();
    assert_eq!(
        error,
        TableError::Cycle {
            path: vec!["p".into(), "q".into(), "r".into(), "p".into()]
        }
    );
    assert_eq!(error.to_string(), "circular reference: p -> q -> r -> p");
}

#[test]
fn table_refuses_unit_errors_in_dependents() {
    let mut t = ParameterTable::build(
        ctx(),
        [spec("L", "10 mm", Unit::MM), spec("A", "L * 2", Unit::MM)],
    )
    .unwrap();
    let l = t.find("L").unwrap();
    let error = t.update(l, "30 deg", Unit::DEG).unwrap_err();
    assert_eq!(
        error,
        TableError::Eval {
            parameter: "A".into(),
            error: EvalError::WrongDimension {
                expected: Dims::LENGTH,
                found: Dims::ANGLE
            }
        }
    );
    assert_eq!(t.get(l).unwrap().unit(), Unit::MM);
    assert_eq!(t.value(l), 10.0);
    assert!(matches!(
        t.set_expression(l, "1 +"),
        Err(TableError::Parse { .. })
    ));
    assert!(matches!(
        t.set_expression(l, "1 mm * 1 mm"),
        Err(TableError::Eval {
            error: EvalError::WrongDimension { .. },
            ..
        })
    ));
}

#[test]
fn table_unit_and_value_edits() {
    let mut t = ParameterTable::new(ctx());
    let p = t.add(spec("p", "10", Unit::MM)).unwrap();
    let q = t.add(spec("q", "p + 1 mm", Unit::MM)).unwrap();
    assert_eq!(t.value(p), 10.0);
    // Bare numbers follow the parameter's unit.
    let changed = t.set_unit(p, Unit::IN).unwrap();
    assert_eq!(changed, ChangeSet::from([p, q]));
    assert_eq!(t.value(p), 254.0);
    assert_eq!(t.get(p).unwrap().value_in_unit(), 10.0);
    assert_eq!(t.get(p).unwrap().formatted_value(3), "10 in");
    t.set_value(p, 12.7).unwrap();
    assert_eq!(t.get(p).unwrap().expression(), "0.5 in");
    assert_eq!(t.value(q), 13.7);
    let angle = t.add(spec("angle", "30", Unit::DEG)).unwrap();
    t.set_value(angle, PI / 4.0).unwrap();
    assert_eq!(t.get(angle).unwrap().expression(), "45 deg");
    assert_eq!(t.get(angle).unwrap().formatted_value(2), "45 deg");
}

#[test]
fn table_keeps_decimal_points() {
    let mut t = ParameterTable::build(ctx(), [spec("w", "0,5 in", Unit::MM)]).unwrap();
    let w = t.find("w").unwrap();
    assert_eq!(t.get(w).unwrap().expression(), "0.5 in");
    let h = t.add(spec("h", "w * 1,5 + max(w, 2,5)", Unit::MM)).unwrap();
    assert_eq!(t.get(h).unwrap().expression(), "w * 1.5 + max(w, 2.5)");
    assert_close(t.value(h), 12.7 * 1.5 + 12.7);
    t.set_expression(w, "1,5").unwrap();
    assert_eq!(t.get(w).unwrap().expression(), "1.5");
    // Errors refer to the text as written.
    let error = t.set_expression(w, "max(1,5)").unwrap_err();
    assert!(error.to_string().contains("“max(1; 5)”"), "{error}");
    assert_eq!(t.value(h), 1.5 * 1.5 + 2.5);
    // A rename rewrites the stored text, with its points.
    t.rename(w, "width").unwrap();
    assert_eq!(
        t.get(h).unwrap().expression(),
        "width * 1.5 + max(width, 2.5)"
    );
}

#[test]
fn table_rename_rewrites_references() {
    let mut t = ParameterTable::build(
        ctx(),
        [
            spec("w", "10 mm", Unit::MM),
            spec("ww", "( w*2 )+w", Unit::MM),
            spec("x", "max(w;ww) mm / 1 mm", Unit::MM),
            spec("y", "ww", Unit::MM),
        ],
    )
    .unwrap();
    let w = t.find("w").unwrap();
    let rewritten = t.rename(w, "Wall_Thickness").unwrap();
    assert_eq!(names(&t, rewritten), ["ww", "x"]);
    assert_eq!(t.get(w).unwrap().name(), "Wall_Thickness");
    assert_eq!(t.find("w"), None);
    assert_eq!(t.find("Wall_Thickness"), Some(w));
    let ww = t.find("ww").unwrap();
    assert_eq!(
        t.get(ww).unwrap().expression(),
        "( Wall_Thickness*2 )+Wall_Thickness"
    );
    assert_eq!(
        t.get(t.find("x").unwrap()).unwrap().expression(),
        "max(Wall_Thickness;ww) mm / 1 mm"
    );
    assert_eq!(t.get(t.find("y").unwrap()).unwrap().expression(), "ww");
    // Re-evaluation still works under the new name.
    let changed = t.set_expression(w, "1 mm").unwrap();
    assert_eq!(names(&t, changed), ["Wall_Thickness", "ww", "x", "y"]);
    assert_eq!(t.value(ww), 3.0);

    assert_eq!(t.rename(w, "Wall_Thickness"), Ok(Vec::new()));
    assert_eq!(
        t.rename(w, "ww"),
        Err(TableError::DuplicateName("ww".into()))
    );
    assert_eq!(
        t.rename(w, "mm"),
        Err(TableError::ReservedName("mm".into()))
    );
    assert_eq!(
        t.rename(w, "sin"),
        Err(TableError::ReservedName("sin".into()))
    );
    assert_eq!(t.rename(w, "1a"), Err(TableError::InvalidName("1a".into())));
    assert_eq!(t.rename(w, ""), Err(TableError::InvalidName(String::new())));
}

#[test]
fn table_remove_checks_references() {
    let mut t = sample_table();
    let width = t.find("width").unwrap();
    assert_eq!(
        t.remove(width),
        Err(TableError::InUse {
            name: "width".into(),
            used_by: vec!["height".into(), "area".into()]
        })
    );
    let area = t.find("area").unwrap();
    let removed = t.remove(area).unwrap();
    assert_eq!(removed.name(), "area");
    assert_eq!(t.find("area"), None);
    assert!(t.get(area).is_none());
    assert_eq!(t.len(), 3);
    // Ids of the others are stable, and a new id is not reused.
    assert_eq!(t.find("width"), Some(width));
    let new = t.add(spec("area", "1", Unit::MM)).unwrap();
    assert!(new > area);
    assert_eq!(t.remove(area).unwrap_err(), TableError::UnknownId(area));
}

#[test]
fn table_names_and_metadata() {
    let mut t = ParameterTable::new(ctx());
    assert!(t.is_empty());
    assert_eq!(t.unused_name("d"), "d1");
    let d1 = t
        .add(
            ParamSpec::model("d1", "( 13 / 3 ) * 1 mm", Unit::MM)
                .with_comment("Linear Dimension-2"),
        )
        .unwrap();
    assert_eq!(t.unused_name("d"), "d2");
    let p = t.get(d1).unwrap();
    assert_eq!(p.kind(), ParamKind::Model);
    assert_eq!(p.comment(), "Linear Dimension-2");
    assert_eq!(p.id(), d1);
    assert_eq!(p.expr(), &parse("13 / 3 * 1 mm"));
    t.set_comment(d1, "new").unwrap();
    assert_eq!(t.get(d1).unwrap().comment(), "new");
    assert_eq!(
        t.add(spec("d1", "1", Unit::MM)),
        Err(TableError::DuplicateName("d1".into()))
    );
    assert_eq!(
        t.add(spec("in", "1", Unit::MM)),
        Err(TableError::ReservedName("in".into()))
    );
    assert_eq!(
        t.add(spec("a b", "1", Unit::MM)),
        Err(TableError::InvalidName("a b".into()))
    );
    let from_value = ParamSpec::from_value(ParamKind::User, "v", 12.5, Unit::MM);
    assert_eq!(from_value.expression, "12.5 mm");
    t.add(from_value).unwrap();
    assert_eq!(t.len(), 2);
    assert_eq!(ParamKind::User.to_string(), "user");
}

#[test]
fn table_unused_dimension_names_follow_adds_renames_and_removals() {
    // The first free name, wherever names were taken or freed (the search
    // starts from a hint, mitcad#413).
    let mut t = ParameterTable::new(ctx());
    let add = |t: &mut ParameterTable| {
        let name = t.unused_name("d");
        t.add(spec(&name, "1 mm", Unit::MM)).unwrap()
    };
    let ids: Vec<ParamId> = (0..5).map(|_| add(&mut t)).collect();
    assert_eq!(names(&t, ids.clone()), ["d1", "d2", "d3", "d4", "d5"]);
    assert_eq!(t.unused_name("d"), "d6");
    t.remove(ids[2]).unwrap();
    assert_eq!(t.unused_name("d"), "d3");
    t.rename(ids[0], "first").unwrap();
    assert_eq!(t.unused_name("d"), "d1");
    // Taken by hand, out of turn: the next one is skipped too.
    t.add(spec("d1", "1 mm", Unit::MM)).unwrap();
    assert_eq!(t.unused_name("d"), "d3");
    t.add(spec("d7", "1 mm", Unit::MM)).unwrap();
    t.add(spec("d3", "1 mm", Unit::MM)).unwrap();
    assert_eq!(t.unused_name("d"), "d6");
    t.rename(ids[1], "d6").unwrap();
    assert_eq!(t.unused_name("d"), "d2");
    // Another prefix searches from the start.
    assert_eq!(t.unused_name("x"), "x1");
}

#[test]
fn table_evaluates_free_expressions() {
    let t = sample_table();
    assert_eq!(t.evaluate("width * 2", Unit::MM), Ok(len(100.0)));
    assert_eq!(t.evaluate("height + 5", Unit::MM), Ok(len(30.0)));
    assert!(matches!(
        t.evaluate("nothing", Unit::MM),
        Err(ExprError::Eval(EvalError::UnknownReference { .. }))
    ));
    assert!(matches!(
        t.evaluate("1 +", Unit::MM),
        Err(ExprError::Parse(_))
    ));
    // The table is a lookup for other expressions.
    assert_eq!(parse("area / width").eval(&t, &ctx()), Ok(len(25.0)));
}

#[test]
fn table_context_change() {
    let mut t = ParameterTable::build(
        ctx(),
        [
            spec("d", "10 mm", Unit::MM),
            spec("ratio", "(d + 5) / d", Unit::NONE),
            spec("e", "d * 2", Unit::MM),
        ],
    )
    .unwrap();
    let ratio = t.find("ratio").unwrap();
    assert_eq!(t.value(ratio), 1.5);
    let changed = t
        .set_context(EvalContext::new(LengthUnit::Inch, AngleUnit::Degree))
        .unwrap();
    assert_eq!(changed, ChangeSet::from([ratio]));
    assert_eq!(t.value(ratio), 13.7);
    assert_eq!(t.context().default_length_unit, LengthUnit::Inch);
}

#[test]
fn table_with_saved_ids() {
    let t = ParameterTable::build_with_ids(
        ctx(),
        [
            (ParamId::from_raw(3), spec("a", "b", Unit::MM)),
            (ParamId::from_raw(7), spec("b", "2", Unit::MM)),
        ],
    )
    .unwrap();
    assert_eq!(t.find("a"), Some(ParamId::from_raw(3)));
    assert_eq!(t.value(ParamId::from_raw(3)), 2.0);
    let mut t = t;
    let c = t.add(spec("c", "a", Unit::MM)).unwrap();
    assert_eq!(c.raw(), 8);
    let error = ParameterTable::build_with_ids(
        ctx(),
        [
            (ParamId::from_raw(3), spec("a", "1", Unit::MM)),
            (ParamId::from_raw(3), spec("b", "2", Unit::MM)),
        ],
    )
    .unwrap_err();
    assert_eq!(error, TableError::IdOrder(ParamId::from_raw(3)));
}

// ---------------------------------------------------------------- .f3d

#[test]
fn expressions_from_f3d() {
    // Expression texts as stored in .f3d files.
    assert_close(
        evaluate("( 13 / 3 ) * 1 mm", &(), &ctx(), Unit::MM)
            .unwrap()
            .value,
        13.0 / 3.0,
    );
    assert_eq!(evaluate("6 mm", &(), &ctx(), Unit::MM), Ok(len(6.0)));
    assert_eq!(
        evaluate("0.0 deg", &(), &ctx(), Unit::DEG),
        Ok(Quantity::angle(0.0))
    );
    assert_close(
        evaluate("30 deg", &(), &ctx(), Unit::DEG).unwrap().value,
        PI / 6.0,
    );
    assert_close(
        evaluate("360.0 deg", &(), &ctx(), Unit::DEG).unwrap().value,
        2.0 * PI,
    );
    // Model parameters as an .f3d design would have them.
    let t = ParameterTable::build(
        f3d::context(LengthUnit::Millimetre),
        [
            ParamSpec::model("d1", "( 13 / 3 ) * 1 mm", Unit::MM),
            ParamSpec::model("d2", "6 mm", Unit::MM),
            ParamSpec::model("d3", "d1 * 2 + d2", Unit::MM),
            ParamSpec::model("d4", "30 deg", Unit::DEG),
            ParamSpec::model("d5", "360.0 deg / 6", Unit::DEG),
            ParamSpec::user("Wall_Thickness", "d2 / 2", Unit::MM),
            ParamSpec::user("count", "4", Unit::NONE),
        ],
    )
    .unwrap();
    assert_close(t.value(t.find("d3").unwrap()), 26.0 / 3.0 + 6.0);
    assert_close(t.value(t.find("d5").unwrap()), PI / 3.0);
    assert_eq!(t.value(t.find("Wall_Thickness").unwrap()), 3.0);
    assert_eq!(
        t.quantity(t.find("count").unwrap()),
        Quantity::unitless(4.0)
    );
}

#[test]
fn f3d_internal_units() {
    assert_eq!(f3d::internal_to_canonical(0.6, Dims::LENGTH), 6.0);
    assert_eq!(f3d::internal_to_canonical(1.0, Dims::AREA), 100.0);
    assert_eq!(f3d::internal_to_canonical(1.0, Dims::VOLUME), 1000.0);
    assert_eq!(f3d::internal_to_canonical(0.5, Dims::ANGLE), 0.5);
    assert_eq!(f3d::internal_to_canonical(3.0, Dims::NONE), 3.0);
    assert_eq!(f3d::canonical_to_internal(6.0, Dims::LENGTH), 0.6);
    assert_close(
        f3d::internal_quantity(2.54, Dims::LENGTH)
            .in_unit(Unit::IN)
            .unwrap(),
        1.0,
    );
    assert_eq!(f3d::parse_unit(""), Ok(Unit::NONE));
    assert_eq!(f3d::parse_unit("cm^3").unwrap().dims(), Dims::VOLUME);
    assert_eq!(f3d::parse_expression(" 6 mm\u{a0}").unwrap(), parse("6 mm"));
    let checks = [
        ("( 13 / 3 ) * 1 mm", "mm", 13.0 / 30.0),
        ("6 mm", "mm", 0.6),
        ("30 deg", "deg", PI / 6.0),
        ("360.0 deg", "deg", 2.0 * PI),
        ("1 in", "in", 2.54),
        ("4", "", 4.0),
    ];
    for (expression, unit, internal) in checks {
        let (q, agrees) =
            f3d::check_parameter(expression, unit, internal, &(), &ctx(), 1e-12).unwrap();
        assert!(agrees, "{expression}: {q}");
    }
    let (_, agrees) = f3d::check_parameter("6 mm", "mm", 0.7, &(), &ctx(), 1e-12).unwrap();
    assert!(!agrees);
    assert!(matches!(
        f3d::check_parameter("6 mm", "xx", 0.6, &(), &ctx(), 1e-12),
        Err(f3d::CheckError::Unit(_))
    ));
}

// ------------------------------------- spreadsheet-style formulas (mitcad#663)

#[test]
fn existing_expressions_read_as_before() {
    // The functions of .f3d expressions keep their names and printing;
    // only more spellings are accepted.
    for name in [
        "sin", "atan2", "sqrt", "sign", "floor", "ceil", "round", "min", "max", "pow", "if", "and",
        "or", "not",
    ] {
        assert_eq!(Function::from_name(name).map(Function::name), Some(name));
    }
    for (text, printed) in [
        ("floor(d1)", "floor(d1)"),
        ("Round(x)", "round(x)"),
        ("MAX(a, b)", "max(a; b)"),
        ("a == b", "a == b"),
        ("max(1,5; 2)", "max(1.5; 2)"),
    ] {
        assert_eq!(parse(text).to_string(), printed, "{text}");
    }
}

#[test]
fn single_equals_compares_inside_parentheses() {
    assert_eq!(num("if(2 = 2; 10; 20)"), 10.0);
    assert_eq!(num("IF(2 = 3; 10; 20)"), 20.0);
    assert_eq!(num("(1 = 1) + 1"), 2.0);
    assert_eq!(num("not(1 = 2)"), 1.0);
    assert_eq!(num("(1 == 1)"), 1.0);
    let vars = [
        ("GridX", Quantity::unitless(6.0)),
        ("a", len(1.0)),
        ("b", len(2.0)),
    ];
    assert_eq!(
        eval_as("IF(GridX = 6; a; b)", &vars, Unit::MM),
        Ok(len(1.0))
    );
    // The same comparison as `==`, and printed as it.
    assert_eq!(parse("if(a = b; 1; 2)"), parse("if(a == b; 1; 2)"));
    assert_eq!(parse("if(a = b; 1; 2)").to_string(), "if(a == b; 1; 2)");
    // At the top level a single `=` is still an error: `name = expression`
    // is split off before parsing (mitcad#231; the model's
    // `split_named_keeps_comparisons_in_the_expression` tests the split).
    assert_eq!(parse_err("a = 1").kind, ParseErrorKind::SingleEquals);
    assert_eq!(parse_err("(a) = 1").kind, ParseErrorKind::SingleEquals);
}

#[test]
fn rounding_to_digits_and_steps() {
    // Decimals of the default unit of the value's dimension.
    assert_close(num("round(2.345; 2)"), 2.35);
    assert_close(num("ROUND(1.005; 2)"), 1.01);
    assert_close(num("Round(1234.5; -2)"), 1200.0);
    assert_close(num("roundup(0.1 + 0.2; 1)"), 0.3);
    assert_eq!(num("ROUNDUP(2.1)"), 3.0);
    assert_eq!(num("roundup(-2.1)"), -3.0);
    assert_eq!(num("ROUNDDOWN(-2.9)"), -2.0);
    assert_close(num("rounddown(2.97; 1)"), 2.9);
    assert_eq!(num("TRUNC(-2.7)"), -2.0);
    assert_close(num("trunc(2.789; 2)"), 2.78);
    assert_eq!(num("INT(2.9)"), 2.0);
    assert_eq!(num("int(-2.5)"), -3.0);
    assert_close(eval("round(12.345 mm; 1)").value, 12.3);
    assert_eq!(eval("round(12.345 mm; 1)").dims, Dims::LENGTH);
    let inch = EvalContext::new(LengthUnit::Inch, AngleUnit::Degree);
    assert_close(
        parse("round(10 mm; 2)").eval(&(), &inch).unwrap().value,
        0.39 * 25.4,
    );
    // A step: a multiple of it, in the value's dimension.
    assert_eq!(eval("FLOOR(12.7 mm; 0.5 mm)"), len(12.5));
    assert_eq!(eval("ceiling(12.1 mm; 0.5 mm)"), len(12.5));
    assert_eq!(eval("floor(12.7 mm; 0.5)"), len(12.5));
    let vars = [("d1", len(15.3))];
    assert_eq!(eval_as("FLOOR(d1; 0.5 mm)", &vars, Unit::MM), Ok(len(15.0)));
    assert_close(
        parse("floor(1 in; 0.3)").eval(&(), &inch).unwrap().value,
        3.0 * 0.3 * 25.4,
    );
    assert_eq!(eval("mround(13 mm; 5 mm)"), len(15.0));
    assert_eq!(eval("MROUND(12.4 mm; 5 mm)"), len(10.0));
    assert_eq!(eval("mround(12.5 mm; 5 mm)"), len(15.0));
    assert_eq!(num("CEILING(2.1)"), 3.0);
    assert_eq!(num("ceiling(-2.5; 1)"), -2.0);
    // The sign of the step does not matter.
    assert_eq!(num("floor(7; -2)"), 6.0);
    // Floating point noise does not cross a step.
    assert_close(num("ceiling(0.3; 0.1)"), 0.3);
    // With one argument as before.
    assert_eq!(num("floor(2.9999999999999996)"), 2.0);
    // Errors.
    assert!(matches!(
        eval_err("floor(1 mm; 1 deg)"),
        EvalError::DimensionMismatch { .. }
    ));
    assert!(matches!(
        eval_err("FLOOR(5; 0)"),
        EvalError::DivisionByZero { .. }
    ));
    assert!(matches!(
        eval_err("round(1; 1 mm)"),
        EvalError::NotDimensionless {
            operation: "round",
            ..
        }
    ));
    assert_eq!(
        parse_err("ROUND(1; 2; 3)").kind,
        ParseErrorKind::WrongArgumentCount {
            function: "round",
            expected: Arity::Range(1, 2),
            found: 3,
            suggestion: None,
        }
    );
    assert_eq!(
        parse_err("round(1; 2; 3)").to_string(),
        "round takes 1 to 2 arguments, found 3 at byte 0"
    );
    assert!(matches!(
        parse_err("mround(1)").kind,
        ParseErrorKind::WrongArgumentCount {
            function: "mround",
            ..
        }
    ));
}

#[test]
fn integer_functions() {
    // `mod` has the sign of the divisor; `%` keeps the dividend's.
    assert_eq!(num("mod(10; 3)"), 1.0);
    assert_eq!(num("MOD(-10; 3)"), 2.0);
    assert_eq!(num("mod(10; -3)"), -2.0);
    assert_eq!(num("-10 % 3"), -1.0);
    assert_eq!(eval("mod(10 mm; 3 mm)"), len(1.0));
    assert_eq!(eval("mod(10 mm; 3)"), len(1.0));
    assert!(matches!(
        eval_err("mod(1; 0)"),
        EvalError::DivisionByZero { .. }
    ));
    assert!(matches!(
        eval_err("mod(1 mm; 1 deg)"),
        EvalError::DimensionMismatch { .. }
    ));
    assert_eq!(num("quotient(10; 3)"), 3.0);
    assert_eq!(num("QUOTIENT(-10; 3)"), -3.0);
    assert_eq!(eval("quotient(10 mm; 3 mm)"), Quantity::unitless(3.0));
    assert_eq!(eval("quotient(10 mm; 3)"), len(3.0));
    assert!(matches!(
        eval_err("quotient(1; 0)"),
        EvalError::DivisionByZero { .. }
    ));
    for (text, expected) in [
        ("even(1.5)", 2.0),
        ("EVEN(3)", 4.0),
        ("even(2)", 2.0),
        ("even(-1)", -2.0),
        ("even(0)", 0.0),
        ("odd(1.5)", 3.0),
        ("ODD(2)", 3.0),
        ("odd(1)", 1.0),
        ("odd(0)", 1.0),
        ("odd(-2)", -3.0),
    ] {
        assert_eq!(num(text), expected, "{text}");
    }
    assert_eq!(eval("even(3 mm)"), len(4.0));
}

#[test]
fn aggregates() {
    assert_eq!(num("SUM(1; 2; 3)"), 6.0);
    assert_eq!(num("sum(5)"), 5.0);
    assert_close(eval("sum(1 mm; 2; 1 in)").value, 28.4);
    assert_eq!(eval("sum(1 mm; 2; 1 in)").dims, Dims::LENGTH);
    assert_eq!(num("average(1; 2; 3; 4)"), 2.5);
    assert_eq!(eval("AVERAGE(10 mm; 20 mm)"), len(15.0));
    assert_eq!(num("median(3; 1; 2)"), 2.0);
    assert_eq!(num("MEDIAN(4; 1; 3; 2)"), 2.5);
    assert_eq!(eval("median(1 mm; 5 mm; 2 mm)"), len(2.0));
    assert_eq!(num("product(2; 3; 4)"), 24.0);
    assert_eq!(eval("PRODUCT(2 mm; 3 mm)"), Quantity::new(6.0, Dims::AREA));
    assert_eq!(eval("product(2 mm; 3)"), len(6.0));
    assert!(matches!(
        eval_err("sum(1 mm; 1 deg)"),
        EvalError::DimensionMismatch {
            operation: "sum",
            ..
        }
    ));
    assert!(matches!(
        parse_err("sum()").kind,
        ParseErrorKind::WrongArgumentCount {
            function: "sum",
            found: 0,
            ..
        }
    ));
}

#[test]
fn clamp_function() {
    assert_eq!(num("CLAMP(5; 1; 3)"), 3.0);
    assert_eq!(num("clamp(-1; 1; 3)"), 1.0);
    assert_eq!(num("clamp(2; 1; 3)"), 2.0);
    let vars = [("d", len(10.0))];
    assert_eq!(
        eval_as("clamp(d; 2 mm; 8 mm)", &vars, Unit::MM),
        Ok(len(8.0))
    );
    assert_eq!(eval("clamp(5 mm; 1; 3)"), len(3.0));
    assert!(matches!(
        eval_err("clamp(1; 3; 1)"),
        EvalError::Domain {
            operation: "clamp",
            ..
        }
    ));
    assert!(matches!(
        eval_err("clamp(1 mm; 1 deg; 2)"),
        EvalError::DimensionMismatch { .. }
    ));
}

#[test]
fn ifs_switch_choose_and_iferror() {
    assert_eq!(num("IFS(0; 1; 1; 2)"), 2.0);
    assert_eq!(eval("ifs(1 > 2; 10 mm; 1 < 2; 20 mm)"), len(20.0));
    let vars = [("x", Quantity::unitless(15.0))];
    assert_eq!(
        eval_as("ifs(x < 10; 1; x < 20; 2; 1; 3)", &vars, Unit::NONE),
        Ok(Quantity::unitless(2.0))
    );
    assert!(matches!(
        eval_err("ifs(0; 1)"),
        EvalError::NoMatch {
            function: "ifs",
            ..
        }
    ));
    // As with `if`, a value error counts only in the branch taken, and the
    // branches agree in dimensions.
    assert_eq!(num("ifs(1; 5; 1; 1 / 0)"), 5.0);
    assert!(matches!(
        eval_err("ifs(1; 1 / 0; 1; 5)"),
        EvalError::DivisionByZero { .. }
    ));
    assert!(matches!(
        eval_err("ifs(1; 1 mm; 0; 1 deg)"),
        EvalError::DimensionMismatch { .. }
    ));
    assert!(matches!(
        eval_err("ifs(1 mm; 1; 1; 2)"),
        EvalError::NotDimensionless {
            operation: "ifs",
            ..
        }
    ));
    assert_eq!(
        parse_err("ifs(1; 2; 3)").kind,
        ParseErrorKind::UnpairedArguments {
            function: "ifs",
            found: 3
        }
    );
    // `switch`: the value after the first equal key, else the default.
    assert_eq!(num("SWITCH(2; 1; 10; 2; 20; 30)"), 20.0);
    assert_eq!(num("switch(5; 1; 10; 2; 20; 30)"), 30.0);
    assert!(matches!(
        eval_err("switch(5; 1; 10; 2; 20)"),
        EvalError::NoMatch {
            function: "switch",
            ..
        }
    ));
    assert_eq!(num("switch(1 in; 25.4 mm; 1; 2)"), 1.0);
    assert!(matches!(
        eval_err("switch(1 mm; 1 deg; 1; 2)"),
        EvalError::DimensionMismatch { .. }
    ));
    // `choose`: counting from 1, a fraction cut off.
    assert_eq!(num("CHOOSE(2; 10; 20; 30)"), 20.0);
    assert_eq!(num("choose(2.9; 10; 20; 30)"), 20.0);
    assert_eq!(eval("choose(1; 5 mm; 7)"), len(5.0));
    assert_eq!(num("choose(1; 5; 1 / 0)"), 5.0);
    for text in ["choose(0; 1; 2)", "choose(4; 1; 2; 3)"] {
        assert!(
            matches!(
                eval_err(text),
                EvalError::Domain {
                    operation: "choose",
                    ..
                }
            ),
            "{text}"
        );
    }
    // `iferror`: a value error gives the fallback; other errors stay.
    assert_eq!(num("IFERROR(1 / 0; 5)"), 5.0);
    assert_eq!(num("iferror(4; 5)"), 4.0);
    assert_eq!(eval("iferror(sqrt(-1); 2 mm)"), len(2.0));
    assert_eq!(num("iferror(ifs(0; 1); 7)"), 7.0);
    assert_eq!(num("iferror(choose(9; 1; 2); 7)"), 7.0);
    assert!(matches!(
        eval_err("iferror(nothing; 1)"),
        EvalError::UnknownReference { .. }
    ));
    assert!(matches!(
        eval_err("iferror(1 mm + 1 deg; 1)"),
        EvalError::DimensionMismatch { .. }
    ));
    assert!(matches!(
        eval_err("iferror(1 mm; 1 deg)"),
        EvalError::DimensionMismatch { .. }
    ));
}

#[test]
fn let_names_values_inside_one_formula() {
    assert_eq!(num("let(x; 2; x * 3)"), 6.0);
    assert_eq!(num("LET(a; 2; b; a + 1; a * b)"), 6.0);
    let vars = [("d1", len(10.0))];
    assert_close(
        eval_as(
            "let(r; d1 / 2; PI * r ^ 2)",
            &vars,
            Unit::parse("mm^2").unwrap(),
        )
        .unwrap()
        .value,
        25.0 * PI,
    );
    // Nested: an inner name may hide an outer one.
    assert_eq!(num("let(x; 1; let(y; x + 1; x + y))"), 3.0);
    assert_eq!(num("let(x; 1; let(x; 5; x) + x)"), 6.0);
    // The names are not parameter references.
    let e = parse("let(x; d1; x * 2)");
    assert_eq!(e.references(), ["d1"]);
    assert_eq!(e.to_string(), "let(x; d1; x * 2)");
    assert_eq!(parse(&e.to_string()), e);
    assert_eq!(parse("LET(x;1;x+1)").to_string(), "let(x; 1; x + 1)");
    // A name holds from after its value on, and only inside the `let`.
    assert!(matches!(
        try_eval("let(x; 1; x) + x", &[]),
        Err(EvalError::UnknownReference { .. })
    ));
    assert!(matches!(
        try_eval("let(x; x + 1; x)", &[]),
        Err(EvalError::UnknownReference { .. })
    ));
    // A name must not hide a parameter.
    assert_eq!(
        try_eval("let(d1; 2; d1)", &vars),
        Err(EvalError::LocalShadowsParameter {
            name: "d1".into(),
            span: Span::new(4, 6)
        })
    );
    // Errors in the names and the pairs.
    let cases = [
        (
            "let(sin; 1; sin)",
            ParseErrorKind::ReservedLocalName("sin".into()),
        ),
        (
            "let(Max; 1; 2)",
            ParseErrorKind::ReservedLocalName("Max".into()),
        ),
        (
            "let(x; 1; x; 2; x)",
            ParseErrorKind::DuplicateLocalName("x".into()),
        ),
        (
            "let(x; 1; y; 2)",
            ParseErrorKind::UnpairedArguments {
                function: "let",
                found: 4,
            },
        ),
        (
            "let(2; 1; 3)",
            ParseErrorKind::Unexpected {
                found: "2".into(),
                expected: "a name",
            },
        ),
    ];
    for (text, kind) in cases {
        assert_eq!(parse_err(text).kind, kind, "{text}");
    }
    assert!(matches!(
        parse_err("let(x; 1)").kind,
        ParseErrorKind::WrongArgumentCount {
            function: "let",
            ..
        }
    ));
    // In the parameter table only real references are dependencies.
    let t = ParameterTable::build(
        ctx(),
        [
            spec("width", "10 mm", Unit::MM),
            spec("height", "4 mm", Unit::MM),
            spec(
                "area",
                "let(h; height / 2; width * h)",
                Unit::parse("mm^2").unwrap(),
            ),
        ],
    )
    .unwrap();
    let area = t.find("area").unwrap();
    assert_eq!(t.value(area), 20.0);
    assert_eq!(
        names(&t, t.get(area).unwrap().dependencies().to_vec()),
        ["height", "width"]
    );
}

#[test]
fn value_gives_the_number_in_a_unit() {
    let vars = [("d1", len(15.0))];
    assert_eq!(
        eval_as("VALUE(d1; \"mm\")", &vars, Unit::NONE),
        Ok(Quantity::unitless(15.0))
    );
    assert_close(
        eval_as("value(d1; \"cm\")", &vars, Unit::NONE)
            .unwrap()
            .value,
        1.5,
    );
    assert_close(num("value(1 in; \"mm\")"), 25.4);
    assert_close(num("value(90 deg; \"rad\")"), PI / 2.0);
    assert_close(num("value(10 mm^2; \"mm^2\")"), 10.0);
    assert_eq!(
        try_eval("value(d1; 1 mm)", &vars),
        Ok(Quantity::unitless(15.0))
    );
    // A plain number is read in the default unit first.
    assert_close(num("value(5; \"in\")"), 5.0 / 25.4);
    assert!(matches!(
        eval_err("value(1 mm; \"deg\")"),
        EvalError::DimensionMismatch { .. }
    ));
    assert!(matches!(
        eval_err("value(1; 0 mm)"),
        EvalError::DivisionByZero { .. }
    ));
    let e = parse("VALUE(d1;\"mm\")");
    assert_eq!(e.to_string(), "value(d1; \"mm\")");
    assert_eq!(parse(&e.to_string()), e);
    assert_eq!(e.references(), ["d1"]);
    // Errors in the unit text are at their place.
    let cases = [
        (
            "value(1; \"xyz\")",
            ParseErrorKind::UnknownUnit("xyz".into()),
            Span::new(10, 13),
        ),
        (
            "value(1; \"\")",
            ParseErrorKind::UnknownUnit(String::new()),
            Span::new(9, 11),
        ),
        (
            "value(1; \"mm",
            ParseErrorKind::UnterminatedText,
            Span::new(9, 12),
        ),
    ];
    for (text, kind, span) in cases {
        let e = parse_err(text);
        assert_eq!((&e.kind, e.span), (&kind, span), "{text}");
    }
}

#[test]
fn radians_degrees_and_pi() {
    assert_eq!(num("PI()"), PI);
    assert_eq!(num("pi()"), PI);
    assert_eq!(parse("PI()").to_string(), "PI");
    assert_eq!(parse("2 * Pi() * r").to_string(), "2 * PI * r");
    assert_eq!(
        parse_err("PI(1)").kind,
        ParseErrorKind::UnknownFunction("PI".into())
    );
    // Both give an angle: a plain number is in degrees for `radians` and in
    // radians for `degrees`, so `sin(radians(30))` is 0.5.
    assert_close(eval("RADIANS(180)").value, PI);
    assert_eq!(eval("radians(180)").dims, Dims::ANGLE);
    assert_close(num("sin(RADIANS(30))"), 0.5);
    assert_close(eval("degrees(PI)").in_unit(Unit::DEG).unwrap(), 180.0);
    assert_eq!(eval("radians(30 deg)"), eval("30 deg"));
    assert_eq!(eval("DEGREES(1 rad)"), Quantity::angle(1.0));
    assert_close(num("value(radians(180); \"rad\")"), PI);
    assert_close(
        eval_as("degrees(PI / 2)", &[], Unit::DEG).unwrap().value,
        PI / 2.0,
    );
    assert!(matches!(
        eval_err("radians(1 mm)"),
        EvalError::NotAngle {
            function: "radians",
            ..
        }
    ));
}

#[test]
fn renaming_names_in_older_expressions() {
    // For files whose parameter names became reserved: by tokens, as the
    // text no longer parses.
    let renamed = HashMap::from([("Max".to_owned(), "Max_1".to_owned())]);
    assert_eq!(
        rename_names("Max * 2 + max(Max; 1) + 2e3 + Maxi", &renamed),
        "Max_1 * 2 + max(Max_1; 1) + 2e3 + Maxi"
    );
    assert_eq!(rename_names("1 + $", &renamed), "1 + $");
    // Units are never renamed, as names or in the place of a unit.
    let renamed = HashMap::from([
        ("Max".to_owned(), "Max_1".to_owned()),
        ("mm".to_owned(), "mm_1".to_owned()),
    ]);
    assert_eq!(
        rename_names("Max * 5 mm + (Max + 1) mm + 2 Max", &renamed),
        "Max_1 * 5 mm + (Max_1 + 1) mm + 2 Max"
    );
    // Only names reserved since mitcad#663 are renamed in such files.
    for name in ["Max", "pi", "Pi", "sum", "Value", "SIN", "Random"] {
        assert!(is_newly_reserved(name), "{name}");
    }
    for name in [
        "mm", "MM", "in", "sin", "max", "PI", "E", "e", "random", "d1",
    ] {
        assert!(!is_newly_reserved(name), "{name}");
    }
}

// --------------------------------------------------------- texts (mitcad#664)

/// Parameters with numbers and texts.
struct Params(Vec<(&'static str, Value)>);

impl Lookup for Params {
    fn lookup(&self, name: &str) -> Option<Quantity> {
        self.0
            .iter()
            .find(|(n, _)| *n == name)
            .and_then(|(_, v)| v.as_number())
    }

    fn lookup_text(&self, name: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(n, _)| *n == name)
            .and_then(|(_, v)| v.as_text().map(str::to_owned))
    }
}

fn params() -> Params {
    Params(vec![
        ("GridX", Value::Number(Quantity::unitless(6.0))),
        ("GridY", Value::Number(Quantity::unitless(4.0))),
        ("Width", Value::Number(len(15.0))),
        ("Angle", Value::Number(Quantity::from_unit(30.0, Unit::DEG))),
        ("Label", Value::Text("6qt".to_owned())),
    ])
}

/// The text of `expression` with [`params`].
fn txt(expression: &str) -> String {
    parse(expression)
        .eval_text(&params(), &ctx())
        .unwrap_or_else(|e| panic!("{expression:?}: {e}"))
}

/// The number of `expression` with [`params`].
fn num_with(expression: &str) -> f64 {
    parse(expression)
        .eval(&params(), &ctx())
        .unwrap_or_else(|e| panic!("{expression:?}: {e}"))
        .value
}

fn value_err(expression: &str) -> EvalError {
    parse(expression)
        .eval_value(&params(), &ctx())
        .expect_err(expression)
}

#[test]
fn text_literals_and_their_quotes() {
    assert_eq!(txt("'6qt'"), "6qt");
    assert_eq!(txt("\"Box\""), "Box");
    // A quote of the same kind is doubled; the other kind is a character.
    assert_eq!(txt("'It''s'"), "It's");
    assert_eq!(txt("\"say \"\"hi\"\"\""), "say \"hi\"");
    assert_eq!(txt("\"It's\""), "It's");
    assert_eq!(txt("''"), "");
    assert_eq!(txt("'  spaced  '"), "  spaced  ");
    // Printed in single quotes; the printing parses back.
    let e = parse("\"It's\" & 'x'");
    assert_eq!(e.to_string(), "'It''s' & 'x'");
    assert_eq!(parse(&e.to_string()), e);
    assert_eq!(parse_err("'open").kind, ParseErrorKind::UnterminatedText);
    // A text names no parameter.
    assert!(parse("'GridX'").is_constant());
    assert_eq!(
        parse("Label & 'Label'").references(),
        ["Label"],
        "only the name outside the quotes"
    );
}

#[test]
fn ampersand_joins_texts_and_numbers() {
    assert_eq!(txt("\"Box \" & GridX & \"x\" & GridY"), "Box 6x4");
    assert_eq!(txt("Label & ' lid'"), "6qt lid");
    // Numbers as `text(x)` writes them: in the default unit of their
    // dimension, at most six decimals.
    assert_eq!(txt("'W' & Width"), "W15 mm");
    assert_eq!(txt("'' & Angle"), "30 deg");
    assert_eq!(txt("'' & 1 / 3"), "0.333333");
    // Below `+` and `-`, above comparisons.
    assert_eq!(txt("'n = ' & GridX + 1"), "n = 7");
    assert_eq!(parse("'a' & b + 1").to_string(), "'a' & b + 1");
    assert_eq!(parse("('a' & b) + 1").to_string(), "('a' & b) + 1");
    assert_eq!(num_with("('a' & 'b') == 'ab'"), 1.0);
    // An inch document writes lengths in inches.
    let inch = EvalContext::new(LengthUnit::Inch, AngleUnit::Degree);
    let text = parse("'' & Width").eval_text(&params(), &inch).unwrap();
    assert_eq!(text, "0.590551 in");
}

#[test]
fn text_functions() {
    assert_eq!(txt("CONCAT('Box '; GridX; 'x'; GridY)"), "Box 6x4");
    assert_eq!(txt("concat(Label)"), "6qt");
    assert_eq!(txt("TEXT(GridX)"), "6");
    assert_eq!(txt("text(Width)"), "15 mm");
    assert_eq!(txt("text(Width; \"0.00\")"), "15.00 mm");
    assert_eq!(txt("text(Width; '0.0 in')"), "0.6 in");
    assert_eq!(txt("text(Width; 'cm')"), "1.5 cm");
    assert_eq!(txt("text(value(Width; 'mm'); '000')"), "015");
    assert_eq!(txt("text(2.5; '0')"), "3");
    assert_eq!(txt("text(-2.5; '0')"), "-3");
    assert_eq!(txt("text(1.5; '0.0#')"), "1.5");
    assert_eq!(txt("text(1.234; '0.0#')"), "1.23");
    assert_eq!(txt("text(-0.001; '0.0')"), "0.0");
    // A unitless value is read in the format's unit.
    assert_eq!(txt("text(5; '0 mm')"), "5 mm");
    assert_eq!(txt("LEFT('Hello'; 2)"), "He");
    assert_eq!(txt("left('Hello')"), "H");
    assert_eq!(txt("right('Hello'; 3)"), "llo");
    assert_eq!(txt("Right('Hello'; 10)"), "Hello");
    assert_eq!(txt("left('äöü'; 2)"), "äö");
    assert_eq!(txt("left('Hello'; 2.9)"), "He");
    assert_eq!(txt("left(Label; 0)"), "");
    assert_eq!(num_with("LEN(Label)"), 3.0);
    assert_eq!(num_with("len('äöü') + 1"), 4.0);
    assert!(matches!(
        value_err("left('a'; -1)"),
        EvalError::Domain {
            operation: "left",
            ..
        }
    ));
    for format in ["'abc'", "'0.0.0'", "'Text'"] {
        let text = format!("text(Width; {format})");
        assert!(
            matches!(value_err(&text), EvalError::InvalidFormat { .. }),
            "{text}"
        );
    }
    assert!(matches!(
        value_err("text(Width; '0.0 deg')"),
        EvalError::DimensionMismatch { .. }
    ));
    assert!(matches!(
        parse_err("left()").kind,
        ParseErrorKind::WrongArgumentCount { .. }
    ));
}

#[test]
fn texts_and_numbers_do_not_mix() {
    let e = value_err("Label + 1");
    assert_eq!(
        e,
        EvalError::NotNumber {
            span: Span::new(0, 5)
        }
    );
    assert_eq!(e.to_string(), "text where a number is expected at byte 0");
    assert!(matches!(value_err("'5' * 2"), EvalError::NotNumber { .. }));
    assert!(matches!(
        value_err("sin(Label)"),
        EvalError::NotNumber { .. }
    ));
    assert!(matches!(
        value_err("max(Label; 1)"),
        EvalError::NotNumber { .. }
    ));
    assert!(matches!(value_err("-Label"), EvalError::NotNumber { .. }));
    assert!(matches!(value_err("Label mm"), EvalError::NotNumber { .. }));
    assert!(matches!(value_err("left(5; 1)"), EvalError::NotText { .. }));
    assert!(matches!(value_err("len(GridX)"), EvalError::NotText { .. }));
    assert!(matches!(
        value_err("text(Label)"),
        EvalError::NotNumber { .. }
    ));
    assert!(matches!(
        value_err("text(Width; 2)"),
        EvalError::NotText { .. }
    ));
    assert!(matches!(
        value_err("Label < 'a'"),
        EvalError::NotNumber { .. }
    ));
    assert!(matches!(
        value_err("Label == 1"),
        EvalError::MixedValues { .. }
    ));
    assert!(matches!(
        value_err("if(GridX > 1; 'a'; 2)"),
        EvalError::MixedValues { .. }
    ));
    assert!(matches!(
        value_err("if(Label; 1; 2)"),
        EvalError::NotNumber { .. }
    ));
    // A number parameter with a text expression; a text one with a number.
    let e = parse("'6qt'")
        .eval_as(&params(), &ctx(), Unit::MM)
        .unwrap_err();
    assert!(matches!(e, EvalError::NotNumber { .. }), "{e}");
    let e = parse("GridX * 2").eval_text(&params(), &ctx()).unwrap_err();
    assert!(
        e.to_string()
            .starts_with("a number where text is expected: make text of it with text(…) or &"),
        "{e}"
    );
    // A text unit is no unit for numbers.
    assert!(parse("5").eval_as(&params(), &ctx(), Unit::TEXT).is_err());
}

#[test]
fn choices_and_comparisons_of_texts() {
    assert_eq!(txt("if(GridX = 6; 'six'; 'other')"), "six");
    assert_eq!(
        txt("IFS(GridX > 10; 'big'; GridX > 3; 'medium'; 1; 'small')"),
        "medium"
    );
    assert_eq!(
        txt("switch(Label; '4qt'; 'small'; '6qt'; 'large'; 'unknown')"),
        "large"
    );
    assert_eq!(txt("switch(Label; '4qt'; 'small'; 'unknown')"), "unknown");
    assert_eq!(txt("choose(2; 'a'; 'b'; 'c')"), "b");
    assert_eq!(txt("iferror(1 / 0 & 'x'; 'none')"), "none");
    assert_eq!(txt("let(n; GridX * 2; 'n' & n)"), "n12");
    assert_eq!(txt("let(t; Label; t & t)"), "6qt6qt");
    assert_eq!(num_with("if(Label = '6qt'; 1; 2)"), 1.0);
    // Case matters.
    assert_eq!(num_with("Label <> '6QT'"), 1.0);
    assert!(matches!(
        value_err("switch(Label; 1; 'a'; 'b')"),
        EvalError::MixedValues { .. }
    ));
    assert!(matches!(
        value_err("let(Label; 1; Label)"),
        EvalError::LocalShadowsParameter { .. }
    ));
    // Numbers in choices are as before.
    assert_eq!(num_with("if(GridX = 6; 1 mm; 2)"), 1.0);
}

#[test]
fn text_function_names_stay_parameter_names() {
    for name in ["Text", "text", "Left", "RIGHT", "len", "Concat"] {
        assert!(is_valid_name(name), "{name}");
        assert!(!is_reserved_name(name), "{name}");
        assert!(!is_newly_reserved(name), "{name}");
    }
    // A function before `(`, a name elsewhere.
    let e = parse("len(Label) + len * 2");
    assert_eq!(e.references(), ["Label", "len"]);
    let vars = Params(vec![
        ("Label", Value::Text("ab".to_owned())),
        ("len", Value::Number(Quantity::unitless(3.0))),
    ]);
    assert_eq!(e.eval(&vars, &ctx()), Ok(Quantity::unitless(8.0)));
    assert_eq!(e.to_string(), "len(Label) + len * 2");
}

#[test]
fn expressions_without_texts_read_as_before() {
    for text in [
        "d1 * 2 + 5 mm",
        "max(1,5; 2)",
        "value(d1; \"mm\")",
        "if(a = b; 1; 2)",
        "-2 ^ 2",
    ] {
        let e = parse(text);
        assert_eq!(parse(&e.to_string()), e, "{text}");
    }
    // The unit of `value` in quotes is a unit, in either quotes.
    assert_eq!(parse("value(d1; 'mm')"), parse("value(d1; \"mm\")"));
    assert_eq!(num("value(25.4 mm; 'in')"), 1.0);
}

#[test]
fn texts_have_a_length_limit() {
    let vars = Params(vec![("Big", Value::Text("x".repeat(MAX_TEXT_BYTES)))]);
    let e = parse("Big & 'y'").eval_value(&vars, &ctx()).unwrap_err();
    assert!(matches!(e, EvalError::TextTooLong { .. }), "{e}");
    let e = parse("concat(Big; 'y')")
        .eval_value(&vars, &ctx())
        .unwrap_err();
    assert!(matches!(e, EvalError::TextTooLong { .. }), "{e}");
}

#[test]
fn text_parameters_in_a_table() {
    let mut table = ParameterTable::new(ctx());
    let gx = table.add(spec("GridX", "6", Unit::NONE)).unwrap();
    let label = table
        .add(spec("Label", "'Box ' & GridX & 'x' & GridX", Unit::TEXT))
        .unwrap();
    let p = table.get(label).unwrap();
    assert!(p.is_text());
    assert_eq!(p.text(), Some("Box 6x6"));
    assert_eq!(p.formatted_value(3), "Box 6x6");
    assert_eq!(p.unit().to_string(), "Text");
    assert_eq!(p.dependencies(), [gx]);
    // A change follows.
    let changed = table.set_expression(gx, "4").unwrap();
    assert!(changed.contains(&label), "{changed:?}");
    assert_eq!(table.get(label).unwrap().text(), Some("Box 4x4"));
    // Another text parameter uses it; a number cannot.
    let lid = table
        .add(spec("Lid", "Label & ' lid'", Unit::TEXT))
        .unwrap();
    assert_eq!(table.get(lid).unwrap().text(), Some("Box 4x4 lid"));
    let e = table.add(spec("w", "Label * 2", Unit::MM)).unwrap_err();
    assert!(
        e.to_string().contains("text where a number is expected"),
        "{e}"
    );
    let e = table.add(spec("t", "GridX", Unit::TEXT)).unwrap_err();
    assert!(
        e.to_string().contains("a number where text is expected"),
        "{e}"
    );
    // The same text again changes nothing.
    assert!(
        table
            .set_expression(label, "'Box ' & GridX & 'x' & GridX")
            .unwrap()
            .is_empty()
    );
    // A rename follows the references, not the texts.
    table.rename(gx, "Count").unwrap();
    assert_eq!(
        table.get(label).unwrap().expression(),
        "'Box ' & Count & 'x' & Count"
    );
    // A text parameter has no limits.
    let e = table
        .set_limits(
            label,
            ParamLimits {
                max: Some("5".to_owned()),
                ..ParamLimits::default()
            },
        )
        .unwrap_err();
    assert!(
        matches!(
            e,
            TableError::Limit {
                error: LimitError::Text,
                ..
            }
        ),
        "{e}"
    );
    assert!(
        e.to_string().contains("a text parameter has no minimum"),
        "{e}"
    );
    // The unit changes to a number's and back.
    table.update(lid, "5", Unit::MM).unwrap();
    assert_eq!(table.get(lid).unwrap().text(), None);
    assert_eq!(table.value(lid), 5.0);
    table.update(lid, "Label", Unit::TEXT).unwrap();
    assert_eq!(table.get(lid).unwrap().text(), Some("Box 4x4"));
    // Loaded in any order.
    let table = ParameterTable::build(
        ctx(),
        [
            spec("Label", "Name & '!'", Unit::TEXT),
            spec("Name", "'Lid'", Unit::TEXT),
        ],
    )
    .unwrap();
    assert_eq!(table.lookup_text("Label").as_deref(), Some("Lid!"));
    assert_eq!(table.lookup("Label"), None);
    assert_eq!(table.evaluate_text("Label & Name").unwrap(), "Lid!Lid");
}

// ------------------------------------------------- table lookups (mitcad#667)

type MockColumn = (&'static str, Unit, Vec<TableCell<'static>>);

/// A table of the mock lookup: its name and columns, the first column
/// naming the rows.
struct MockTable {
    name: &'static str,
    columns: Vec<MockColumn>,
}

/// Parameters and tables, as a document's parameter table gives them.
struct Tables {
    params: Params,
    tables: Vec<MockTable>,
}

impl Lookup for Tables {
    fn lookup(&self, name: &str) -> Option<Quantity> {
        self.params.lookup(name)
    }

    fn lookup_text(&self, name: &str) -> Option<String> {
        self.params.lookup_text(name)
    }

    fn column(&self, table: &str, column: &str) -> Result<TableColumn<'_>, ColumnError> {
        let table = self
            .tables
            .iter()
            .find(|t| t.name == table)
            .ok_or(ColumnError::NoTable)?;
        let (_, unit, cells) = table
            .columns
            .iter()
            .find(|(name, ..)| *name == column)
            .ok_or(ColumnError::NoColumn)?;
        let labels = table.columns[0].2.clone();
        Ok(TableColumn::new(*unit, cells.clone()).with_labels(labels))
    }
}

/// An empty cell in [`number_cells`].
const EMPTY: f64 = f64::NAN;

/// Number cells, [`EMPTY`] for an empty one.
fn number_cells(values: &[f64]) -> Vec<TableCell<'static>> {
    values
        .iter()
        .map(|&n| {
            if n.is_nan() {
                TableCell::Empty
            } else {
                TableCell::Number(n)
            }
        })
        .collect()
}

fn text_cells(texts: &[&'static str]) -> Vec<TableCell<'static>> {
    texts.iter().copied().map(TableCell::Text).collect()
}

/// The parameters of [`params`] with `Size = 'M5'` and `Shaft = 5 mm`,
/// and three tables.
fn tables() -> Tables {
    let mut params = params();
    params.0.push(("Size", Value::Text("M5".to_owned())));
    params.0.push(("Shaft", Value::Number(len(5.0))));
    let screws = MockTable {
        name: "Screws",
        columns: vec![
            ("Size", Unit::TEXT, text_cells(&["M3", "M4", "M5", "M6"])),
            ("d", Unit::MM, number_cells(&[3.0, 4.0, 5.0, 6.0])),
            ("dk", Unit::MM, number_cells(&[5.5, 7.0, 8.5, EMPTY])),
            ("k", Unit::MM, number_cells(&[3.0, 4.0, 5.0, 6.0])),
            (
                "Count",
                Unit::NONE,
                number_cells(&[10.0, 20.0, EMPTY, 40.0]),
            ),
            (
                "Note",
                Unit::MM,
                number_cells(&[EMPTY, EMPTY, EMPTY, EMPTY]),
            ),
            // A reserved word as a column's name.
            ("in", Unit::IN, number_cells(&[0.125, 0.25, 0.25, 0.5])),
        ],
    };
    // Unsorted, with a key twice.
    let steps = MockTable {
        name: "Steps",
        columns: vec![
            ("d", Unit::MM, number_cells(&[10.0, 4.0, 6.0, 8.0, 4.0])),
            (
                "Pitch",
                Unit::MM,
                number_cells(&[1.5, 0.7, 1.0, 1.25, 0.75]),
            ),
        ],
    };
    let inch = MockTable {
        name: "Inch",
        columns: vec![
            ("Size", Unit::TEXT, text_cells(&["1/4", "1/2"])),
            ("d", Unit::IN, number_cells(&[0.25, 0.5])),
            ("Angle", Unit::DEG, number_cells(&[30.0, 45.0])),
        ],
    };
    Tables {
        params,
        tables: vec![screws, steps, inch],
    }
}

/// The value of `expression` with [`tables`].
fn look(expression: &str) -> Result<Value, EvalError> {
    parse(expression).eval_value(&tables(), &ctx())
}

/// The number of `expression` with [`tables`].
fn look_num(expression: &str) -> Quantity {
    parse(expression)
        .eval(&tables(), &ctx())
        .unwrap_or_else(|e| panic!("{expression:?}: {e}"))
}

/// The millimetres of `expression` with [`tables`], a length.
fn look_mm(expression: &str) -> f64 {
    let q = look_num(expression);
    assert_eq!(q.dims, Dims::LENGTH, "{expression:?}");
    q.value
}

/// The unitless number of `expression` with [`tables`].
fn look_n(expression: &str) -> f64 {
    let q = look_num(expression);
    assert_eq!(q.dims, Dims::NONE, "{expression:?}");
    q.value
}

/// The error of `expression` with [`tables`], as a value.
fn look_err(expression: &str) -> EvalError {
    look(expression).expect_err(expression)
}

#[test]
fn column_references_parse_and_print() {
    let e = parse("XLOOKUP(Size;Screws.Size;Screws.dk; 1 mm)");
    assert_eq!(e.to_string(), "xlookup(Size; Screws.Size; Screws.dk; 1 mm)");
    assert_eq!(parse(&e.to_string()), e);
    let NodeKind::Call { args, .. } = &e.root().kind else {
        panic!("{e}");
    };
    let column = NodeKind::Column {
        table: "Screws".to_owned(),
        column: "Size".to_owned(),
    };
    assert_eq!(args[1].kind, column);
    // Parameters and columns apart, each once.
    let e = parse("INDEX(Screws.k; MATCH(Size; Screws.Size)) + MAX(Screws.k; d1)");
    assert_eq!(e.references(), ["Size", "d1"]);
    assert_eq!(e.table_references(), [("Screws", "k"), ("Screws", "Size")]);
    let text = "INDEX(Screws.k; 1)";
    let parsed = parse(text);
    let spans = parsed.column_reference_spans();
    assert_eq!(spans.len(), 1);
    let (table, column, span) = spans[0];
    assert_eq!((table, column), ("Screws", "k"));
    assert_eq!(&text[span.start..span.end], "Screws.k");
    // A lookup is not a constant, though it names no parameter.
    let e = parse("XLOOKUP('M5'; Screws.Size; Screws.dk)");
    assert!(e.references().is_empty());
    assert!(!e.is_constant());
    // A column may have a reserved word as its name.
    let e = parse("max(Screws.in)");
    assert_eq!(e.table_references(), [("Screws", "in")]);
    // Function names in any case.
    assert_eq!(parse("Index(Screws.dk; 1)"), parse("index(Screws.dk; 1)"));
}

#[test]
fn exact_lookups() {
    // The examples of the design note.
    assert_eq!(look_mm("XLOOKUP(Size; Screws.Size; Screws.dk)"), 8.5);
    assert_eq!(look_mm("INDEX(Screws.k; MATCH(Size; Screws.Size))"), 5.0);
    assert_eq!(look_n("MATCH('M4'; Screws.Size)"), 2.0);
    assert_eq!(look_n("MATCH('M4'; Screws.Size; 0)"), 2.0);
    assert_eq!(look_mm("XLOOKUP('M3'; Screws.Size; Screws.dk; 0; 0)"), 5.5);
    assert_eq!(look_mm("INDEX(Screws.dk; 2)"), 7.0);
    // Number keys: a unitless key is read in the default unit.
    assert_eq!(look_mm("XLOOKUP(4; Screws.d; Screws.k)"), 4.0);
    assert_eq!(look_mm("XLOOKUP(4 mm; Screws.d; Screws.k)"), 4.0);
    assert_eq!(look_mm("XLOOKUP(0.4 cm; Screws.d; Screws.k)"), 4.0);
    assert_eq!(look_n("MATCH(Shaft; Screws.d)"), 3.0);
    // Equal as `==` compares, within its tolerance.
    assert_eq!(look_mm("XLOOKUP(4 + 1e-12; Screws.d; Screws.k)"), 4.0);
    // The first matching row wins.
    assert_eq!(look_n("MATCH(4; Steps.d)"), 2.0);
    assert_eq!(look_mm("XLOOKUP(4; Steps.d; Steps.Pitch)"), 0.7);
    // Texts are compared with their case.
    let e = look_err("MATCH('m5'; Screws.Size)");
    assert!(matches!(e, EvalError::NotFound { .. }), "{e}");
    // Inside other formulas, and through `let`.
    let text = "LET(s; 'M4'; XLOOKUP(s; Screws.Size; Screws.d) * 2)";
    assert_eq!(look_mm(text), 8.0);
    let text = "XLOOKUP(5; Screws.d; Screws.Size) & '!'";
    assert_eq!(look(text), Ok(Value::Text("M5!".to_owned())));
    let text = "IF(XLOOKUP(Size; Screws.Size; Screws.d) > 4; 1; 2)";
    assert_eq!(look_n(text), 1.0);
}

#[test]
fn approximate_lookups() {
    // Steps.d is 10, 4, 6, 8, 4 (unsorted); Pitch 1.5, 0.7, 1, 1.25, 0.75.
    // xlookup -1: exact, else the next smaller.
    for (key, pitch) in [
        ("5", 0.7),
        ("Shaft", 0.7),
        ("9", 1.25),
        ("8", 1.25),
        ("99", 1.5),
    ] {
        let text = format!("XLOOKUP({key}; Steps.d; Steps.Pitch; 0; -1)");
        assert_eq!(look_mm(&text), pitch, "{text}");
    }
    // xlookup 1: exact, else the next larger.
    for (key, pitch) in [("5", 1.0), ("1", 0.7), ("6", 1.0), ("9", 1.5)] {
        let text = format!("XLOOKUP({key}; Steps.d; Steps.Pitch; 0; 1)");
        assert_eq!(look_mm(&text), pitch, "{text}");
    }
    // match: 1 is the next smaller, -1 the next larger.
    assert_eq!(look_n("MATCH(5; Steps.d; 1)"), 2.0);
    assert_eq!(look_n("MATCH(5; Steps.d; -1)"), 3.0);
    assert_eq!(look_n("MATCH(9; Steps.d; 1)"), 4.0);
    assert_eq!(look_n("MATCH(9; Steps.d; -1)"), 1.0);
    assert_eq!(look_n("MATCH(6; Steps.d; -1)"), 3.0);
    // Nothing above or below the key.
    let e = look_err("MATCH(20; Steps.d; -1)");
    assert!(matches!(e, EvalError::NotFound { .. }), "{e}");
    assert_eq!(
        e.to_string(),
        "no row of Steps.d is 20 mm or more at byte 0"
    );
    let e = look_err("MATCH(1; Steps.d; 1)");
    assert_eq!(e.to_string(), "no row of Steps.d is 1 mm or less at byte 0");
    // Exact by default.
    let e = look_err("MATCH(5; Steps.d)");
    assert_eq!(e.to_string(), "no row of Steps.d is 5 mm at byte 0");
    // The approximate modes need a number key.
    for text in [
        "MATCH('M5'; Screws.Size; 1)",
        "XLOOKUP('M5'; Screws.Size; Screws.d; 0; -1)",
    ] {
        let e = look_err(text);
        assert!(matches!(e, EvalError::ApproximateText { .. }), "{text}");
    }
}

#[test]
fn units_through_a_lookup() {
    let tables = tables();
    let as_unit = |text: &str, unit: Unit| parse(text).eval_as(&tables, &ctx(), unit);
    // An `in` column into an `mm` parameter.
    let q = as_unit("XLOOKUP('1/4'; Inch.Size; Inch.d)", Unit::MM).unwrap();
    assert_eq!(q.dims, Dims::LENGTH);
    assert_close(q.value, 6.35);
    assert_close(look_mm("INDEX(Screws.in; 4)"), 12.7);
    // A key in millimetres in an inch column; a unitless one in the
    // default unit.
    assert_eq!(look_n("MATCH(12.7 mm; Inch.d)"), 2.0);
    assert_eq!(look_n("MATCH(12.7; Inch.d)"), 2.0);
    // An angle column.
    let q = as_unit("XLOOKUP('1/2'; Inch.Size; Inch.Angle)", Unit::DEG).unwrap();
    assert_close(q.value, Unit::DEG.to_canonical(45.0));
    // A unitless column gives a number, promoted in a length parameter.
    let text = "XLOOKUP('M4'; Screws.Size; Screws.Count)";
    assert_eq!(look_n(text), 20.0);
    assert_eq!(as_unit(text, Unit::MM), Ok(len(20.0)));
    assert_eq!(as_unit(text, Unit::NONE), Ok(Quantity::unitless(20.0)));
    // A length column in a unitless parameter is refused.
    let text = "XLOOKUP('M4'; Screws.Size; Screws.d)";
    assert!(as_unit(text, Unit::NONE).is_err());
    // A text column gives a text; a text in a number parameter is refused.
    let text = "XLOOKUP(5 mm; Screws.d; Screws.Size)";
    assert_eq!(parse(text).eval_text(&tables, &ctx()), Ok("M5".to_owned()));
    let e = as_unit(text, Unit::MM).unwrap_err();
    assert!(matches!(e, EvalError::NotNumber { .. }), "{e}");
    let text = "XLOOKUP('M5'; Screws.Size; Screws.d)";
    let e = parse(text).eval_text(&tables, &ctx()).unwrap_err();
    assert!(matches!(e, EvalError::NotText { .. }), "{e}");
    // A unitless key or column next to the other's dimension, as rule 3.
    assert_eq!(look_n("MATCH(Shaft; Screws.d)"), 3.0);
    assert_eq!(look_n("MATCH(20 mm; Screws.Count)"), 2.0);
}

#[test]
fn keys_not_found() {
    let e = look_err("XLOOKUP('M7'; Screws.Size; Screws.dk)");
    assert!(matches!(e, EvalError::NotFound { .. }), "{e}");
    assert_eq!(e.to_string(), "no row of Screws.Size is 'M7' at byte 0");
    let e = look_err("MATCH(7; Screws.d)");
    assert_eq!(e.to_string(), "no row of Screws.d is 7 mm at byte 0");
    // `if_not_found` instead, in the results' dimension.
    assert_eq!(look_mm("XLOOKUP('M7'; Screws.Size; Screws.dk; 1 mm)"), 1.0);
    assert_eq!(look_mm("XLOOKUP('M7'; Screws.Size; Screws.dk; 2)"), 2.0);
    let text = "XLOOKUP(7; Screws.d; Screws.Size; 'none')";
    assert_eq!(look(text), Ok(Value::Text("none".to_owned())));
    // Evaluated anyway: a value error in it counts only when it is used,
    let text = "XLOOKUP('M5'; Screws.Size; Screws.dk; 1 / 0)";
    assert_eq!(look_mm(text), 8.5);
    let e = look_err("XLOOKUP('M7'; Screws.Size; Screws.dk; 1 / 0)");
    assert!(matches!(e, EvalError::DivisionByZero { .. }), "{e}");
    // and its kind and dimension must agree with the results'.
    for text in [
        "XLOOKUP('M5'; Screws.Size; Screws.dk; 30 deg)",
        "XLOOKUP('M5'; Screws.Size; Screws.dk; 'none')",
        "XLOOKUP('M5'; Screws.Size; Screws.dk; Nothing)",
    ] {
        let e = look_err(text);
        assert!(
            matches!(
                e,
                EvalError::DimensionMismatch { .. }
                    | EvalError::MixedValues { .. }
                    | EvalError::UnknownReference { .. }
            ),
            "{text}: {e}"
        );
    }
    // A unitless result next to a fallback with a unit is promoted, as
    // with `if`.
    let text = "XLOOKUP('M4'; Screws.Size; Screws.Count; 1 mm)";
    assert_eq!(look_mm(text), 20.0);
    // IFERROR catches it.
    let text = "IFERROR(XLOOKUP('M7'; Screws.Size; Screws.dk); 3 mm)";
    assert_eq!(look_mm(text), 3.0);
    assert_eq!(look_n("IFERROR(MATCH('M7'; Screws.Size); 0)"), 0.0);
    let text = "IFERROR(XLOOKUP('M5'; Screws.Size; Screws.dk); 3 mm)";
    assert_eq!(look_mm(text), 8.5);
    // As does an `if` branch not taken.
    let text = "IF(1; 2 mm; XLOOKUP('M7'; Screws.Size; Screws.dk))";
    assert_eq!(look_mm(text), 2.0);
}

#[test]
fn empty_cells_and_rows_out_of_range() {
    // Screws.dk is empty in row 4, labelled M6.
    let e = look_err("XLOOKUP('M6'; Screws.Size; Screws.dk)");
    assert!(matches!(e, EvalError::EmptyCell { row: 4, .. }), "{e}");
    assert_eq!(e.to_string(), "Screws.dk is empty in row 4 (M6) at byte 27");
    for text in [
        "XLOOKUP('M6'; Screws.Size; Screws.dk; 1 mm)",
        "INDEX(Screws.dk; 4)",
    ] {
        let e = look_err(text);
        assert!(matches!(e, EvalError::EmptyCell { .. }), "{text}: {e}");
    }
    assert_eq!(look_mm("IFERROR(INDEX(Screws.dk; 4); 9 mm)"), 9.0);
    // Empty key cells are skipped.
    assert_eq!(look_n("MATCH(40; Screws.Count)"), 4.0);
    assert_eq!(look_n("MATCH(30; Screws.Count; 1)"), 2.0);
    let e = look_err("MATCH(0; Screws.Note; -1)");
    assert!(matches!(e, EvalError::NotFound { .. }), "{e}");
    // INDEX takes the whole part of its row, which must be in the column.
    assert_eq!(look_mm("INDEX(Screws.dk; 2.9)"), 7.0);
    for (text, expected) in [
        ("INDEX(Screws.dk; 5)", 5.0),
        ("INDEX(Screws.dk; 0)", 0.0),
        ("INDEX(Screws.dk; -1)", -1.0),
        ("INDEX(Screws.dk; 0.5)", 0.0),
    ] {
        let e = look_err(text);
        let EvalError::RowOutOfRange { row, rows, .. } = &e else {
            panic!("{text}: {e}");
        };
        assert_eq!((*row, *rows), (expected, 4), "{text}");
    }
    assert_eq!(
        look_err("INDEX(Screws.dk; 5)").to_string(),
        "row 5 is outside the rows of Screws.dk (1 to 4) at byte 17"
    );
    assert_eq!(look_n("IFERROR(INDEX(Screws.dk; 9); 0)"), 0.0);
    let e = look_err("INDEX(Screws.dk; 1 mm)");
    assert!(matches!(e, EvalError::NotDimensionless { .. }), "{e}");
}

#[test]
fn lookup_type_errors() {
    // A text key in a number column, a number key in a text column.
    let e = look_err("XLOOKUP('M5'; Screws.d; Screws.dk)");
    assert_eq!(
        e,
        EvalError::NotNumber {
            span: Span::new(8, 12)
        }
    );
    let e = look_err("XLOOKUP(5; Screws.Size; Screws.dk)");
    assert!(matches!(e, EvalError::NotText { .. }), "{e}");
    let e = look_err("MATCH(Size; Screws.d)");
    assert!(matches!(e, EvalError::NotNumber { .. }), "{e}");
    // A number key of another dimension.
    for text in [
        "XLOOKUP(30 deg; Screws.d; Screws.dk)",
        "MATCH(30 deg; Inch.d)",
    ] {
        let e = look_err(text);
        assert!(matches!(e, EvalError::DimensionMismatch { .. }), "{text}");
    }
    // Type errors are not caught by IFERROR.
    for text in [
        "IFERROR(XLOOKUP(5; Screws.Size; Screws.dk); 0)",
        "IFERROR(MATCH('M5'; Screws.d); 0)",
        "IFERROR(XLOOKUP(30 deg; Screws.d; Screws.dk); 0)",
    ] {
        let e = look_err(text);
        assert!(
            matches!(
                e,
                EvalError::NotText { .. }
                    | EvalError::NotNumber { .. }
                    | EvalError::DimensionMismatch { .. }
            ),
            "{text}: {e}"
        );
    }
    // Keys and results of one table.
    let e = look_err("XLOOKUP('M5'; Screws.Size; Inch.d)");
    assert!(matches!(e, EvalError::DifferentTables { .. }), "{e}");
    // Unknown tables and columns, at their names.
    let text = "XLOOKUP('M5'; Bolts.Size; Bolts.dk)";
    let e = look_err(text);
    assert!(matches!(e, EvalError::UnknownTable { .. }), "{e}");
    assert_eq!(e.to_string(), "unknown table Bolts at byte 14");
    let span = e.span().unwrap();
    assert_eq!(&text[span.start..span.end], "Bolts");
    let text = "XLOOKUP('M5'; Screws.Size; Screws.dx)";
    let e = look_err(text);
    assert!(matches!(e, EvalError::UnknownColumn { .. }), "{e}");
    assert_eq!(e.to_string(), "table Screws has no column dx at byte 34");
    let span = e.span().unwrap();
    assert_eq!(&text[span.start..span.end], "dx");
    // A lookup without tables (the trait's default) knows none.
    let e = try_eval("INDEX(Screws.dk; 1)", &[]).unwrap_err();
    assert!(matches!(e, EvalError::UnknownTable { .. }), "{e}");
    let closure = |_: &str| -> Option<Quantity> { None };
    let e = parse("MAX(Screws.dk)").eval(&closure, &ctx()).unwrap_err();
    assert!(matches!(e, EvalError::UnknownTable { .. }), "{e}");
}

#[test]
fn aggregates_take_columns() {
    // Their non-empty cells.
    assert_eq!(look_mm("MAX(Screws.dk)"), 8.5);
    assert_eq!(look_mm("MIN(Screws.dk)"), 5.5);
    assert_eq!(look_n("SUM(Screws.Count)"), 70.0);
    assert_eq!(look_mm("AVERAGE(Screws.d)"), 4.5);
    assert_eq!(look_mm("MEDIAN(Steps.d)"), 6.0);
    assert_close(look_mm("MAX(Screws.in)"), 12.7);
    // Beside values, unitless ones promoted.
    assert_eq!(look_mm("MIN(Screws.d; 2)"), 2.0);
    assert_eq!(look_mm("MAX(Screws.d; Steps.d)"), 10.0);
    assert_eq!(look_mm("SUM(Screws.d; 1 cm)"), 28.0);
    // Columns of only empty cells.
    assert_eq!(look_mm("SUM(Screws.Note)"), 0.0);
    let e = look_err("MAX(Screws.Note)");
    assert!(matches!(e, EvalError::NoValues { .. }), "{e}");
    assert_eq!(look_n("IFERROR(AVERAGE(Screws.Note); 1)"), 1.0);
    assert_eq!(look_n("MAX(Screws.Note; 3)"), 3.0);
    // Text columns and other dimensions are refused.
    let e = look_err("MAX(Screws.Size)");
    assert!(matches!(e, EvalError::NotNumber { .. }), "{e}");
    let e = look_err("SUM(Screws.d; 30 deg)");
    assert!(matches!(e, EvalError::DimensionMismatch { .. }), "{e}");
    // Without columns, as before: min and max still need two values.
    let e = parse_err("max(5)").kind;
    assert!(matches!(e, ParseErrorKind::WrongArgumentCount { .. }));
    assert_eq!(num("max(1; 5)"), 5.0);
}

#[test]
fn columns_outside_lookups_are_refused() {
    for (text, column, at) in [
        ("Screws.dk", "Screws.dk", Span::new(0, 9)),
        ("Screws.dk + 1 mm", "Screws.dk", Span::new(0, 9)),
        ("-Screws.dk", "Screws.dk", Span::new(1, 10)),
        ("(Screws.dk)", "Screws.dk", Span::new(1, 10)),
        ("IF(1; Screws.dk; 0)", "Screws.dk", Span::new(6, 15)),
        (
            "XLOOKUP(Screws.d; Screws.d; Screws.dk)",
            "Screws.d",
            Span::new(8, 16),
        ),
        (
            "MATCH(1; Screws.d; Screws.k)",
            "Screws.k",
            Span::new(19, 27),
        ),
        ("INDEX(Screws.dk; Screws.k)", "Screws.k", Span::new(17, 25)),
        ("PRODUCT(Screws.d)", "Screws.d", Span::new(8, 16)),
        ("SUM(Screws.d mm)", "Screws.d", Span::new(4, 12)),
        ("SUM(Screws.d * 2)", "Screws.d", Span::new(4, 12)),
        ("LET(c; Screws.d; SUM(c))", "Screws.d", Span::new(7, 15)),
    ] {
        let e = parse_err(text);
        let kind = ParseErrorKind::ColumnOutsideLookup(column.to_owned());
        assert_eq!((e.kind, e.span), (kind, at), "{text}");
    }
    assert_eq!(
        parse_err("Screws.dk * 2").to_string(),
        "Screws.dk is a column: look a value up with XLOOKUP or INDEX at byte 0"
    );
    // A value where a column must be.
    for (text, function, argument) in [
        ("INDEX(2; 1)", "index", 1),
        ("XLOOKUP('M5'; 'M5'; Screws.dk)", "xlookup", 2),
        ("XLOOKUP('M5'; Screws.Size; d1)", "xlookup", 3),
        ("MATCH('M5'; Size)", "match", 2),
    ] {
        let kind = ParseErrorKind::ColumnExpected { function, argument };
        assert_eq!(parse_err(text).kind, kind, "{text}");
    }
    assert_eq!(
        parse_err("INDEX(2; 1)").to_string(),
        "argument 1 of index must be a column such as Table.Column at byte 6"
    );
    // The dot only between names, with no spaces.
    for text in ["Screws .dk", "Screws. dk", "Screws.", ".dk"] {
        let e = parse_err(text);
        assert_eq!(e.kind, ParseErrorKind::UnexpectedChar('.'), "{text}");
    }
    let e = parse_err("Screws.dk.x");
    assert!(matches!(e.kind, ParseErrorKind::Unexpected { .. }), "{e}");
    // A tree built by hand is refused when evaluated.
    let column = Node::new(
        NodeKind::Column {
            table: "Screws".to_owned(),
            column: "dk".to_owned(),
        },
        Span::new(0, 9),
    );
    let e = Expr::from_node(column.clone())
        .eval_value(&tables(), &ctx())
        .unwrap_err();
    assert!(matches!(e, EvalError::ColumnOutsideLookup { .. }), "{e}");
    let one = Node::new(NodeKind::Number(1.0), Span::default());
    let index = Node::new(
        NodeKind::Call {
            function: Function::Index,
            args: vec![one, column],
        },
        Span::default(),
    );
    let e = Expr::from_node(index).eval(&tables(), &ctx()).unwrap_err();
    assert!(matches!(e, EvalError::NotColumn { .. }), "{e}");
}

#[test]
fn unsupported_and_invalid_modes() {
    // Wildcard matching and the search order are refused.
    let e = look_err("XLOOKUP('M5'; Screws.Size; Screws.dk; 0; 2)");
    assert!(matches!(e, EvalError::LookupUnsupported { .. }), "{e}");
    assert_eq!(
        e.to_string(),
        "wildcard matching (mode 2) is not supported at byte 41"
    );
    let e = look_err("XLOOKUP('M5'; Screws.Size; Screws.dk; 0; 0; 1)");
    assert!(matches!(e, EvalError::LookupUnsupported { .. }), "{e}");
    let e = parse_err("XLOOKUP('M5'; Screws.Size; Screws.dk; 0; 0; 1; 1)").kind;
    assert!(matches!(e, ParseErrorKind::WrongArgumentCount { .. }));
    // Not value errors: IFERROR does not catch them.
    let e = look_err("IFERROR(XLOOKUP('M5'; Screws.Size; Screws.dk; 0; 2); 0)");
    assert!(matches!(e, EvalError::LookupUnsupported { .. }), "{e}");
    // Other modes.
    for (text, expected) in [
        ("XLOOKUP('M5'; Screws.Size; Screws.dk; 0; 3)", 3.0),
        ("XLOOKUP(5; Screws.d; Screws.dk; 0; 0.5)", 0.5),
        ("MATCH('M5'; Screws.Size; 2)", 2.0),
        ("MATCH(5; Screws.d; -2)", -2.0),
    ] {
        let e = look_err(text);
        let EvalError::InvalidMode { mode, .. } = &e else {
            panic!("{text}: {e}");
        };
        assert_eq!(*mode, expected, "{text}");
    }
    let e = look_err("MATCH(5; Screws.d; 1 mm)");
    assert!(matches!(e, EvalError::NotDimensionless { .. }), "{e}");
    let e = parse_err("MATCH('M5')").kind;
    assert!(matches!(e, ParseErrorKind::WrongArgumentCount { .. }));
}

#[test]
fn lookup_function_names_stay_parameter_names() {
    // As the text functions' (rule 29): functions only before `(`.
    for name in ["Index", "index", "Match", "MATCH", "XLookup", "xlookup"] {
        assert!(is_valid_name(name), "{name}");
        assert!(!is_newly_reserved(name), "{name}");
    }
    let vars = [("Index", len(3.0)), ("match", len(1.0))];
    assert_eq!(try_eval("Index * 2 + match", &vars), Ok(len(7.0)));
    // The names of a column reference are not renamed as parameters.
    let renamed = HashMap::from([("Max".to_owned(), "Max_1".to_owned())]);
    assert_eq!(
        rename_names("Max + max(T.Max) + Max.x", &renamed),
        "Max_1 + max(T.Max) + Max.x"
    );
    // Help for the functions, with examples that parse.
    for function in [Function::XLookup, Function::Match, Function::Index] {
        let help = assistance::function_help(function);
        assert!(Expr::parse(help.example).is_ok(), "{}", help.name);
    }
    let text = "XLOOKUP(Size; Screws.Size; ";
    assert_eq!(
        assistance::signature_at(text, text.len()),
        Some((Function::XLookup, 2))
    );
}

#[test]
fn expressions_without_columns_read_as_before() {
    // A dot that is no column reference reads as it did; numbers keep
    // their points.
    assert_eq!(num("1.5 + .5 + 2."), 4.0);
    assert!(Expr::parse("d1.5").is_err());
    assert!(Expr::parse("d1 .5").is_err());
    assert_eq!(num("max(1,5; 2)"), 2.0);
    for text in [
        "sum(1; 2)",
        "min(1 mm; 2)",
        "median(1; 2; 3)",
        "average(2; 4)",
    ] {
        let e = parse(text);
        assert_eq!(parse(&e.to_string()), e, "{text}");
        assert!(e.table_references().is_empty(), "{text}");
    }
}

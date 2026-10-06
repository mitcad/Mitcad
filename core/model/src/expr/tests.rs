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
        "'max' takes at least 2 arguments, found 1; separate them with ';' or a space \
         after the comma: 'max(1; 5)' at byte 0"
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

#[test]
fn equality_ignores_positions() {
    assert_eq!(parse("1+2"), parse("  1 +   2 "));
    assert_ne!(parse("1 + 2"), parse("2 + 1"));
    assert_eq!("d1 * 2".parse::<Expr>(), Ok(parse("d1*2")));
}

#[test]
fn names_are_case_sensitive() {
    assert_eq!(parse("pi").references(), ["pi"]);
    assert_eq!(
        parse_err("Sin(30)").kind,
        ParseErrorKind::UnknownFunction("Sin".into())
    );
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
    assert_eq!(
        parse_err("5 xyz").to_string(),
        "unknown unit 'xyz' at byte 2"
    );
    assert_eq!(
        parse_err("2 ** 3").to_string(),
        "use '^' for powers, not '**' at byte 2"
    );
    assert_eq!(
        eval_err("1 mm + 2 deg").to_string(),
        "cannot apply '+' to length and angle at byte 0"
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
    assert_eq!(
        eval_as("5 mm", &[], Unit::NONE),
        Err(EvalError::WrongDimension {
            expected: Dims::NONE,
            found: Dims::LENGTH
        })
    );
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
        "parameter 'x': unknown parameter 'depth' at byte 8"
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
    assert!(error.to_string().contains("'max(1; 5)'"), "{error}");
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

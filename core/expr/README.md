# mitcad-expr

MitCad's parameter expressions with units, without dependencies: the
lexer and parser (`d1 * 2 + 5 mm`, `30 deg`, `if(a == b; 1; 2)`, the
inch and foot marks `.875"` and `3' 6"` after a number), the
evaluator with dimensions and units, formatting, parameter limits (`min`,
`max`, `step`), text values, and the `ParameterTable` that keeps named
parameters and evaluates them in dependency order. The language is a
superset of the parameter expressions of `.f3d` designs; `f3d` checks
their stored values.

The crate's documentation (`src/lib.rs`) holds the grammar and the
numbered rules. The model re-exports the crate as `mitcad_model::expr`,
so the model and the crates above it use it by that path; the `.f3d`
reader's corpus test uses it directly.

Its tests (`cargo test -p mitcad-expr`) run as the ctest `core.expr`.

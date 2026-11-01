<!-- SPDX-License-Identifier: MIT -->
# Table lookups in parameter expressions (design note)

Design note for mitcad#667. Status: **agreed with the owner on
2026-10-27** (every recommendation of the open questions below); nothing
here is implemented yet. Configurations build on it
([configurations.md](configurations.md)). It builds on the
spreadsheet-style functions (mitcad#663), text parameters (mitcad#664),
parameter limits (mitcad#665), the parameters CSV (mitcad#666) and the
Formula Editor (mitcad#668).

A design keeps named **tables** of data (screw sizes, standard
dimensions, coin sizes) and parameter expressions look values up in them
with spreadsheet functions:

```text
Size     = 'M5'                                        (text parameter)
HeadDia  = XLOOKUP(Size; Screws.Size; Screws.dk)       → 8.5 mm
HeadH    = INDEX(Screws.k; MATCH(Size; Screws.Size))   → 5 mm
Pitch    = XLOOKUP(Shaft; Screws.d; Screws.Pitch; 1 mm; -1)
```

## Storage

A new top-level field `tables` in the design file (left out when empty,
listed in `TOP_FIELDS` of `core/model/src/file/v2.rs`), kept in the
document beside `configurations`, in a new module
`core/model/src/tables.rs`:

```json
"tables": [
  {"name": "Screws",
   "comment": "ISO 4762 socket head cap screws",
   "columns": [
     {"name": "Size",  "unit": "Text"},
     {"name": "d",     "unit": "mm"},
     {"name": "dk",    "unit": "mm"},
     {"name": "k",     "unit": "mm"},
     {"name": "Pitch", "unit": "mm"}],
   "rows": [
     ["M3", 3, 5.5, 3, 0.5],
     ["M4", 4, 7,   4, 0.7],
     ["M5", 5, 8.5, 5, 0.8]]}
]
```

- **Table names** follow the parameter name rules (`is_valid_name`:
  case-sensitive, not reserved) and share the parameters' namespace: a
  table cannot take a parameter's name, nor a parameter a table's. This
  keeps `Screws` unambiguous in error messages and in the Formula Editor.
- **Column names** use the same characters as parameter names and are
  unique within their table. They may be reserved words (`in`, `Max`),
  since a column is only ever written after `Table.`.
- **A column's unit** is any unit a parameter takes (`mm`, `in`, `deg`,
  `mm^2`), `""` for unitless, or `Text`. A number cell is stored as a JSON
  number **in the column's unit** (`5.5` in an `mm` column is 5.5 mm), a
  text cell as a JSON string, an empty cell as `null`. A cell of the wrong
  kind (text in a number column, a number in a `Text` column) is refused.
- **Cells are constants**, not expressions (open question 1): a table is
  data, it depends on nothing, and so it can never be part of a cycle.
- Sizes: at most 10 000 rows and 100 columns per table; a text cell at
  most `MAX_TEXT_BYTES`.
- Tables are part of the definition: undo and redo restore them, the
  version comparison (`diff`) shows them row by row (`table Screws row 3
  (M5): dk 8.5 mm -> 8.7 mm`, the row labelled by its first cell), and
  copying parameters into another document brings the tables they use (a
  different table of the same name there gets a free name, and the copied
  expressions follow).

## Syntax and functions

`Table.Column` (no spaces around the dot) is a **column reference**. The
lexer makes `.` a token only between a name and a name start; today that
text is an error (`unexpected character “.”`), so the language stays a
superset and every expression that parses now parses the same. A column
reference is a list of cells and is accepted only as an argument of the
functions below; anywhere else it is an error ("Screws.dk is a column:
look a value up with XLOOKUP or INDEX").

Both XLOOKUP and INDEX/MATCH, with the spreadsheet signatures, so that
formulas typed from habit work:

| Function | Result |
|---|---|
| `XLOOKUP(key; keys; results; [if_not_found]; [mode])` | the cell of `results` in the row where `keys` matches `key` |
| `MATCH(key; keys; [mode])` | the row number (from 1, unitless) where `keys` matches `key` |
| `INDEX(column; row)` | the cell of `column` in row `row` |
| `MIN`, `MAX`, `SUM`, `AVERAGE`, `MEDIAN` | also take a number column: its non-empty cells |

**Matching.**

- `keys` and `results` must be columns of the same table.
- Rows are searched from the top; empty key cells are skipped. Where
  several rows match, the first one wins.
- Modes, with the spreadsheet numbers (which differ between the two
  functions):

  | Match | XLOOKUP `mode` | MATCH `mode` |
  |---|---|---|
  | exact (default of both) | `0` | `0` |
  | exact, else the next smaller (the largest cell ≤ key) | `-1` | `1` |
  | exact, else the next larger (the smallest cell ≥ key) | `1` | `-1` |

  The approximate modes look at every row, so the column need not be
  sorted. A wildcard mode (`2`) and XLOOKUP's sixth argument (the search
  order) are refused as not supported.
- **MATCH defaults to exact**, unlike the spreadsheet default `1`
  (open question 3). Spreadsheets themselves moved to exact with XLOOKUP,
  since an approximate default returns a wrong row without a word.

**Numbers and texts.**

- A number key needs a number column of its dimension. A unitless key
  next to a length column is read in the default unit, as rule 3 of
  `core/expr/src/lib.rs` promotes it. Exact equality uses the
  tolerance of `==` (rule 10: relative 1e-9).
- A text key needs a `Text` column. Texts are equal as `==` compares
  them: same characters, **case included** (rule 28; open question 4).
  The approximate modes need numbers: with a text key they are an error.
- A text key in a number column, or a number key in a `Text` column, is a
  type error (rule 27). Type errors are not caught by IFERROR.

**Nothing found.**

- No matching row is a value error: `no row of Screws.Size is 'M7'`, like
  `switch` without a match.
- XLOOKUP's `if_not_found` is used instead. As with `if` (rule 11), it is
  evaluated anyway; a value error in it counts only when it is used, and
  its dimension must agree with `results`.
- `IFERROR(…; fallback)` catches the error too.
- An empty result cell is a value error (`Screws.dk is empty in row 3
  (M5)`).
- `INDEX` takes the whole part of `row`; a row outside 1…rows is a value
  error, as `choose` treats an index.

**Units through a lookup.** The result is the cell as a value of the
column's unit: `5.5` in an `mm` column is 5.5 mm, `0.25` in an `in` column
is 6.35 mm, a unitless column gives a number, a `Text` column a text. From
there the usual rules apply: in a length parameter a unitless result is
promoted (rule 4), and a text result in a number parameter is refused.

## Dependencies and recompute

- `Expr` gains `table_references()`: the `(table, column)` pairs it uses,
  next to `references()` (parameter names, unchanged).
- The `Lookup` trait gains `column(table, column)`, which gives the
  column's cells and unit. `ParameterTable` evaluates with the document's
  tables beside the parameters, by passing them into its `Overlay`.
- A parameter depends on each table it names. A table depends on
  nothing, so tables come before every parameter and no cycle can pass
  through one.
- **A table edit** (`set_table`, a CSV import) re-evaluates every
  parameter that names the table, and their dependents, in one atomic step
  (as `set_expressions` does). If any of them would fail (a key no longer
  found, a column removed or changed in kind), the edit is refused with
  that parameter and its error, and nothing changes. The resulting
  `ChangeSet` lists only the parameters whose values changed. Features
  then recompute through the existing rule: only features whose inputs
  changed value. A table edit that changes no used value recomputes
  nothing.
- Feature slots and sketch dimensions take lookups through their
  parameters' expressions; nothing in the features changes. Limits
  (`min`, `max`) are expressions and may use lookups too.
- **Renames.** A table rename or a column rename rewrites every
  expression and limit that uses it, as parameter renames do
  (`replace_references` extended to column references).
- **Deletes.** Deleting a table, or dropping a column, that a parameter
  uses is refused, naming the users (as `delete_parameter` is).

## Relation to configurations

Configurations (`core/model-base/src/configurations.rs`, mitcad#64) and tables
answer different questions and stay **separate** (open question 6):

- A **configuration row** is a variant of the whole design. It sets
  parameter expressions, has selectors for cascaded choices, a
  designation, a part number and a supplier page, and is what a library
  part is inserted in.
- A **table** is reference data that formulas read; it changes nothing by
  itself.

They work together. A configuration row sets only the key
(`Size = 'M5'`), and the dimensions follow through lookups. A library
author then keeps the standard's numbers in one table instead of
repeating them in every configuration row, and a row's `values` stay
short. Applying a configuration is a parameter change, so the lookups
re-evaluate with no extra rule.

Not proposed now: generating configuration rows from a table's key
column (one row per `Size`). It is a small command to file later if
library authors ask for it.

## CSV import and export of a table

One CSV per table, as spreadsheets export one sheet per file. It is
separate from the parameters CSV (mitcad#666), which stays as it is: a
parameter row whose expression uses a lookup exports and imports as text,
and where the table is missing its `value` column is taken, as for any
expression that does not evaluate.

- **Format.** One header row of column names, each with its unit in
  brackets: `Size [Text],d [mm],dk [mm],Pitch [mm]`, and `[]` for
  unitless. UTF-8, RFC 4180 quoting, CRLF, using the reader and writer of
  `parameters_csv.rs`; a header with semicolons and no comma is read with
  `;`.
- **Reading cells.** A header without brackets is a `Text` column when any
  cell is not a number, else a unitless one (mitcad#804: a column whose
  cells have units takes the first one's unit, in the design's default
  units, so `10 mm` makes an `mm` column). A cell is read as a constant
  expression, so `5,5` (from a `;`-separated file) is 5.5, and `0.25 in`
  in an `mm` column is stored as 6.35. A name the model cannot use
  (`Head diameter`) gets a free one (`Head_diameter`), as the parameters
  CSV renames.
- **Export** writes every unit in brackets (`[Text]` too) and numbers
  with decimal points (the shortest decimal that gives the value back), so
  export and import give the same table.
- **Import** creates the table or replaces it whole. It goes through the
  same atomic check as `set_table`: parameters whose values change are
  listed, and a parameter that would fail refuses the import, as a refused
  cell does (mitcad#804). The preview shows this first; the import is one
  undo step.

## UI

A **Tables** tab in Edit Parameters (open question 5), since tables
are edited together with the parameters that use them, and the dialog
already works during sketch and feature edits:

- A list of tables on the left: New, Rename, Delete, Import CSV…,
  Export CSV….
- A grid on the right. Each header cell has the column's name and a unit
  box (lengths, angles, unitless, Text); there are add, remove and move
  for rows and columns.
- A wrong cell stays red with the reason, as a refused parameter cell
  does. Each committed edit is one undo step (`Edit Table Screws`).
- Changing a column's unit converts the numbers (the values stay); a
  change between unitless and a unit keeps the numbers.
- A "Used by" line names the parameters that use the table.
- The Formula Editor and the parameter suggestions complete table names,
  then column names after `Screws.`, and the function catalogue lists
  XLOOKUP, MATCH and INDEX with their signature help.

## Commands and queries (for commands.md)

| | Fields | Result |
|---|---|---|
| command `set_table` | `name`, `table`: `comment`, `columns` [{`name`, `unit`}], `rows` (arrays of numbers in the column's unit, texts, null) | `changed` (parameters whose values changed); creates or replaces; undo step `Edit Table <name>`; refused with the first problem or the first parameter that would fail |
| command `rename_table` | `name`, `new_name` | expressions follow |
| command `rename_table_column` | `table`, `name`, `new_name` | expressions follow |
| command `delete_table` | `name` | refused while a parameter uses it |
| query `tables` | `name` (optional: only that table) | `tables`: `name`, `comment`, `columns`, `rows`, `used_by` (parameter names), `problems` |
| query `table_csv` | `name` | `csv`, `rows` |
| query `table_csv_preview` | `name`, `csv` | `columns` (name, unit, `renamed`, `inferred`), `rows`, `refused` (cells: line, column, reason), `changed` (parameters: `name`, `from`, `to`), `failing` (parameters with their errors), `summary` |
| command `import_table_csv` | `name`, `csv` | as the preview; undo step `Import Table <name>` |

Changes to existing queries:

- `parameters`: `table_dependencies` (`["Screws.Size", "Screws.dk"]`).
- `evaluate`: its `references` field is unchanged; it gains
  `table_references`.
- `formula_help`: `tables` ({name, columns with units}) for completion,
  and the three functions in `functions`.
- `diff`: a `tables` section (added, deleted, renamed tables; changed
  columns; changed rows).

## Other CAD tools

- **Spreadsheet objects in the document** (the `.FCStd` format): cells
  with aliases are referenced from expressions as `Sheet.alias`. Lookups
  there go through cell addresses. MitCad's `.FCStd` import already turns
  aliased cells into parameters
  (`core/import-fcstd/src/params.rs`); tables do not change that.
  MitCad does not take over A1 addressing: named columns survive inserted
  rows and columns, and read better in a formula.
- **Design tables and family tables** (a common concept): an embedded or
  linked spreadsheet whose rows are variants and whose columns are
  parameters and feature states. That is MitCad's configurations, not
  lookups.
- **Parameters linked to an external spreadsheet file** (several
  formats): a live link to a file outside the design. MitCad imports CSV
  instead, so a design stays self-contained and reproducible from its
  version history.
- **The `.f3d` parameter language** has no tables or lookups; its
  variants are configuration tables whose rows set parameters. There is
  nothing to import for this ticket.
- **Rule-based configurators** look up rows of embedded tables by key in
  a rule language. This is the closest in spirit; MitCad does the same
  with spreadsheet function names.

## Not in scope

These are not part of this design: expressions in cells; A1 addresses
and ranges; live links to external files; wildcard and pattern matching;
linear interpolation between rows; array formulas and lookups on two
keys.

For two keys, use a combined key column (`M5x16`) and
`XLOOKUP(Size & "x" & Length; Screws.Key; …)` (open question 7).

## Implementation tickets

1. **Expression language: column references and lookup functions**
   (`core/expr/src/`).
   - Adds the `Table.Column` token and node, `Lookup::column`,
     `Expr::table_references`, and XLOOKUP, MATCH and INDEX with the
     modes above.
   - The aggregates take columns. The grammar and rules in `mod.rs`, and
     the function help in `assistance.rs`, are updated.
   - Done when: `expr` tests with a mock lookup cover exact and both
     approximate modes for each function, number and text keys, units
     through a lookup (`in` column into an `mm` parameter, unitless,
     text), not found with and without `if_not_found` and under IFERROR,
     an empty cell, INDEX out of range, type errors, a column outside a
     lookup, wildcard mode refused, and that every expression of the
     existing test suite parses and evaluates the same.
2. **Tables in the model and the design file** (`core/model/src/tables.rs`,
   document, `file/v2.rs`, api). Depends on 1.
   - Storage and validation; `set_table`, `rename_table`,
     `rename_table_column`, `delete_table`, the `tables` query.
   - Dependencies, atomic re-evaluation on a table edit, renames and
     refused deletes. Undo, `diff`, copies between documents; the
     `parameters`, `evaluate` and `formula_help` fields.
   - commands.md gets a section of its own.
   - Done when: model tests cover a table edit that changes a used value
     (only the features using it recompute) and one that changes none
     (nothing recomputes); an edit that would make a parameter fail is
     refused with nothing changed; renames rewrite expressions and
     limits; deletes are refused while the table is used; save and load,
     undo and redo, and the diff rows; a configuration row that sets
     only the key.
3. **Table CSV import and export** (model and api). Depends on 2.
   - `table_csv`, `table_csv_preview` and `import_table_csv`.
   - Done when: model tests cover a round trip with every unit kind, a
     `;`-separated file with decimal commas, units in cells, inferred
     columns, renamed column names, refused cells, and an import refused
     because a used key disappears.
4. **Tables tab in Edit Parameters and Formula Editor completion**
   (app). Depends on 2 and 3.
   - Done when: a UI test creates a table, types cells (a wrong one
     stays red with its reason), drives a dimension through XLOOKUP,
     edits a cell and sees the body follow, imports a CSV through the
     preview, and completes `Screws.` in the Formula Editor; the user
     guide has a "Tables" part under Parameters and expressions;
     app/COMMANDS.md is updated.

## Open questions for the owner

Answered on 2026-10-27: the owner took every recommendation.

1. **Cells: constants or expressions?** Recommended: constants. A table
   then stays data, has no dependencies, and cannot form cycles; derived
   values belong in parameters.
2. **One namespace for tables and parameters?** Recommended: yes. `Screws`
   is then always one thing, in errors and in completion.
3. **MATCH's default mode: exact (0) or the spreadsheet default (1)?**
   Recommended: exact. The spreadsheet default silently picks a wrong
   row; XLOOKUP is exact by default too.
4. **Texts in lookups: case-sensitive or not?** Recommended: case-sensitive,
   as `=` already compares texts in MitCad (rule 28). Spreadsheets ignore
   case, so `'m5'` would not find `M5`.
5. **Where are tables edited: a Tables tab in Edit Parameters, or a
   dialog of their own?** Recommended: the tab, next to the parameters
   that use the tables, and available during sketch and feature edits.
6. **Do configurations stay separate from tables?** Recommended: yes.
   Configurations are design variants for libraries; tables are data for
   formulas; a configuration sets a key and the lookups do the rest.
7. **Lookups on two keys?** Recommended: not now. A combined key column
   with `&` covers it; a dedicated function can come if real tables need
   one.

<!-- SPDX-License-Identifier: MIT -->
# Configurations and configuration rules (design note)

Design note for configurable designs (mitcad#797). Status: **agreed with
the owner on 2026-10-27**. The model of options, rows and rules (the
`values` limit, open and closed tables, the migration, the cascade,
`configure` and Check Rules) is implemented (mitcad#798, mitcad#799;
[commands.md](../core/model-api/src/commands.md#configurations-options-rows-and-rules)),
and so are the options of inserted components (mitcad#801,
[commands.md](../core/model-api/src/commands.md#options-of-inserted-components));
the rest is not yet. It builds on
the configuration tables (mitcad#64), text parameters (mitcad#664),
parameter limits (mitcad#665), the parameters CSV (mitcad#666) and the
table lookups (mitcad#667, [table-lookups.md](table-lookups.md), whose
recommendations the owner accepted on the same day).

A configurable design stands for a family of products. The user picks a
value for each **option** (Color, Length, Size), **rules** remove the
values and combinations that are not allowed, and named **rows** are
presets of option values. The same choice is made in the design itself,
when a library part or another design is inserted, and in BUILD.

```text
Options   Color  ∈ {'Black'; 'White'; 'Red'}
          Tables ∈ {0; 1}
          Power  ∈ {0; 1}
          Length   300 mm … 2000 mm, step 10 mm

Rule "Black is short"     when Color = 'Black'   then Length max 1000 mm
Rule "Power needs tables" when Tables = 0        then Power only 0

Row  "Standard"   Color 'White', Tables 1, Power 1, Length 1200 mm
```

## Decisions so far

Owner, 2026-10-27:

1. **Options and rules, not only listed rows.** The user picks option
   values. Rules filter what is allowed. Rows are optional presets. A
   table can still be **closed**, in which case only its rows are valid
   combinations. A closed table works like today's tables.
2. **Everything goes through parameters.** An option is a user
   parameter. A body's appearance and material, a feature's suppression
   and the visibility of a body or an occurrence can follow an
   expression of parameters. A configuration only ever sets parameters,
   so BUILD, tables, the CSV and rules all see one mechanism, and the
   bindings also work in a design without configurations.
3. **The first version configures** parameters (value and
   min/max/step), appearance and material, suppression, visibility, and
   the options of inserted components.
4. **Rules hold everywhere**: in the Configurations window, in Change
   Parameters, the CSV import, applying a row, inserting a part, BUILD and
   the commands (MCP, CLI).
5. **A broken rule is repaired and reported.** Options have an order.
   After a change, the later options that are no longer allowed move to
   the nearest allowed value, and a notice names the rule. A single
   allowed value is chosen by itself.
6. **The rule editor is a block editor** in the manner of Blockly and of
   the visual rule engines of commercial CAD tools, written natively in
   Qt (no QtWebEngine).
7. **The editing UI is its own Configurations window** with the tabs
   Options, Rows and Rules. The options of the active configuration can
   also be chosen without opening the window.
8. **Inserted components from outside** (library parts, BUILD parts,
   linked designs) have options of their own. A design's own components
   follow the design's options.
9. **Tables and configurations meet** in three places. An option's
   values can be a table column. A rule can require that the combination
   is a row of a table. Rows can be generated from a table's key column.
10. **Rules only narrow.** They restrict options and reject combinations,
    and never set the value of a computed parameter. Computed values come
    from expressions (`if`, lookups).
11. **A bound option of an inserted component that breaks the
    component's rule** gives the component an error status. The parent's
    change is not refused.

## Options

An option is a user parameter that the configuration names, in choice
order:

```json
"configurations": {
  "options": ["Size", "Color", "Length"],
  "closed": false,
  "default": "Standard",
  "rows": [...],
  "rules": [...]
}
```

- An option's expression is a constant: a number with a unit, `0` or `1`,
  or a text (`'Black'`). Choosing a value writes that constant. An option
  whose expression is a formula is **driven**. The Configurations window
  shows it as driven, and the cascade below does not move it.
- **Allowed values** are a new limit next to `min`, `max` and `step`
  (mitcad#665). `values` is a list of expressions (`["'Black'",
  "'White'"]`, `["300 mm", "500 mm", "800 mm"]`) or a table column
  (`"Colors.Name"`, mitcad#667). It is stored and treated like the other
  limits: in the design file, in the `limits` of the `parameters` query,
  in undo, in the CSV, and in renames. It can be used on any parameter,
  not only on an option.
  - For a number, a value outside the list snaps to the nearest listed
    value (`limited: "values"`), or is refused when the parameter is
    strict.
  - For a text, a value outside the list is always refused, since a text
    has no nearest value.
- **Domain**: what an option can be before rules. This is a list (its
  `values`), or a range (its `min`, `max`, `step`), or free.
- Options of a library component replace today's `selectors`. A
  selector was a text only the picker knew about. An option is a
  parameter that expressions can use. For example, `XLOOKUP(Size;
  Screws.Size; Screws.dk)` makes a row set only the key, as
  table-lookups.md proposes.

## Rows

A row is a named preset of option values. It keeps what rows have today:
`designation`, `part_number`, `supplier_url`, and a stable `name` that
BUILD (`build.rows`), `derived_part.config` and `insert_component`'s
`library.config` refer to.

- **Open table** (`closed: false`): a row sets options only. Any
  combination that the rules allow is valid. A combination that matches
  a row takes that row's designation and part number. Any other
  combination takes the library's designation pattern with the option
  values in braces (`ISO 4762 M{Size}x{Length}`), as BUILD writes `{L}`
  today.
- **Closed table** (`closed: true`): only the rows' combinations are
  valid, as in today's tables. A closed table's rows may also set other
  parameters (`parameters`, today's form), so that existing libraries
  keep working without tables.
- A row that breaks a rule is a problem of the table (the Rows tab marks
  it with the rule's name), as a row with a wrong unit is today.

**Migration.** A file whose table has `selectors` opens as a closed
table. Each selector becomes a text parameter of that name (the next
free name, such as `Size2`, if a parameter already has it, with a
warning). Each row's `select` becomes its values for those parameters.
Row names, the default and the other parameters stay as they are. The
file is written in the new form from then on. Library components keep
working without being republished.

## Rules

```json
"rules": [
  {"name": "Black is short", "enabled": true,
   "when": "Color = 'Black'",
   "then": [{"option": "Length", "max": "1000 mm"}]},
  {"name": "Power needs tables",
   "when": "Tables = 0",
   "then": [{"option": "Power", "values": ["0"]}]},
  {"name": "Made sizes",
   "then": [{"in_table": "Made", "columns": {"Color": "Color", "Length": "Length"}}]},
  {"name": "No red XL",
   "when": "and(Color = 'Red'; Size = 'XL')",
   "then": [{"forbid": "Red is not made in XL"}]}
]
```

- A rule has a `name` (unique), `enabled`, an optional `comment`, an
  optional `when` (a condition in the expression language, true when
  left out), `then` and an optional `else`.
- **Conditions** are expressions of the existing language (`=`, `<>`,
  `<`, `and`, `or`, `not`, `if`, text comparison, lookups). They may use
  any parameter, not only options. Renames rewrite them as they rewrite
  expressions.
- **Actions**:
  - `values` (allow only these) and `exclude` (not these) restrict an
    option to a list. A list may be a table column.
  - `min`, `max` and `step` restrict an option's range. They are
    expressions in the option's unit.
  - `require` (a condition that must hold) and `forbid` reject the
    combination. Both take a `message`.
  - `in_table` requires the combination to be a row of a table. It names
    the table (mitcad#667) and maps options to columns.
- **Rules only narrow.** An option's allowed values are its domain
  intersected with every active restriction. The order of the rules
  therefore does not change the result, and two rules cannot fight over
  a value. When the intersection is empty, the problem names the rules
  (`Black is short and Long only leave Length no value when Color =
  'Black'`). Setting a value is the same as allowing only one value.
- **Rules do not take part in evaluation.** Options are chosen
  constants. Rules check the evaluated parameters and change only the
  constants of options, so rules can never form a dependency cycle. Two
  rules can point in opposite directions (Color restricts Length, and
  Length restricts Color).

### The cascade (decision 5)

After any change of a parameter (an option, a row, another parameter
that a condition uses, a CSV import, a table edit), the model goes
through the options in order:

1. An option keeps its value when that value is allowed, given the
   options before it and the parameters.
2. Otherwise it moves:
   - a number with a range: to the nearest allowed value, by the step
     rule of mitcad#665;
   - a list: to the nearest listed value (for a number), else to the
     default row's value when it is allowed, else to the first allowed
     value;
   - a single allowed value: to that value.
3. A driven option is not moved. A rule that it breaks stays broken.

When every rule then holds, the change and the moves are **one undo
step** (`Set Color to Black`). The answer lists the moves (`Length 1500 mm
→ 1000 mm: Black is short`), and the UI shows them as a notice. When a
rule still fails (a `require`, a `forbid`, an `in_table`, or a driven
option), the change is **refused** with the rule's name and message, as
a strict limit refuses a value.

**Allowed values for a chooser.** The `configure` query gives each option
its allowed values, given the options before it. For a list, a value is
allowed when the cascade from it ends with no broken rule. A range is the
intersection of the active `min`, `max` and `step`. Every chooser uses
this query: the Configurations window, the active configuration bar, the
part pickers and MCP. The final check of a whole combination is always
exact.

**Check rules.** For finite domains (up to 100 000 combinations), the
Rules tab can report the following: option values that are never
allowed, rules that never apply, empty intersections, rows that break a
rule, and combinations that the cascade cannot repair. Larger domains
are sampled, and the report says so.

## Bindings: appearance, material, suppression, visibility

Each of the following can follow an expression (decision 2):

| What | Field | Result of the expression |
|---|---|---|
| A body's appearance | `appearance_expr` | text: an appearance id, else a unique appearance name |
| A face's appearance | `appearance_expr` of a face assignment | as above |
| A body's material | `material_expr` | text: a material id or a unique name |
| A feature's suppression | `suppressed_when` | condition |
| A body's or an occurrence's visibility | `visible_when` | condition |

- The UI writes a mapping as `switch(Color; 'Black'; 'matte_black';
  'White'; 'gloss_white'; 'default')`. The Formula Editor edits any
  other form.
- A binding takes the place of the value set by hand. Setting the value
  by hand removes the binding, in one undo step.
- An appearance or a material that the result names but that does not
  exist gives a warning on the body, which then has the default look, as
  an unknown id has today.
- Suppression that follows an expression recomputes from that feature,
  as `suppress_feature` does. Appearance, material and visibility
  recompute nothing.
- Parameters that a binding uses cannot be deleted, renames rewrite the
  binding, and `dependencies` lists them.
- Bindings are seen at a glance (mitcad#862): a bound browser row's fx
  badge counts its bound properties (`fx 3`) and shows a card of what they
  follow in words (`Color: Black → Black plastic, White → White plastic`,
  `when Holes = Yes · hidden now`), with Edit… per line; the Configurations
  window's Bindings tab lists every binding of the design, filtered by
  the option that drives it, and the Options tab says how many bindings
  use each option.

## Inserted components (decision 8)

A component inserted from a library or from a linked design, or derived
by BUILD, has `parameters`: its options' values, and other user
parameters of the source where a closed table allows it. A value is an
expression of the **parent** design's parameters:

```json
{"library": {"component": "shelf-board", "config": "Standard"},
 "parameters": {"Length": "Shelf_W - 2 * Wall", "Color": "Color"}}
```

- `config` (a row) gives the starting values, and `parameters`
  overrides them. `derived_part.parameters`, which BUILD's cut length
  `L` uses today, is the same field.
- Options belong to the inserted **component**, not to an occurrence.
  All occurrences of the component share the options. A second
  configuration is a second component, which the part picker makes, as
  it does for a second row today.
- The source's rules hold. The cascade moves constant values of the
  component's options and reports the moves. A value bound to the parent
  that breaks a rule is not moved. Instead, the component gets an error
  status that names the rule, as a failed feature does, and the parent's
  change itself is not refused.
- The source's bindings follow the component's values, as they do in
  the source itself (mitcad#873). A row of a library part can thus
  suppress a feature that another row uses.
- The parts list and the designation take the combination's row, or
  else the pattern with option values (see Rows).

## Tables and configurations (decision 9)

- An option's `values` can be a table column (`Screws.Size`). A table
  edit changes the chooser and runs the cascade.
- The rule action `in_table` lists allowed combinations as rows of a
  table instead of a chain of rules.
- **Generate rows**: a command makes one row per value of a key column
  (`Screws.Size`). The rows are named by the key, and each row sets the
  option to that key. Existing rows of the same name are updated, not
  duplicated, in one undo step.
- Tables and configurations otherwise stay separate, as in
  table-lookups.md: a row sets the key, and lookups do the rest.

## UI

**Configurations window** (`solid.configurations`, Configurations… beside
Change Parameters in SOLID > MODIFY and the Edit menu; its own window,
decision 7):

- **Options**: the options in choice order (drag to reorder), and **Add
  Option** (an existing user parameter, or a new one). Each option has
  its domain: a list editor, or a table column, or min/max/step. Each
  option also has a default and a comment. A driven option is shown as
  such.
- **Rows**: a sheet with one row per preset and a column per option,
  plus Designation, Part Number and Supplier URL. It has the Closed
  check, Generate Rows from Table…, and Apply. A cell or row that breaks
  a rule is marked with the rule's name in the tooltip.
- **Rules**: the list of rules (name, enabled check, a problem mark),
  the block editor for the selected rule, and Check Rules with its
  report.

**Block editor** (native Qt, `QGraphicsView`):

- The palette has these groups:
  - **Rule**: when / then / else.
  - **Logic**: and, or, not; compare (=, ≠, <, ≤, >, ≥); is one of.
  - **Values**: an option or a parameter, a number with a unit, a text,
    a table column.
  - **Actions**: allow only, exclude, min, max, step, require, forbid,
    combination in table.
- Blocks snap together by shape, as in Blockly. A value block only fits
  a value socket, and an action block only fits a then or else slot.
- A rule is stored as JSON with its conditions as expression text. The
  editor turns each expression into blocks. Any part that has no block
  is shown as a formula block, edited in the Formula Editor. Every rule
  therefore opens in the editor, and nothing is lost.
- **The rule as text** (mitcad#965) is always in sight under the blocks
  and is edited too: a line per part (`when Color == 'Black'`, `then
  Length max 1000 mm`), read back and saved as a block edit is, a line that
  does not read marked with its place and reason. **Add** (mitcad#966)
  offers templates, each a complete rule made from the design's options.
- Each edit is one undo step. Blocks can be focused and edited with the
  keyboard (arrows, Enter, Delete), as well as with the mouse.
- The editor works in the Floating and Docked layouts on Windows, Linux
  and macOS.

**Active configuration**:

- A **Configuration** bar in the browser has a chooser per option: a
  combo for a list, and a field and a slider for a range. A combo
  chooses a row preset.
- The choosers show only allowed values (from the `configure` query),
  and a notice lists the cascade's moves.
- In Change Parameters, options carry an option mark, and the rules'
  restrictions are marked as limits are. The Tables tab of mitcad#667 is
  in Change Parameters, as the design note proposes.

**Bindings**:

- The Appearances and Materials windows (Appearance… and Material… of a
  body's context menu) have Varies with in their assignment panel: a table
  from option values to appearances or materials, filled from the
  window's own list (mitcad#861; it replaced mitcad#809's Follow Option
  dialog).
- A feature's context menu has Suppress When….
- A body's or an occurrence's context menu has Show When….
- Each one is an expression field with the Formula Editor.

**Part pickers and BUILD**:

- The picker's size table (`PartPicker`, the BUILD pickers) shows a
  chooser per option in order, filtered by the rules, instead of only
  the rows.
- Free numeric options are fields within their allowed range.
- A closed table looks as it does today.

Profile planning and BUILD preview retain text constants and text
expressions bound to the parent design, and use source-declared units for
non-length numeric options. Lengths keep the existing millimetre fallback
for bare expressions, matching derived-part slots; explicitly unit-bearing
expressions retain their units. The source's configured geometry supplies
the preview's frames, bounds and designation. As with a committed derived
part, constants may move through the source's rule cascade; a conflicting
parent-bound expression is refused with the rule's reason. Every preview
validates that distinction before consulting its cache, whose key contains
the normalized typed values after the cascade. Text options are never
omitted from planning or read as length expressions.
The existing numeric-only source-state helper remains available to other
BUILD operations.

A repaired cut length is also the reported preview length, including an
automatic ray suggestion. The plan keeps moved constants in the shared
cascade's expression form, as component-option edits do, so the saved
component and parts list agree with its geometry. A geometry-bound length
uses the committed feature's exact target calculation, including Face,
Centre or Through and the added gap. Planning measures the target, then
rebuilds once at that measured length with its value pinned; a binding
that changes its own placement is refused. Preview configuration retains
that pin and never repairs the geometric measurement as a free constant.

## Commands and queries (for commands.md)

| Command / query | Change |
|---|---|
| `set_configurations` | The new form, with `options`, `closed`, `rows` and `rules`. It refuses the table with the first problem, as today. It reads the old form with the migration above. |
| `configure` (command) | `values` ({option: value}, partial). It runs the cascade and answers `changed`, `moves` (option, from, to, rule) and `refused` (rule, message). Undo step `Configure Color = Black`. |
| `apply_configuration` | A row. Its answer adds `moves`. |
| `configure` (query) | `values` (partial, optional; else the current values). For each option it answers `value`, `allowed` (a list, or `min`/`max`/`step`), `restricted_by` (rule names), `driven`, and `row` (the matching row or null). It also answers `problems`. Nothing changes. |
| `check_rules` (query) | The report of Check Rules. |
| `generate_configuration_rows` | `table`, `column`, `option`. Undo step `Generate Rows from Screws`. |
| `set_body_appearance`, `set_body_material`, `set_face_appearance`, `suppress_feature`, `set_body_visible`, `set_occurrence_visible` | Each takes `expression` instead of the value. A value set by hand removes the expression. |
| `set_parameter`, `add_parameter` | `values` as a new limit. |
| `insert_component`, `derived_part` | `parameters` (expressions of the parent's parameters). |

MCP exposes the same commands. The reference tool gets a Configurations
section.

## Other CAD tools

- **Configuration tables with typed columns** (a common concept): rows
  are variants, and the columns are parameters, suppression, appearance,
  material, visibility, inserted configurations and properties.
- **Visual configuration rules** (a commercial CAD tool, 2026): a block
  engine with rule, logic, getter and setter blocks. Rules prevent
  invalid combinations, adjust dependent values, enable or suppress
  features, and set a minimum and a maximum. Values that break a rule
  are flagged and reverted, and options that are not allowed disappear
  from the driven columns.
- MitCad takes the block editor and the "repair and report" behaviour.
  MitCad differs in three ways:
  - Options rather than listed rows: an open table is the default.
  - Typed columns become bindings to parameters.
  - Rules only narrow, so their order never matters.
- The `.f3d` configuration table is described in
  `core/import/docs/f3d-learning.md`. No file of the census has one,
  so importing it is not in scope (see below).

## Not in scope

- Different configurations of a design's own component in two
  occurrences: each occurrence would need its own evaluation of the
  component.
- Importing `.f3d` configuration tables and rules, until the corpus has
  a design with them.
- Rules that set values outside options (setters of computed
  parameters): use expressions with `if` and lookups.
- A general constraint solver: the cascade and Check Rules are
  deliberately simple and explainable.
- Configurable properties other than designation, part number and
  supplier URL. Text parameters can carry any other text.

## Implementation tickets

In dependency order. The #667 tickets are those of table-lookups.md.

1. **Model: configuration options, allowed values and the new table
   form.** Covers the `values` limit, options, open and closed tables,
   migration of `selectors`, `set_configurations`, the `configure`
   command and query without rules, docs and tests. Size l.
2. **Model: configuration rules.** Covers rules in the file, actions,
   the narrowing semantics, the cascade with moves and refusals,
   enforcement in every path that sets parameters, renames and deletes,
   `check_rules`, and undo. Size l. Needs 1.
3. **Model: appearance, material, suppression and visibility from
   expressions.** Covers the fields, commands, recompute, the file,
   queries, renames, and the dependency checks. Size l. Independent.
4. **Model: options of inserted components.** Covers `parameters` on
   library and linked inserts, the same field as `derived_part`
   overrides, the component's error status, and the parts list and
   designation. Size l. Needs 1 and 2.
5. **Expressions: `Table.Column`, XLOOKUP, MATCH, INDEX** (mitcad#667
   ticket 1). Size m.
6. **Model: tables in the design and the file** (mitcad#667 ticket 2).
   Size m. Needs 5.
7. **Tables as CSV** (mitcad#667 ticket 3). Size s. Needs 6.
8. **UI: the Tables tab in Change Parameters** (mitcad#667 ticket 4).
   Size m. Needs 6.
9. **Model: tables in configurations.** Covers option values from a
   column, the `in_table` action, and generating rows. Size m. Needs 2
   and 6.
10. **UI: the Configurations window (Options, Rows) and the active
    configuration bar.** Size l. Needs 1, and 2 for the filtering.
11. **UI: the rule block editor** (native Qt, Rules tab, Check Rules).
    Size l. Needs 2 and 10.
12. **UI: Follow Option, Suppress When and Show When.** Size m. Needs
    3.
13. **UI: options in the part pickers and BUILD pickers.** Size m.
    Needs 4 and 10.

# The .f3d import: sheet metal

Sheet metal features of the file replayed as native sheet metal. Part of
the [.f3d import](f3d.md) notes; the [import
documents](../README.md#documents) list the other topics.

## Sheet metal

Sheet-metal items (mitcad#211) of the `.f3d` and the `.ipt` import
(`core/import-sheet/src/sheet.rs`) come from the decoders as the dump's sheet items
(`SCHEMA.md`, *Sheet metal*) and replay as MitCad's native sheet
features (`commands.md`, *Sheet rules and base Face*, *Folded sheet
features*), each checked against its own history state as any other item:

- **A face** (the `.f3d` base flange, the `.ipt` face that makes a new
  body): `sheet_face` of its sketch's regions (those its decoded loops
  select, else those of its measured profile), the material on the side of
  the sketch's plane the decoder gives (the reference side `front` along
  MitCad's sketch normal, `back` against it), with the rules: the first
  face of a rule of the file makes a `sheet_rules` feature named
  `Sheet rules <rule>` in the face's item (its own timeline row, before
  the face), and later faces of that rule and side use it. The rules'
  values are the file's parameters (the thickness parameter, the bend
  radius and reliefs as expressions of it), so a changed thickness
  rebuilds the sheet. The rule's inputs the decoder did not read (the
  relief shapes; the corner relief's depth, which the files have no field
  for and MitCad does not use) make the item `inferred`, with a note.
- **A flange on one edge** (`sheet_flange`): the `.f3d` edge flange's edge
  is found in the replay by its geometry (its names found in the
  history), and `sheet_reference` names the panel boundary it lies on;
  the flange turns towards the side of the face the edge was selected on,
  to the other side when its flip byte is set; position (`inside`,
  `outside`), length datum (`inner`, `outer`, `tangent`), angle, length
  and radius are read (`read`). The `.ipt` flange's edges are read by
  their topological names (mitcad#242, `core/ipt/README.md`, *Topological
  names*): the decoder finds each in the ASM history state the flange
  starts from and gives its ends; the import finds the edge of its replay
  there and `sheet_reference` names its boundary. Its position and length
  datum are told from the stored path (which must start towards one of
  the edges read) and the length's points (`inferred`: the file's own
  fields for them are not found); a width extent of the file (offsets from
  the edge's two ends) becomes the flange's `offsets`, the start offset
  the one at the boundary's axial start.
  An `.f3d` flange on several edges is one `sheet_flange` per edge in its
  item. Bend position `tangent` (4, the bend tangent to the side face) is
  MitCad's position `side_tangent` and height datum 3 its length mode
  `outer_tangent` (mitcad#242: both measured from the bend's outer extent,
  defined up to the 180-degree hems) *(verified: the hems on two edges and
  the 90-degree flanges of both designs give their states)*. Widths are
  measured from the edge's midpoint: `EdgeWidth` both ways, `EdgeWidth_1`
  towards the file's edge's start point and `EdgeWidth_2` towards its end,
  as the flange's `width` (`middle`), which may be wider than the edge
  *(verified: the partial flanges of both designs, one wider than its
  edge, give their states)*.
- **A hem** (`.f3d`, mitcad#1364): an open or flat hem folds its wall back
  over the panel in one bend of 180 degrees, so it is a `sheet_flange` of
  angle 180 degrees on its edge (found and turned as an edge flange's),
  its inside radius half the hem's gap (an expression of the gap's
  parameter), its length the hem's length measured from the bend's outer
  surface (`outer_tangent`), at position `side_tangent` (`read`, with a
  note) *(verified: the bodies' volumes of the reference models' open
  hem, gap 2 mm, and flat hem, gap 0.02 mm, both on a 2.5 mm sheet: the
  flat hem's gap is its inside diameter too)*. The other forms
  (mitcad#1378) fold over the panel at the same position, their lengths
  from the same plane:
  - a **rolled** hem is one bend of its angle (past 180 degrees) and
    radius with no wall: a `sheet_flange` of that angle and radius, its
    length 0 from the bend's tangent line (mode `tangent`) *(verified: the
    reference model's rolled hem, 270 degrees, radius 3 mm)*;
  - a **teardrop** or **rope** hem is a `sheet_flange` whose `hem` is that
    form with the hem's gap, its radius the hem's radius, its length the
    hem's: the teardrop's one bend passes 180 degrees and its wall comes
    back so that the inner corner of its end lies a gap above the panel and
    its outer corner at the length; the rope's bend passes 180 degrees by
    as much as a second bend of the same radius turns back, its wall a gap
    above the panel, parallel to it, ending at the length *(verified: the
    reference models' teardrop and rope hems, radius 2 mm, gap 1 mm,
    length 10 mm: their stored faces and volumes)*;
  - a **double** hem is a `sheet_flange` whose `hem` is `double` with the
    hem's gap and setback, every bend's inside radius half the gap: two
    bends of 90 degrees with a wall of gap + thickness between them, a wall
    over the panel to the 180-degree fold, whose outer surface lies at the
    hem's length, and a wall back under it ending the setback short of the
    first wall's inner surface *(verified: the reference model's double
    hem, gap 2 mm, length 10 mm, setback 1 mm: its stored faces and
    volume)*.
- **A cut** (`.ipt`, mitcad#242): `sheet_cut` of mode `straight` (the
  file's cut with neither of its two sheet options set goes straight along
  the sketch's normal, through a bend too) with the file's extent and
  direction, when it goes into the material at least the thickness
  *(verified: the two cuts reaching a bend give their states, one within
  7e-7 of the volume)*. Where it crosses a bend the model keeps it as an
  overlay of the sheet. A cut with its slot-31 option (across the bends,
  mitcad#280) is a developed `sheet_cut` with every bend opened, the panel
  whose surface holds the sketch staying *(verified: the one such cut)*;
  one with the slot-33 option is not translated.
- **Ordinary items on a sheet** (mitcad#242): an extrusion, hole, mirror,
  library feature or fallback whose result is the sheet's material changed
  through its thickness keeps the sheet valid (the model's sheet-domain
  adapter, `commands.md`), so the sheet items after it replay natively.
- **Corner rounds and chamfers** (`.ipt`): each edge set's edges read by
  their names (mitcad#242) and found in the replay as an external dump's
  fingerprints are, each a corner of a panel's contour by
  `sheet_reference`, as `sheet_corner_round` / `sheet_corner_chamfer`
  (`read`). An edge not found by its name makes the item fall back with
  the edge and the reason; the history state is not searched for others.
- **Bends** (`.ipt`, mitcad#242, mitcad#280): its two edges read by their
  names and found in the replay. Edges of parallel panels of two sheet
  bodies with the bend record's kinds 3 and 5 are a double bend:
  `sheet_bend` of port mode `double`, two 45-degree bends, the radius its
  parameter, the fixed edge the one the stored axis and direction point
  away from its panel's material, the body the file's bend works on kept
  *(verified: the corpus's four bends, each joining a strip of a joined
  face to a flange's wall)*. Edges of panels of one sheet body that meet at
  no seam would close a loop and are refused; other kinds, and a single
  bend between two bodies (its angle and sides not decoded: none in the
  test files), are not translated.
- **Folds** (`.ipt`, mitcad#280): enumeration 1 with both first booleans
  false is `sheet_fold` at position `edge` on the sketch line lying on the
  decoded line, of the panel whose surface holds it, the material right of
  the line (seen along the sketch's normal) staying and the other side
  turning against the sketch's normal; other options are not decoded
  (`core/ipt/README.md`).
- **A face joined to the sheet** (`.ipt`, mitcad#242): its material lies
  along the decoded direction from its sketch's plane, against it for the
  plate's side 1. Where that slab is a panel's (the sketch on the panel's
  front or back surface), the face is a `sheet_face` joined to the sheet
  body, which adds its regions to the panel it touches (`read`) *(verified:
  the two joined faces the corpora reach with a valid sheet; #211 took
  them for double bends because it did not read the side)*. A face whose
  bend part lists nothing and whose material lies in no panel's plane
  touches no material of the sheet: its regions are new sheet bodies with
  the sheet's rules (mitcad#280) *(verified: the corpus's joined face of
  two strips, which double bends join to the sheet after it)*. A face bent
  onto the sheet (its bend part's items not decoded) falls back with that
  reason. No pairs of edges are offered as candidates.
- **Holes** through a sheet: MitCad's simple hole through a planar panel
  cuts the sheet's material domain (`commands.md`, *hole*), so the body
  stays a sheet for the items after it.
- A sheet item on a body whose sheet is no longer valid (an earlier item
  fell back and its base feature replaced the body, or an ordinary
  feature changed it) is not translated: it falls back, its note naming
  the item that first took the sheet meaning away (`its sheet body … is no
  longer a valid sheet: <item> took its sheet meaning away`; the sheet
  state keeps that first item, `commands.md`, *sheet_state*). It is never
  replayed as an ordinary cut, rounding or chamfer instead.

The stream's records ([*From the descriptions to the decoder*](f3d-undecoded.md#from-the-descriptions-to-the-decoder), cadmpeg's
*BaseFlange*, *EdgeFlange* and *Hem* layouts; `core/f3d-design/src/build/sheet.rs`):

| Record | Fields read |
|---|---|
| Base flange (`7AFA0A65…`, `Face<n>`) | its profile's sketch and loops; its thickness (f64 at offset 123, the rule's) *(verified: both designs)* |
| Edge flange (`7AFA0A65…`, `EdgeFlange<n>`) | found by the marked angle and height owners `p` (the holders of `FlangeAngle` and `FlangeHeight`): before them `u32` bend position (1 outside, 2 inside, 3 adjacent, 4 tangent to the side face), `u32` edge count, a wrapper reference per edge, the settings reference, `u32` height datum (1 inner, 2 outer, 3 tangent to the bend's outer surface *(verified, mitcad#242)*); after them a `u32`, at `p + 26` a flip byte (0, 1) *(verified: two flanges of each design, one flipped, give their states)*, at `p + 37` the inside bend radius (f64, cm); width owners `EdgeWidth`, `EdgeWidth_1`/`_2` (distances from the edge's midpoint, mitcad#242) |
| Hem (`7AFA0A65…`, `Hem<n>`, mitcad#1364) | its parameters by their roles (`HemGap`, `HemLength`, `HemRadius`, `HemAngle`, `HemSetback`), their holders the marked owners of the fixed section at base `b`: `u32` bend position (4, tangent to the side face *(seen: every hem)*), `u32` edge count 1, the edge wrapper and settings references, the owners from `b + 41`; at `b + 66` (two owners) or `b + 76` (three) a `u32` form (1 open, 2 rolled, 3 double, 4 teardrop, 5 rope, 6 flat *(ours: one hem of each form in the reference models)*), then a byte and the rule's inside bend radius (f64, cm); its edge as an edge flange's |
| Sheet-metal rule (`B42ED662…`) | expressions with their values in order (bend radius, relief width, relief depth, two more), five GUID texts, the corner relief size, the K-factor (f64, four bytes after the corner relief size's value), the material, the thickness text and a reference to the thickness parameter *(seen: one rule; the order of the relief fields is taken from their default expressions)* |

Edge recipes of some flanges have names with a nonzero word after their
operations (`0x04000102`, `0x04000022`, `0x04000042`): the recipe reader
reads it over, as the blobs' own names have a word there
(`core/f3d-decode/src/recipe.rs`; the design corpus comparison is
unchanged).

A flange whose trim reaches an earlier bend's seam (flanges on adjacent
edges, a flange on the side edge of a wall) trims that bend too, developed
(`commands.md`, *Folded sheet features*) *(verified: the `.f3d` designs'
two-edge flange on the walls' side edges and the `.ipt` multi-edge
flanges below give their states)*. An `.ipt` flange on several edges is
one `sheet_flange` per edge read by its name, in the file's order, with
the inputs its path gives (the edges on the surface the path's edge is
on); every edge must be read, or the flange is not translated
(mitcad#242; the candidate search among the panel's boundaries #211 used
until then is gone). A rounding or chamfer of the file whose every edge is a
corner of a valid sheet's panel is tried first as sheet corners (the same
geometry, the body stays a sheet).

Not translated yet, with the reason in each item's note:
- faces bent onto the sheet (their bend part's items not decoded);
- `.ipt` flanges on several edges with corner settings of their own (the
  corners their walls make, `core/ipt/README.md`, *Corner settings*);
- `.ipt` bends closing a loop of material, bends of other kinds, folds
  with other options or across holes (MitCad's fold needs one
  uninterrupted line), cuts with the slot-33 option;
- `.f3d` widths other than those above (an `EdgeOffset_1`), the `adjacent`
  position, bends and folds.


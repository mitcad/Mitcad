<!-- SPDX-License-Identifier: MIT -->
# Sheet metal: model and geometry contract

This is the implementation contract for mitcad#125,
starting with mitcad#205.
The parent remains open until the complete acceptance map at the end is
satisfied. Native folded modelling, Unfold/Refold and the derived Flat Pattern
are implemented through phase mitcad#208, the production Cut and the reliefs
through mitcad#209, the command UI and the Flat Pattern DXF through
mitcad#210; the import phase (mitcad#211) is in progress, see the
[import contract](#import-contract).

Phase mitcad#206 implements native `sheet_rules` and `sheet_face` definitions,
exact bounded planar domains, paired shape/state cache results and the
`sheet_state`/`sheet_rules` queries. Phase mitcad#207 adds editable Flange, Bend,
Fold, Corner Round and Corner Chamfer, exact cylindrical support surfaces,
paired thickness boundaries, tangent ports and `sheet_trim_domain`. Its
geometry tests exercise partial opening and cuts across both bend seams. The
native JSON details are in
[the document API](../core/model-api/src/commands.md#sheet-rules-and-base-face).
Phase mitcad#208 adds `sheet_unfold`, `sheet_refold` and the Flat Pattern item
(`set_flat_pattern`, `remove_flat_pattern`, the `flat_pattern` and `flat_point`
queries); see
[its API section](../core/model-api/src/commands.md#sheet-unfold-refold-and-flat-pattern)
and the [implementation notes](#implementation-notes-mitcad208). Phase
mitcad#209 makes `sheet_cut` the production Cut (several sketches and contours,
removed domains, split bodies) and adds `sheet_corner_relief`; see
[its notes](#implementation-notes-mitcad209). Phase mitcad#210 adds the Sheet Metal
command group, the browser's rules and Flat Pattern rows, the folded/flat
display and `export_flat_pattern_dxf`; see
[its API section](../core/model-api/src/commands.md#sheet-metal-command-ui-and-flat-pattern-dxf)
and [its notes](#implementation-notes-mitcad210). The leftovers of these phases
(corner reliefs without a common panel or at non-perpendicular bends, copies of
Flat Pattern items and split pieces, design diffs of Flat Pattern items) are in
[their notes](#implementation-notes-mitcad125-gaps).

## Scope and architectural boundary

A sheet-metal body has a constant material thickness, explicit fabrication
rules, labelled planar panels and cylindrical bends, and a correspondence
between its folded and developed material domains. Face, Flange, Bend, Fold,
Cut, Corner Round, Corner Chamfer, Unfold and Refold are separate editable
timeline features. A whole-body Flat Pattern is a derived browser item with
DXF export. Native modelling, the command UI and parametric `.ipt` and `.f3d`
import are all part of the parent scope.

The initial allowance law is a K-factor. Bend tables can be added as another
typed law later; they are not required in addition to K-factor support. The
initial surface family is planar panels and constant-radius cylindrical bends.
Conical, doubly curved, variable-thickness and stretched material, embossing and
arbitrary B-rep-to-sheet recognition are outside this contract. Unsupported
surfaces must produce a specific error, never an approximate flat pattern
reported as valid.

The [model architecture](architecture.md#the-model) remains intact:

- `core/model` owns parameters, definitions, semantic sheet state, references,
  timeline evaluation and cache policy. It does not inspect OCCT topology.
- `Kernel` receives resolved values and semantic construction specifications;
  `geometry` constructs and validates the named B-rep and trim correspondence.
  The CXX bridge transports data and errors. Shapes remain immutable.
- The application uses JSON commands and queries through the existing
  [command framework](../app/COMMANDS.md). It cannot maintain a second sheet
  model or derive an export by inspecting viewport triangles.
- `core/dxf` writes the evaluated flat contours and bend information. The flat
  result is not a detached sketch that can drift away from its source body.

Existing extrusion, Boolean, profile, split, transform and naming operations
are useful construction tools. An affine transform with a zero scale, a
projection of the folded silhouette, or rigidly moving faces while discarding
the bend zones does not implement unfolding.

## Units, rules and material orientation

Lengths are millimetres and angles are radians in resolved model/kernel data,
following existing APIs. User expressions retain their units and persistent
`ParamId` references. Rule fields are not `BodyAttributes`: visibility,
appearance and material metadata do not own geometric parameters.

`SheetRulesDef<P>` is a parameter-owning `sheet_rules` timeline feature. A Face
refers to an earlier rules feature. The browser's Edit Rules action edits that
feature and identifies every body using it. By default, creating a first Face
creates a private rule feature and the Face in one command checkpoint; an
explicit selection can reuse existing rules. Creating or editing a shared rule
must not silently fork it. Changing to independent rules is a separate explicit
action, with one Undo step. A missing or suppressed rule is an error.

| Field | Meaning and validation |
|---|---|
| `thickness` | Positive finite distance, greater than modelling tolerance. |
| `bend_radius` | Default inside radius; positive and resolvable by the kernel. A zero-radius mathematical crease is not silently substituted for a bend. |
| `allowance` | Initially `{type: "k_factor", k: P}` with finite, dimensionless `0 <= k <= 1`. |
| `reference_side` | `front` or `back`: the named material surface lying on the base sketch. |
| `bend_relief` | A typed rectangular, round or tear relief with dimensional defaults. |
| `corner_relief` | A typed relief and gap/size defaults for junctions of bends. |

The sketch's right-handed frame defines positive normal `n`. Front-to-back
material direction is `+n`: with `reference_side = front`, the front is at
offset 0 and the back at `+thickness`; with `back`, front is at `-thickness`
and back at 0. These are stable material labels, not a reclassification by
current outward normal. Flipping the reference side therefore relocates the
material relative to the sketch and invalidates dependent geometry.

For an oriented bend edge let `e` follow the stored edge direction and `m` point
outward from the stationary panel, chosen so `e cross m = n`. A positive bend
angle rotates the moving panel's outward tangent towards `+n`; a negative angle
rotates it towards `-n`. `angle` is this signed rotation from the flat state,
not the included angle between walls. The inner material side follows from the
sign and is stored in the bend correspondence. It is not guessed again after a
mirror, imported reference resolution or a topology reorder.

For inside radius `r`, thickness `t`, K-factor `K` and rotation `theta`:

```text
neutral radius Rn = r + K*t
bend allowance BA = abs(theta) * Rn
inner setback Si = r * tan(abs(theta)/2)
outer setback So = (r+t) * tan(abs(theta)/2)
```

For the initial cylindrical-bend topology, require
`angular_tolerance < abs(theta) < pi - angular_tolerance`. Setbacks
become singular as the angle approaches pi. A flange of up to and including
180 degrees (a hem) is supported since mitcad#242: its half-cylinder bend is
an ordinary chart, and its length and position are measured from the bend's
outer extent instead of the virtual sharps, which a 180-degree bend does not
have (see the [notes](#implementation-notes-mitcad242)). All frames must be finite and
orthonormal, and `r`, `t` and `Rn` must exceed linear tolerance. A positive
radius does not guarantee that a short flange fits.

These formulas define a manufacturing allowance, not an elastic deformation
model. For `K != 0.5`, folded solid volume need not equal developed area times
thickness. Tests must not impose that false invariant.

The side, sign, dimension and placement conventions here are MitCad design
decisions. Importers must translate source conventions explicitly; similarly
named fields do not imply equivalent measurements.

## Persistent definitions and evaluated state

Keep definitions separate from results. Proposed logical types are:

```text
RuleRef = FeatureUid
PanelId = { feature: FeatureUid, slot: u32 }
BendId  = { feature: FeatureUid, slot: u32 }
DomainRef = Panel(PanelId) | Bend(BendId)
SheetDefinition = { body: BodyUid, rules: RuleRef }

SheetState = {
  schema: 2, body, owner: ComponentUid, shape_generation,
  rules: RuleRef, rules_version,
  panels: map<PanelId, PanelDomain>,
  charts: material regions and current Plane/Cylinder embeddings,
  bends: map<BendId, BendDomain>,
  joins: labelled panel/bend adjacency,
  trims: named material-domain contours with provenance,
  opened: map<BendId, { by: FeatureUid }>,
  correspondence: chart boundaries and per-bend material-side parity
}
```

`PanelDomain` holds its material frame, bounded 2D region including holes, front
and back surface roles and boundary labels. `BendDomain` holds its two panel
boundaries, inside radius, signed angle, resolved allowance, finite axial
interval, `inner_is_front` parity and trimmed material chart. A contour crossing
a boundary has a shared identity on both charts; it is not two independently
fitted curves. A joined
sheet component can have several panels and bends but one coherent thickness
and rule binding. Disconnected pieces require distinct body identities and
sheet-state entries.

The tagged domain category is persistent: panel slot 0 and bend slot 0 of the
same feature are different domains. An opened bend retains its Bend identity
when its embedding becomes planar. `charts` is authoritative for reconstruction;
the existing `panels[].frame` query keeps the base Face reference-plane meaning.
A planar chart instead carries its resolved front frame and explicit
`front_to_back_sign` (`positive` or `negative`). At material depth `q`, its map is
`front_origin + u*x_axis + v*y_axis + q*sign*cross(x_axis,y_axis)`, for `0 <= q <= t`.
Resolve the rule's reference-side offset once when adapting Face. Opening a
bend can reverse this sign without reversing the material chart or applying
that offset again.

The body's B-rep and `Versioned<SheetState>` form one evaluated result. Add a
sheet change channel to `FeatureOutput`/cached `Output` and a sheet-state map to
recompute's environment/result. `Set`, `Remove`, `Invalidate` and rigid-transform
changes must be explicit; omission on an arbitrary body replacement must not
preserve old metadata. Final publication validates that every live sheet entry
belongs to a live body with the same owner and shape generation.

Apply each shape change and its sheet delta in the same recompute change loop,
after determining the final owner override. Extend `Env`, `Recomputed`, forked
results, memory-cache comparison/fingerprints, `StoredRead`, result-store
`VariantHead`/restore and memory accounting together. A shape-generation match
is required even when a reused `BodyUid` is unchanged; the ID alone cannot prove
that the sheet metadata belongs to the current geometry.

Rule outputs need their own versioned environment entries and dependency reads.
`Read::SheetRules(rule, version-or-missing)` and
`Read::Sheet(body, version-or-missing)` complement existing body/sketch/placement
reads. A missing input is still a read, so failure-cache entries recover after
Undo or reference repair. Parameter values, rules, reference side, semantic
domains, opened bend set, tolerance policy and construction algorithm version
participate in result fingerprints. Inherited values must be read from the
bound versioned rules output through the evaluation context, not from ambient
document defaults or a previously copied numerical constant.

### Names and topology changes

The creator and an allocated semantic slot identify a panel or bend. The slot
is recorded in the feature's settled definition/provenance mapping, survives
parameter edits and is never reused for a different input after deletion.
Initial allocation follows canonical source reference order; OCCT traversal,
floating-point face ordering and a display label cannot allocate identities.
Reordering a selection does not rename the same bend. Ambiguous input edits
fail with the missing reference and candidate choices rather than binding to
the nearest face.

Use the existing name grammar with new roles, for example
`F3:sheet_front(0)`, `F3:sheet_back(0)`, `F4:sheet_bend_front(0)` and
`F4:sheet_bend_back(0)`. The bend's `inner_is_front` field identifies which
material side is inside. Boundary roles include their source segment or semantic
slot. Edges and vertices continue to derive their names from meeting faces.
Boolean history carries aliases and deterministic piece suffixes as it does
for existing features. A `PanelId`/`BendId` refers to the entire semantic domain,
including trimmed pieces; it is not an alias for one transient B-rep face.

Add a typed `SheetCurveId`/lineage record for domain boundaries generated by
bends, reliefs and cuts. The current sketch `SegmentKey` identifies sketch or
glyph curves; generated trims must not borrow unused sketch entity IDs. A
lineage records its producing feature, semantic operation slot, source
boundary IDs and parameter intervals. Recomputing a split may change intervals
and piece suffixes while retaining the originating curve identity.

A boundary reference identifies its panel, typed curve and, where needed,
source-piece selector and bounded parameter interval. Composite sketch aliases
can label several curves. An operation requiring one boundary rejects an
ambiguous alias rather than selecting the first curve. Geometry returns local
fragment tokens and proven source spans; the model allocates persistent IDs.
Source spans describe coverage, not an affine map to an output curve's parameter.
An exact conic-to-spline conversion may change that parameterization.

If a cut disconnects a sheet, retain the original `BodyUid` for the first piece
under the existing deterministic body-split convention and allocate other
bodies from the cutting feature. Publish a domain-to-body lineage map and move
the appropriate semantic domains to each output atomically. Preserve references
only when their target is unique; references to removed or ambiguous domains
fail. Every result stays in the input body's owning component. A semantic
selection is the pair `BodyUid + PanelId/BendId`: after a split, the same origin
slot can have domains in several bodies. Never resolve an ambiguous pair by
taking the first surviving face.

### Other operations and assemblies

| Operation | Required treatment of sheet state |
|---|---|
| Sheet feature | Publish new shape, domains and correspondence together. |
| Rigid body move | Transform all panel/bend frames and named geometric maps along with the shape. |
| Component/occurrence placement | Keep body-local sheet state unchanged; apply the placement only at the assembly boundary. |
| Component copy / Paste New | Copy the rule dependency closure by default; remap body, rule, parameter, feature, panel and bend references. Same-document rule sharing requires an explicit choice. |
| Body pattern/copy or mirror | Preserve semantics only through a sheet-aware copy/handedness adapter with deterministic names; otherwise invalidate the copied result explicitly. |
| Arbitrary Boolean, scale, shell, generic fillet or face edit | Invalidate affected sheet semantics unless a tested adapter updates the domains; never retain an old flat result. |
| Planar through Hole / contour edit | A sheet-aware adapter may translate it to the same material-domain cut operation. Until that exists, invalidation is explicit. |
| Remove body | Remove its sheet state and make its flat item unavailable. |

Invalidation is per actual affected `BodyUid`, not merely per feature-owning
component. This matters for assembly cuts and split outputs owned by another
component. Unchanged bodies retain their states. An invalidated body remains
ordinary solid geometry and reports the first operation that invalidated its
sheet meaning; it cannot continue through sheet commands or export a fabricated flat
pattern without an explicit supported reconstruction.

Sheet modelling edits a component definition. Multiple occurrences therefore
show the same edited sheet; UI selection must say so. Occurrence-specific sheet
editing is not implied. All incoming assembly selections carry a checked full
occurrence path. Convert once to the owning component's coordinates; reject an
ambiguous omitted path. A component-instance pattern shares one sheet state,
not one independently updated state per occurrence. Linked components remain
read-only. A flat result is not counted as another physical assembly instance.

Copying a component must collect rule dependencies outside its component
subtree; copying only subtree-owned features is insufficient. Insertion from
another document always remaps the required rule/parameter closure, even when
source and destination happen to have equal textual IDs. The copied rules must
precede their consumers in the destination timeline.

## Feature contracts

Each feature has its own JSON `type`, reference validation, parameter mapping,
dependency declaration and editable command. Features consume the previous
sheet state at their timeline position. They cannot refer to a later feature,
use themselves as a stationary panel, or silently fall back to all bends or
another body when a reference disappears.

### Face

`sheet_face` resolves closed coplanar sketch regions, including holes, against
a rules feature and creates material at the specified reference side. Touching
regions can form one connected sheet; separate islands create distinct sheet
bodies with independent flat items. Preserve internal labelled panel boundaries
when selected adjacent regions are intentionally distinct, even if the B-rep
merges their coplanar faces. Reject self-crossing contours, zero-width material
and thickness offsets which cannot produce a valid solid.

Analytic example: a 40 by 20 rectangle with a radius-2 circular hole and
`t = 1` has developed area `800 - 4*pi`, identical front/back outline, and
volume `800 - 4*pi`. Changing thickness changes volume and back offset while
retaining the panel and hole identities.

### Flange

`sheet_flange` selects a straight free boundary of a panel, the signed angle,
length and its measurement mode, inside radius or rule default, placement,
end offsets and relief overrides. The free straight wall length `Lstraight`
is derived from the entered `L`:

| `length.mode` | Measurement | `Lstraight` |
|---|---|---|
| `tangent` | Moving tangent line to free edge along the wall. | `L` |
| `inner` | Virtual sharp intersection of inner planes to free edge. | `L - Si` |
| `outer` | Virtual sharp intersection of outer planes to free edge. | `L - So` |

Position uses a signed distance along `m` from the selected boundary to the
stationary tangent line: `inside = -So`, `outside = -Si`,
`bend_line = -BA/2`. Thus the selected line is respectively the outer virtual
corner, inner virtual corner, or centreline of the developed bend allowance.
These definitions, and dimension arrows on the preview, are authoritative;
the words inside/outside alone do not define geometry.

Start/end offsets shorten the edge's oriented axial interval and must leave
positive usable width. The kernel checks `Lstraight > tolerance`, enough base
material for the selected setback, valid relief intersections, and collisions.
It reports which length/offset/radius fails; it does not clamp the user's value.

For `r=2`, `t=1`, `K=0.4`, `theta=pi/2`, `BA=1.2*pi`, `Si=2` and `So=3`.
An entered length 12 gives straight lengths 12, 10 and 9 for tangent, inner
and outer respectively. A stationary tangent length 30 plus a tangent flange
length 12 develops to `42 + 1.2*pi = 45.769911...`. Position offsets are
`-3`, `-2`, and `-0.6*pi`; test these independently from the length mode.

### Bend and Fold

`sheet_bend` works on two named planar panel domains and their registered
straight tangent ports: an unambiguous full-span boundary pair with compatible
thickness and parallel axes, or a labelled common virtual-sharp seam. A port
contains a panel ID, oriented domain boundary, axial interval and material-side
orientation. Arbitrary face tangent planes do not establish this pairing.
Introduce a finite-width cylindrical bend while holding the stationary panel
and moving exactly the detachable moving graph branch. The implementation
must cover both a labelled coplanar seam and a compatible pair of already
positioned faces. A second graph path between those branches is an error.
Missing overlap, incompatible radius/angle or a required trim outside selected
material produces a diagnostic; nearby faces are not automatically connected.

The native definition may supply a distinct `moving_body` for already positioned
ports. Both bodies must have the same owner and rules binding. Success preserves
the stationary body ID and consumes the moving body atomically; a failed merge
blocks both intended inputs. Omitting `moving_body` selects the same body.
The settled `port_mode` is `virtual_sharp` for a common coplanar seam or
`tangent_ports` for an already positioned pair. Parameter edits do not silently
change this interpretation or move a fixed pair to match a new radius.

`sheet_fold` uses a straight sketch line on a panel to create that partition
and bend. The line is resolved in the panel's material coordinates and has an
explicit stationary side. The bend-line placement convention is the same as
Flange. The moving connected region is determined from the adjacency graph,
not from a world-axis test. The line partitions one planar domain into two
panels and a `BA`-wide strip. Apply endpoint reliefs before verifying that the
moving side connects only through the new bend. A line ending inside material
without sufficient relief, curved/branched lines and inadequate material are
rejected.

For two coplanar regions totalling 60 mm across a full-width seam, a centred
Fold using the rule above consumes `BA/2` from each planar side. Fully developing
it again restores the original 60 mm outline. A Bend on an already positioned
90-degree pair instead measures its two tangent lengths plus `1.2*pi`; it must
not assume that the sum of its folded bounding-box dimensions is the blank
length. Both operations must be tested with reversed sign and stationary side.

### Corner Round, Corner Chamfer and reliefs

Corner Round and Corner Chamfer modify a named material contour through the
whole thickness. They are not a generic fillet on only one thickness edge.
A radius-2 round of an external square corner removes developed area
`4 - pi`; a 2 by 3 chamfer removes area 3. Internal contour corners and holes
have explicit orientation and must not be confused with external corners.
Corner references store the ordered incoming and outgoing semantic curves.
Chamfer's first and second distances follow those boundaries away from the
corner. Rounding or chamfering a concave hole corner reduces the void and adds
material; the corresponding external corner removes material.

Rectangular relief is a finite-width slot with a straight end; round relief is
a slot with a semicircular end, with its entered depth measured to the furthest
tip. Both enter the bend region far enough to separate the moving material
from adjacent stationary material. Validate width, depth, available material
and tangency. Tear relief is an explicit slit/topological separation policy,
not a Boolean with a numerically zero-width box. Where a valid solid needs a
finite clearance, the required gap is an exposed parameter and is exported;
the kernel cannot invent an undocumented manufacturing gap. The exact tear
construction must pass the early geometry proof before its UI is enabled.

Corner relief acts on the selected junction of labelled bends, and defaults
are evaluated from the rules rather than copied as untracked constants. Every
relief contour retains its producing feature and endpoint provenance through
unfolding, cuts and refolding. Tests cover all three shapes and short-flange,
overlapping-relief and disappearing-junction failures.

## Development, cuts and refolding

For a bend use an orthonormal local basis: `e` along its axis, `a` the starting
radial direction and `b` the direction of increasing angular coordinate. The
basis carries the chosen bend direction. Let `u` be distance along the neutral
strip, `v` distance along the axis and `q` distance from the inside surface.

```text
0 <= u <= BA, 0 <= q <= t, phi = u / (r + K*t)
folded(u,v,q) = centre + v*e + (r+q)*(a*cos(phi) + b*sin(phi))
local neutral coordinates = (u, v, q-K*t)
```

Material-side parity is essential. If `q_front` measures from front towards
back, then `q = q_front` when `inner_is_front`, otherwise `q = t-q_front`.
The neutral offset from front is correspondingly `K*t` or `(1-K)*t`. Opposite
bend signs in a Z sheet can have opposite parity; a signed radial basis alone
does not preserve front/back identity. Opening a strip embeds its thickness
relative to the stationary panel's reference surface using this parity and
offset. Do not align every bend's neutral plane to one global Z plane when
`K != 0.5`; align the actual front/back tangent ports. A flat outline uses
developed in-plane distances and is independent of that display thickness
origin.

The axial interval and 2D `(u,v)` domain are trimmed. This is a nonlinear map;
each panel uses a rigid placement, but a bend zone becomes a finite-width
neutral strip with its own boundary correspondence. Curve splitting at chart
seams and parametric curve transport must preserve endpoints and orientation
to the documented modelling tolerance. A tessellated outline is not the
authoritative result. Exact lines/arcs are retained where possible; otherwise
use bounded-error analytic/spline representations with explicit tolerance.

Traverse the labelled graph from the stationary panel. Shared boundaries must
agree by construction. Test closure along non-tree adjacency; a non-developable
cycle cannot be resolved by silently dropping an edge or overlapping material.
Check self-intersection between developed domains, including overlaps between
panels, bend strips, holes and reliefs. Intentional common seams are not
overlaps. Report involved panel/bend IDs and prevent valid/export status.

`sheet_unfold` names a stationary panel and an explicit nonempty set of bends.
It changes the actual evaluated body to a partially developed configuration,
retaining original bend definitions and named material charts. Unselected
bends retain their folded geometry and move with their connected panels.
Opening one bend must not implicitly open all bends. Unsupported selections
which have no consistent stationary/moving partition fail.

`sheet_refold` names the earlier Unfold and the nonempty subset it closes
(default: all still open bends of that Unfold). It reconstructs current trimmed
material in the stored folded configuration. It must not restore an old B-rep
snapshot: cuts made after Unfold must survive. Nested/partial unfold operations
track which feature opened each bend; Refold cannot close a bend owned by an
unrelated open operation or reopen an already closed bend.

`sheet_cut` is a thickness-normal material cut. It names the sheet, the sketch
regions, a stationary panel and the bends opened to interpret that sketch.
It can run on an explicitly unfolded body or internally open selected bends,
cut the developed material, then restore the input open/folded configuration.
The sketch plane must coincide with the selected developed panel plane within
tolerance; it is not arbitrarily projected through unrelated folded panels.
Split its boundary across panel and bend charts, subtract through `q`, and
reconstruct all affected surfaces and wall faces. No radius adjustment or
clipping of a sketch to the nearest planar face may stand in for this operation.

### Mandatory early proof

Before completing the folded-feature family, implement an OCCT-backed narrow
proof using two planar panels, one finite cylindrical bend and labelled charts:

1. Use `t=2`, `r=3`, `K=0.3`, angle 90 degrees, width 20 and planar tangent
   lengths 30 and 15, with the base sketch at Z=0 and `reference_side=back`.
   Base material occupies `x=[-30,0], y=[0,20], z=[-2,0]`;
   the outgoing flange occupies `x=[3,5], y=[0,20], z=[3,18]`. One bend basis is
   `centre=(0,20,3), e=(0,-1,0), a=(0,0,-1), b=(1,0,0), v=[0,20]`.
   Assert `BA=1.8*pi`, flat length `45+1.8*pi`, folded volume `1800+80*pi`
   and flat volume `1800+72*pi`. The volume difference is expected. Entered
   inner/outer lengths 18/20 must each produce a 15 mm straight flange.
2. In developed coordinates, cut rectangle `u=[-2,BA+2], v=[8,12]`, crossing
   both tangent seams. Its flat removed area is `4*(BA+4)`; folded removed
   volume is `4*(8+4*pi)`. Verify continuous cut walls, seam agreement and
   retained IDs, not only these scalar measures.
3. Cut a radius-4 circle centred at `(BA/2,10)`, also crossing both seams.
   Inverse-map the inner and outer cut boundaries to the source circle within
   tolerance. Assert cylinder radii 3 and 5, corresponding radial cut-wall
   generators, a closed valid solid and no uncut patch inside the bend. This
   distinguishes true bend transport from rotating planar faces alone.
4. Unfold/refold without a cut and compare the full named geometry with the
   original folded reference. Repeat with reversed angle, changed K/thickness
   and the stationary panel on the other side; add a contour tangent to a seam.
5. Extend to a U/Z sheet with opposite bend signs and `K=0.3`: open only B1,
   cut across B1, refold B1 and open it again to recover the same mask. The
   stationary panel stays fixed; B2 retains its radius, sign and parity and is
   carried rigidly with its panel. Inconsistent cycles, overlapping bend strips
   and flat self-overlap must fail explicitly.

If chart-boundary transport or tear topology fails this proof, revise this
contract and the construction seam before building UI around it. A Face/Flange
implementation does not justify postponing the cross-bend requirement.

### Implementation notes (mitcad#208)

Decisions made while implementing Unfold, Refold and the Flat Pattern:

- **Graph development lives in the model** (`sheet/folded/develop.rs`). The
  traversal starts at the fixed panel and crosses `joins`. A panel, or a bend
  whose configuration does not change, is carried by the rigid motion of the
  domain it is reached from. A changing bend is rebuilt from the material
  station of the side it is entered from: the front-surface point at that
  tangent line, the unit `u` direction, the axis and the front-to-back
  direction. Its developed chart is the plane through that station (front
  surface continuing the neighbour's front surface; the strip has the
  neutral length `BA` on both surfaces), and its folded chart is the cylinder
  with the bend's own radius, `Rn`, angle and `inner_is_front`. The other
  side's rigid motion maps its old station onto the new one. Chart regions,
  curves, ports and identities never change; the kernel only reconstructs.
  The sign-independent invariant `dot(front_to_back, u x axis)` is preserved by
  every chart, so ports keep matching front-to-front and back-to-back.
- **Which side a port lies on** is read from the bend chart (`u = 0` or
  `u = BA` within `1e-6 mm`), never from the bend's stationary/moving labels,
  so either neighbour can be the fixed side.
- **Closure.** A domain reached a second time must receive the same rigid
  motion at that port, within `1e-6 mm` at the port point and one millimetre
  around it; otherwise the configuration is a non-developable cycle. Material
  not reachable through registered seams is refused, never dropped.
- **Stale records.** Before developing, every bend record must match the rules
  the sheet is evaluated with (`r`, `Rn = r + K*t`, `BA = Rn*|angle|`, parity,
  rules version). A mismatch is reported as a stale development.
- **Recorded fixed panel.** `opened[].stationary` stores the panel each Unfold
  held; Refold holds the same panel, so Unfold followed by Refold is the
  identity up to rounding and the folded placement is restored exactly. A
  Refold of bends opened against different panels is refused. The
  `folded_embedding` of an open bend is the cylinder refolding would build
  while holding its `u = 0` side; only its intrinsic values are used.
- **Self-overlap.** Planar charts are grouped by plane and front direction.
  Within a group every pair whose 2D bounding boxes overlap by more than
  `1e-6 mm` in both directions is intersected by `sheet_trim_domain`; a common
  area above `max(1e-6 mm2, 1e-9 * smaller area)` names both domains. Shared
  seams have no area. Folded configurations rely on the kernel's collision
  check of the constructed solid.
- **Walls that meet (mitcad#1409).** Where the folded solid's walls meet
  (the collision check finds faces that meet, or the solid checker refuses
  the solid), every pair of material regions whose boxes meet is compared,
  each region built on its own: a common volume above `1e-6 mm * t^2`
  (deeper than the tolerance over a square of the thickness) refuses the
  sheet naming both walls ("pass through each other"); else a common area
  of their surfaces above `1e-6 mm * t` between regions no port joins
  refuses it as a face contact ("touch over a face ... leave a seam gap").
  Walls that touch only along a line or at a point, such as a full open
  corner closed by welding at the exact inside dimension, are kept as they
  are: not fused, their faces not merged, the corner line an edge of each
  wall. A face contact (a half open corner, the closing wall lying on the
  other wall's end) is refused because the one solid it makes is not
  sound: the solid checker refuses it, and holes, cuts and fillets on it
  fail. Nothing adds a clearance: the body is built at the entered
  dimensions or refused. CLI `sheet_open_corner`.
- **Flat Pattern.** One `FlatPatternDef` per body in `DocState::flat_patterns`
  (file field `flat_patterns`), with `stationary` (default: the body's lowest
  panel when the item is created), `x_axis` (only `panel_x`, the panel's own
  material axis) and `reference_side` (`back` mirrors `x`). It is evaluated at
  the marker by developing every bend from the current state, so the drawing
  is the same whichever bends the timeline has open. The outline is the kernel
  union of all developed charts mapped into the fixed panel's chart: one
  closed region is required. Centre and tangent lines are clipped by exact
  chords of the strip's chart (tangent lines just inside the strip, reported
  on the seam). Up means the bend's inside is on the viewed side. The
  developed solid is built and measured for validation and display
  (`Document::flat_pattern_shape`) but never published as a body. Evaluations
  are cached per body under a fingerprint of the sheet state, shape, owner,
  rules versions, settings and algorithm versions; a refusal is cached with
  its diagnostics under the same key.
- **Minimal Cut (the cross-bend proof).** `sheet_cut` develops the named
  bends holding a panel, maps the sketch (on that panel's front or back
  surface) into its chart, and trims every chart in the developed plane with
  one `sheet_trim_domain` call whose seams are the ports between them. The
  trim's ports replace those joins; seams to material outside the plane must
  survive whole and are rebound by geometry, because lines return with their
  own normalized parameters. Fragments of one curve receive distinct `piece`
  numbers ordered by midpoint. Embeddings are then restored. The CLI fixtures
  `sheet_cut_proof` and `sheet_cut_partial` reproduce contract proofs 2 and 5
  with the analytic volumes (folded `4*(8+4 pi)`, developed `2*4*(BA+4)`) and
  recover the same mask after Refold and a second Unfold.
- **Trim loops.** A trim whose operation frame is a reflection returned
  clockwise exteriors; trimmed charts are now normalized to counter-clockwise
  exteriors and clockwise holes, which boundary normals rely on (a second
  Flange on the opposite edge of a U failed before).
- **Not yet covered.** Paste New does not copy Flat Pattern items, and design
  diffs do not list them (both done later, see the mitcad#125 gap notes). The
  DXF export and command UI are mitcad#210.
- **Hooks for mitcad#209 (production Cut).** Extend `sheet/folded/cut.rs`:
  `develop::reconfigure` opens the bends a cut needs while a chosen panel
  stays fixed; `planar_map`, `then` and `inverse` give each developed chart's
  isometry (`TrimDomain::to_operation`); `cut::settle` installs a multi-domain
  trim result with lineage, unique pieces and orientation and returns the
  piece of each edge token for the trim's ports. Still to add there: several
  sketches and reliefs, results split into several bodies (body allocation),
  domains removed completely, and stable references after adjacency edits.
  `check_overlaps`, `chart_point` and `chart_coordinates` validate and map
  results.

### Implementation notes (mitcad#209)

Decisions made while implementing the production Cut and the reliefs:

- **One cut operation** (`sheet/folded/cut.rs`, `developed_cut`): the listed
  bends are developed from the fixed panel, every chart in its plane is
  trimmed by the tool regions in one `sheet_trim_domain` call whose seams are
  the ports between them, the result is settled (lineage, unique pieces,
  orientation) and the input embeddings are restored. `sheet_cut` and
  `sheet_corner_relief` differ only in where the tool regions come from.
- **Several sketches and contours.** Each profile is mapped from its own
  sketch frame into the fixed panel's chart; each sketch must lie on that
  panel's front or back surface. Profiles resolve like extrusion profiles: a
  region key that no longer exists after a sketch edit takes the regions
  inside the outline it traces now (all pieces of a split region), else the
  region with the most curves in common, with a warning, so a dimension edit
  that changes which seams a contour crosses keeps the cut. Tools may overlap; holes in a region are kept.
- **Cuts ending on a tangent line.** A tool boundary lying exactly on a seam
  removes material on one side only. The trim now ends the seam there instead
  of refusing it: the interval is no longer a port and the surviving side
  keeps it as a free edge (`geometry/src/sheet_trim.cpp`, `synchronize`). This
  also covers contours tangent to a seam and the tear corner relief.
- **Domains removed completely.** A chart without material left is removed with
  its lineage and ports. A removed bend loses its record and its open state;
  later features naming it fail ("not a bend of this sheet"). A removed domain
  whose seam leads to material that is not developed is refused, because the
  intent at that seam is ambiguous; open that bend for the cut.
- **Split bodies.** `publish_pieces` builds the cut state once; every solid the
  kernel returns is one piece with the material regions of its membership.
  The pieces are ordered by centre of mass, then the lower corner of the
  bounding box (rounded to 1e-6 mm, as the kernel orders Boolean pieces); the
  first keeps the input `BodyUid`, the others become `F<cut>.b0`, `b1`, ...
  owned by the input body's component. `partition` gives each piece its
  charts (regions renumbered in order), the ports between its regions, the
  records and open states of its bends and the lineage of its domains; a port
  joining two pieces is an inconsistent result. The same chart identity can
  then exist in two bodies; a semantic selection is the pair of body and
  domain. An Unfold opened against a panel that went to another piece cannot
  be refolded in this piece (its fixed panel is missing) and fails explicitly.
  The Flat Pattern item stays with the original body id; a pattern whose fixed
  panel went to the other piece is `invalid` and names the missing panel.
- **Bend reliefs** are those of #207 (Flange end offsets, Fold lines ending
  inside material): rectangular and tear slots of the entered width (tear is a
  slot of the exact entered gap) and round slots whose depth includes the tip.
  Their contours are generated curves of the bend feature and keep that
  provenance through later cuts, Unfold and Refold; the CLI fixture
  `sheet_corner_relief` cuts across a Flange's relief slot with the bend open.
- **Corner relief** (`sheet/folded/relief.rs`, `sheet_corner_relief`). The two
  bends must leave one common panel (selected with `panel` when ambiguous).
  With both developed, `s` is the distance from each tangent line on that
  panel into its strip; the tangent lines meet at `J0`, the centrelines
  (`s = BA/2`) at `C`. Round: disc of diameter `width` at `C`. Rectangular:
  square of side `width` at `C`, sides along both strips (perpendicular bends
  only). Tear: exactly the crossing zone `0 <= s1 <= BA1, 0 <= s2 <= BA2`,
  the minimum separation of the two bend zones. The placement at the
  centreline intersection is a MitCad decision; importers translate other
  placements explicitly. `depth` is used by bend reliefs only; the corner
  relief's size is `width`. Defaults come from the bound rules'
  `corner_relief`. A relief that removes a whole domain ("the flange is too
  short for it") or disconnects the sheet is refused, as is one reaching a
  seam of a third, folded bend; a relief that removes nothing warns.
- **Tests.** Model: `partition` on hand-built L and Z charts, the junction
  geometry, definition refusals and multi-sketch definitions (mock kernel).
  OCCT (CLI fixtures with reopen passes, analytic volumes and flat areas):
  `sheet_cut_multi` (two sketches, four contours across one and two bends of
  the Z, a hole, a contour ending on a tangent line, an adjacency edit, a rule
  edit, errors), `sheet_cut_split` (a wall removed completely, the strip
  removed so the sheet becomes two bodies, a failing Unfold of the removed
  bend, a cut of all material refused) and `sheet_corner_relief` (all three
  shapes, the refusals, a cut across a bend relief, Unfold/Refold, a K edit).
  `geometry.sheet_trim` covers a cut ending on a tangent line.
- **Not covered yet.** Corner reliefs at junctions of bends on different
  panels (a bend joining two bends directly), non-perpendicular rectangular
  corner reliefs, and Paste New/copies of split pieces beyond the generic
  sheet-state copy rules (all done later, see the mitcad#125 gap notes). The
  command UI and DXF export are mitcad#210.

### Implementation notes (mitcad#210)

Decisions made while implementing the command UI and the DXF export:

- **Commands on the model API.** The Sheet Metal group of the Solid tab
  (`app/commands/SheetCommands.cpp`, [app/COMMANDS.md](../app/COMMANDS.md#sheet-metal))
  has Face, Flange, Bend, Fold, Corner Round, Corner Chamfer, Sheet Cut,
  Corner Relief, Unfold, Refold, Flat Pattern, Sheet Rules, Edit Sheet Rules,
  Show Flat Pattern and Export Flat Pattern. It keeps no sheet model: picks
  are faces and edges named by their domains, which the `sheet_reference`
  query turns into panels, bends, boundaries and corners (`at` chooses among
  pieces of one curve, and between panels whose coplanar caps the kernel
  joins into one face), and `sheet_seam` into the two boundaries of a
  coplanar seam (a Bend picked as two flat faces, or the joined face clicked
  on each side of the seam). Previews, Cancel and Undo are the framework's.
- **The first Face's rules.** "New Rules" (the default while the design has no
  rules) sends the rules' definition inline in the Face's `add_feature`; the
  model adds a `sheet_rules` feature just before the Face in the same undo
  step (also in `preview`), so Cancel leaves nothing and Undo takes both. With
  rules in the design the Face preselects the newest; reusing rules is the
  explicit choice of one of them. Inline rules get round reliefs of 1 x 2 mm;
  the Sheet Rules command and its edit expose all fields.
- **Editing rules from the browser.** A sheet metal body's row lists its rules
  (`SheetRules1 (2 mm, R 3 mm, K 0.3)`) and its Flat Pattern; double-click or
  Edit Sheet Rules opens the rules feature's edit, which every sheet using
  them follows (no silent fork). The values are the `sheet_bodies` query's.
- **Folded/flat display.** `FlatPatternDef.shown` (saved, an undo step,
  outside the evaluation's fingerprint) makes the application show the
  developed solid (`analysis_shape` `flat_pattern`) in place of the folded
  body; picks on it name the same domains. It is per item, not per window.
- **Stationary choice.** Flat Pattern, Unfold, Cut and Fold take a flat face of
  the sheet (front or back): the domain is the panel either way.
- **DXF.** Made from the evaluated outline and bend lines (`sheet/flat_dxf.rs`),
  never from the solid: each contour once, the pieces chart seams split joined
  again, layers `CUT_OUTER`, `CUT_INNER`, `BEND_UP`, `BEND_DOWN` and optional
  `BEND_TANGENT` always declared, `$INSUNITS` and coordinates in the chosen
  unit, the pattern's frame. Up/down follow the viewed side, so exporting the
  back reverses them and mirrors `x`. Without an item the export uses a new
  item's settings rather than refusing. The export writes the design's length
  unit unless `units` names another (mitcad#1408); the application's save
  dialog offers the units as file types, the design's first.
- **Tests.** Model: `sheet/flat_dxf_tests.rs` (seam joining, hole circles,
  layers, inches, round trips through `mitcad-dxf`), `sheet/pick_tests.rs`,
  `sheet_ui_tests.rs` (inline rules and Undo, picks, sheet metal bodies, the display
  choice, export refusals). CLI with OCCT: `sheet_flat_dxf` (the contract L
  with a hole: contours, bend line, back view in inches with tangents, then K,
  thickness and radius edits changing the drawing; reopened), `export_flat`
  and `export_flat_invalid` (a self-overlapping pattern is not written).
  Windows UI: `app.ui-windows-sheet` and `app.ui-windows-sheet-ops` (Fold,
  Bend, Corner Round and Chamfer, Corner Relief, Sheet Cut, Unfold and Refold
  with analytic volumes and areas, timeline edits and Undo). MCP: `cli.mcp`.
- **Not yet covered.** Paste New does not copy Flat Pattern items and design
  diffs do not list them (left from mitcad#208; done in the mitcad#125 gap
  notes); the command panels have no
  manipulators; Fold and Bend previews show the result, not the bend line's
  arrows of the contract. macOS uses the same Qt panels
  ([macos-shared-qt.md](macos-shared-qt.md)).

### Implementation notes (mitcad#125 gaps)

Decisions made while closing the leftovers of mitcad#208 to mitcad#210:

- **Junctions without a common panel** (`sheet/folded/relief.rs`). The
  junction is always taken in a development: a panel is held and the two
  bends, with every bend on the seam paths from that panel to them, are
  developed into its plane. When the bends leave a common panel nothing
  changes. Otherwise (their panels meet through planar seams, further panels
  or other bends) the held panel is the one next to the first bend on the
  shortest seam path to the second (breadth-first over `joins` in their stored
  order), or the selected `panel`, which must join one of the two bends
  ("joins neither" otherwise); every bend on the paths is opened. `s = 0` of a
  strip is the tangent line through which that development reaches it. The
  rule stays the centreline intersection: `C` and the crossing zones are the
  same whichever tangent line is `s = 0` and whichever adjacent panel is held,
  since the development is rigid up to that choice. The junction lies where the
  developed strips' lines cross, so it can fall where only one strip has
  material; a relief that removes nothing still warns. Parallel bends and
  bends not connected through registered seams are refused.
- **Rectangular relief at non-perpendicular bends.** The rectangular relief is
  the crossing of two bands of width `width` centred on the two centrelines,
  `|s1 - BA1/2| <= width/2` and `|s2 - BA2/2| <= width/2`, the same
  construction as the tear (the crossing of the two strips) at the relief's
  size. For perpendicular bends it is the earlier square of side `width`
  ; otherwise a parallelogram with sides parallel to both bend
  lines, `width` across each strip and area `width^2 / sin(angle)`. A rotated
  rectangle would leave a slot side that is not parallel to either bend line,
  which no flange edge or press-brake tool follows; the parallelogram keeps
  every side on a line of one bend.
- **Seams of trimmed panels.** A flange, fold or corner operation trims its
  panel with `sheet_trim_domain`, which returns lines with their own
  parameters, possibly traversed the other way. Registered seams of a trimmed
  domain whose boundary survives whole are now bound to it again (the Cut's
  `rebind`), so a flange on a panel with a planar seam to another panel works;
  seams that still evaluate to the same points are left untouched, and seams
  the trim shortened stay for the reconstruction to refuse.
- **Paste New of Flat Pattern items and split pieces** (`document/copy.rs`).
  Copying a component (Paste New, inserting a copy of a design) copies the
  Flat Pattern item of every copied body, split pieces `F<cut>.b<k>` included,
  with its settings (also `shown`) and the fixed panel's feature mapped to its
  copy. The pieces themselves are the copied Cut's bodies, evaluated with their
  own sheet states, owners and copied rules. A component copied as bodies has
  no sheet state and no item.
- **Design diffs** (`diff/`) list Flat Pattern items by body uid as
  `flat_patterns` (`Flat Pattern of <body>: added`, `deleted`, or the changed
  fields `stationary`, `x_axis`, `reference_side`, `shown`), counted in the
  summary (`+1 flat pattern`) and listed under "Flat Patterns" in the text.
- **Tests.** Model: `relief.rs` (junction, side independence, the
  parallelogram's corners and area, the held panel and seam paths without a
  common panel), `sheet_flat_tests.rs` (Paste New copies the item with its
  settings, independent, saved, one undo step), `diff/tests.rs`. OCCT (CLI
  fixtures with reopen passes, analytic volumes and flat areas):
  `sheet_corner_relief_paths` (flanges on two panels joined by a planar seam:
  round, square and tear reliefs, every adjacent panel held giving the same
  result, a panel joining neither refused, Unfold/Refold, a K edit),
  `sheet_corner_relief_angled` (flanges at `sin a = 3/sqrt(10)`: the
  parallelogram `sqrt(10)/3 * width^2` and the tear zone `BA^2 sqrt(10)/3`,
  either bend order, a K edit) and `sheet_copy_pieces` (Paste New of a split
  sheet with two Flat Patterns, independent copied rules, Undo/Redo).

### Implementation notes (mitcad#242)

Decisions made while closing the import gaps of mitcad#211:

- **Hems and the outer extent.** A flange's angle may reach 180 degrees
  (and since mitcad#1378 pass it, below).
  `X = (r+t) sin(min(|angle|, 90 degrees))` is how far the bend's outer
  surface reaches past its stationary tangent line. Position
  `side_tangent` puts the tangent line at `-X` (the outer surface touches
  the side plane through the selected boundary and does not pass it);
  length mode `outer_tangent` measures from the plane touching the outer
  surface normal to the wall (straight length `L - X`). At 90 degrees they
  are `inside` and `outer`; at 180 degrees the virtual sharps do not exist
  and `inside`, `outside`, `inner` and `outer` are refused. The `.f3d`
  flanges "tangent to the side face" with the height datum 3 are these.
- **Widths from the middle.** `width: {type: middle, before, after}`
  measures from the boundary's midpoint along the canonical axial
  direction. A width past an end of the boundary is a wall wider than its
  bend: the bend covers the boundary, and the wall's overhang is cut back
  at its root by the relief depth, so the bend's side face stays free (a
  rectangular notch; the moving chart's root edge stays the bend's port).
- **Faces joined in a panel's plane.** `sheet_face` with `body` adds its
  regions to the panel whose front or back surface holds its sketch (one
  union through the thickness, the sheet's rules). A coplanar join leaves
  no seam, so the added material has no panel identity; its sketch curves
  become the panel's boundaries through the trim's lineage. The `.ipt`
  joined faces that #211 took for double bends are such joins (their
  plate's side was not read).
- **The sheet-domain adapter** (`sheet/rederive.rs`). An ordinary feature on
  a valid sheet metal body (also an import's base feature) is followed by a
  re-derivation: per plane of planar charts, the result's faces on the front
  surface give the new outline (exact edge curves mapped into the chart);
  the lost material is removed with the Cut's trim (seams, lineage, pieces);
  the rebuilt sheet must have the result's volume, area and centre of mass.
  Through-thickness changes (cuts, holes, their mirrors and patterns,
  library cut features, contour corners) keep the sheet; added material,
  changed bends, slanted walls and dressups of surface edges invalidate it
  with that reason. Bends are not re-derived.
- **Patterns and mirrors of sheet features** (`sheet/replay.rs`). Copies in
  timeline order on the working sheet: a flange is rebuilt on the copy of
  its boundary (found by geometry in the current sheet, front or back
  surface; side and axial direction from the transform); other features
  repeat their material change (exact booleans of their before and after
  bodies) and the adapter re-derives the domains, now also uniting added
  coplanar material with the panel it touches. Spline edges the sheet
  construction gives straight and circular walls are read back as lines
  and arcs. Copies' identities are renumbered per copy. A straight cut is
  cut again from the copy of its sketch plane (an overlay of the copy where
  it crosses a bend), and a feature kept as a `change` overlay is copied as
  one (its placement the copy's transform). CLI `sheet_pattern_straight_cut`,
  `sheet_mirror_dressup`.
- **Straight cuts and overlays.** `sheet_cut` mode `straight` is an
  extrusion's cut. Re-derived where possible; otherwise (walls following
  the sweep through a bend or an inclined panel) kept as a `SheetOverlay`
  of the state and cut from every rebuilt solid, with a guard against
  cutting material added later. A sheet with overlays has no developed
  form: Unfold, Refold, Fold, Bend and the Flat Pattern refuse it. The
  `.ipt` cuts are straight cuts (both of their sheet options unset).
  Ordinary fillets and chamfers the adapter cannot re-derive are kept as
  `change` overlays: the material the feature changed (exact booleans of
  its input and result, read again from the timeline's results at every
  rebuild, moved with the body), with the same guard and refusals.
- **Relief depth.** Any positive depth behind the stationary tangent
  separates the moving material; the earlier minimum of one thickness was a
  convention that rules of half a thickness (the `.f3d` default) do not
  follow.
- **Tests.** CLI `sheet_flange_hem` (a 180-degree hem: volume `2000 +
  160 pi`, flat area `1000 + 72 pi`; `side_tangent` with `outer_tangent` at
  90 degrees; a wall 24 wide on a boundary of 20 with root notches of the
  relief depth; a partial width with reliefs half a thickness deep), the
  adjusted `sheet_flange_geometry` and `sheet_fold_reliefs`;
  `sheet_face_join` (a 20 x 4 strip joined to a 40 x 20 panel, a flange on
  the strip, a thickness edit, a face apart refused), `sheet_adapter` (an
  ordinary cut keeps the sheet, a flange after it, a thickness edit, a boss
  invalidates it) and `sheet_cut_straight` with its reopen pass (a straight
  cut in the panel as a domain edit; one through a bend as an overlay:
  `40 + 4 (12.5 asin 0.6 - 3)` removed; a later developed cut; Unfold and
  the Flat Pattern refused), `sheet_mirror` (a joined strip and a hole
  mirrored, a flange after them, a thickness edit), `sheet_mirror_flange`
  (a flange mirrored onto the opposite edge, following a length edit),
  `sheet_corner_whole` (a corner chamfer along a whole boundary),
  `sheet_dressup_overlay` (an ordinary chamfer kept as an overlay through a
  later flange and a thickness edit, Unfold refused) and
  `sheet_curved_seam` (two regions of one Face meeting on a half circle,
  a flange on the bigger one: `800 + 15 pi + 200`, one body).
- **Panels meeting on a curve.** Panels of one plane may touch along a
  boundary no bend port pairs (two regions of one Face meeting on an arc,
  a spline or a straight seam that is not a bend). The general
  construction sews each group of panels joined by ports into its own
  shells, so coincident walls of such a seam are never sewn together, and
  then fuses solids that share a panel plane and touch (the sum of their
  volumes must be kept, otherwise the regions overlap and the sheet is
  refused). The fused solid keeps every panel's identity.

### Implementation notes (mitcad#280)

- **Double bends.** `sheet_bend` with port mode `double` joins two parallel
  panels of separate bodies whose straight boundaries face each other by a
  jog: two opposite bends of the entered angle (at most 90 degrees) and a
  straight panel between them. The jog is built from the fixed boundary:
  the first bend at the fixed panel's port (the flange construction with
  the tangent line on the boundary), the straight panel, the second bend
  from the straight's far port; the second bend's moving plane must be the
  other panel's (front surface on its front surface, within `1e-6 mm`).
  With `h` the distance between the panels' planes and `rm = r + t/2`, the
  straight is `(h - 2 rm (1 - cos a)) / sin a` long and the jog's run
  `2 rm sin a + L cos a`. The other panel is cut back to the run with the
  flange's trim (`cut_back`), through the bends seamed to it along the cut,
  developed; the cut stops `1e-4 mm` past the panel's boundary, because in
  that development other panels (a base panel's corner beyond a relief
  notch) can lie just beyond it. Unfold, Refold and the Flat Pattern
  develop the jog as any bends (CLI `sheet_double_bend`: `1300 - 100 sqrt 2
  + 25 pi` folded, `1300 - 100 sqrt 2 + 24 pi` developed, with both
  boundaries fixed in turn, a thickness edit, a turned-over moving body and
  a reopen pass).
- **Loops of material.** The `.ipt` double bends that #242 took for
  loop-closing bends join a separate island to the sheet: the joined face
  before them adds strips that touch no material (their own bend part
  lists no edge, and their history state holds them as separate lumps of
  the body), so each double bend closes no loop. A double bend between
  panels of one body would close one; the folded construction could build
  it, but Unfold, Refold and the Flat Pattern develop a tree and refuse a
  second path to a domain unless it closes exactly, so such a bend is
  refused with that reason instead of building a sheet that has no
  developed form.
- **Front and back of the moving body.** Two bodies made from sketches with
  opposite normals have their front surfaces on opposite sides. A double
  bend joins front to front: the moving body's charts take the stationary
  body's labels (each frame moved to the other surface, the material
  direction reversed, the material coordinates kept). A moving body with
  bends is refused then (their inside sides would change).
- **Copies of faces that make bodies.** A pattern or mirror whose objects
  are faces that made new sheet metal bodies copies each body as a new sheet metal body
  of the pattern: the panels' frames moved by the element's transform (a
  reflection reverses the frame's handedness, so the material direction
  keeps its image by reversing the parity), the domains renamed as the
  pattern's (slots offset by 100000 per copy) and the solid built again
  from them (CLI `sheet_double_bend_mirror`: a strip and its mirror joined
  to a panel's two ends by double bends, `1400 - 200 sqrt 2 + 50 pi`).
- **Cuts ending on a folded seam.** A developed cut (also the adapter's
  re-derivation of an ordinary cut) whose contour ends on the seam of a bend
  that is not developed, removing material on the panel's side only,
  shortens the seam to the stretch the panel's boundary keeps; the bend's
  boundary keeps the rest as a free edge (its chart is outside the cut's
  plane and does not change). A cut that leaves the boundary in more than
  one stretch along the seam still asks for the bend to be opened. CLI
  `sheet_cut_on_seam`.
- **The adapter and walls in a panel's plane.** The re-derivation reads a
  plane's material from the result's faces on its front surface; faces the
  sheet names as thickness walls of other domains (the ends of a box's side
  flanges in the plane of its end wall's outside) are skipped, so an
  ordinary cut on such a box keeps the sheet.
- **Corners of one feature on its input.** A corner round's or chamfer's
  corners are all found on the sheet before the feature, then each trims
  its panel: an earlier corner's trim renumbers the pieces of the curves a
  later corner names. A boundary reference to a piece prefers that piece
  to another piece that inherited its alias when it was split off.
- **Rounds past a short side.** A round whose tangent length exceeds one
  side of its corner, where the contour continues on an arc beyond that
  side (an earlier round), takes that side whole: the round's circle is
  tangent to the other side's line and to the arc's circle (the one
  solution touching the line within its side and the arc within its
  extent), the material between the contour and the round removed or added.
  CLI `sheet_corner_round_past`: `358 - (16 - 4 pi)` after a radius-4 round,
  then `+ 0.170772982484` for a radius-1 round below it.
- **Fold lines on the back surface.** A fold line may lie on the panel's
  front or back surface: across the thickness it is the same line. CLI
  `sheet_fold_back`: `800 + pi` either way.

### Implementation notes (mitcad#288)

- **Folds across holes and cut windows.** A fold line that crosses holes or
  cut windows leaves the bend's lines (the stationary panel's new boundary,
  the strip's two sides and the moving panel's new boundary) in stretches.
  Each stretch gets its own piece number of its generated curve, in the
  order of the axial coordinate (as a developed cut numbers its fragments),
  and each pair of stretches facing each other is a seam of its own over
  its part of the axial interval; the strip keeps the openings, so the
  window stays a window through the bend. The pairs must cover the same
  axial intervals (they come from one line through the same material).
  Unfold, Refold and the Flat Pattern develop such a bend as any other (the
  several seams between one panel and its bend close by construction). CLI
  `sheet_fold_window`: a 60 x 20 panel (`t = 2`, `r = 3`, `K = 0.3`) with
  a 10 x 4 window across the fold, `2320 + 6.4 pi` folded, `2320` and the
  window again developed; a window across one line only, `2360 + 7.2 pi +
  8/9` on either side; a thickness edit.
- **Folds across oblique edges (mitcad#1424).** The bend's strip of a Fold is
  the panel's material between the two tangent lines, so where the line
  crosses an edge at another angle than 90 degrees the strip ends on that
  edge, and its stationary and moving tangent lines end at different axial
  positions. Each seam covers the part of its line that has material (the
  stationary panel's new boundary pairs with the strip's first side, the
  strip's second side with the moving panel's), not the whole length of the
  line; a bend built from a rectangle (Flange, Hem, Bend) still has sides
  over its whole interval. The bend's `axial_interval` is the interval its
  strip's material covers. Each end of the line is classified from the
  material on the normal just past it (1e-5 mm): material on both sides of
  the line there is an end inside material (relief as before); otherwise
  material past the end within the bend's reach (beyond the stationary
  tangent line: the line drawn exactly from edge to edge of a trapezoid
  whose moving side widens, or whose stationary side widens into the
  strip) lets the strip, the moving panel and the stationary panel's cut
  run on past that end, bounded by the outline; with none, the bend ends
  at the end of the line. The relief no longer follows the panel's
  bounding box (a line ending on the outline of a panel rotated in its
  sketch asked for a relief that cut nothing). A seam whose sides still do
  not pair names the bend and the boundary curve of its strip where they
  part. CLI `sheet_fold_oblique`: a trapezoid folded across its slanted
  sides at 90 and 40 degrees (`2000 + 100 BA/9`), the line from edge to
  edge and past both edges, either side fixed, centred or from the line,
  the flat pattern its outline, opened and folded back;
  `sheet_fold_oblique_polygon`: a reported seven-sided panel folded at 40
  and 90 degrees with the line past both edges it crosses, the same.
- **Patterns of straight cuts with many copies.** A pattern whose copies
  are all of one straight sheet cut cuts the copies that keep clear of the
  bends' seams (the box of each copy's tool does not reach a seam's line on
  the front or the back surface) at once: one boolean of all their tools
  and one re-derivation of the domains, the result the same as cutting
  them one after another; the copies that reach a seam are cut one by one
  after them as before (an overlay where one crosses a bend). A vent
  pattern of 228 copies on a folded sheet took over a minute copy by copy
  and takes about ten seconds so. CLI `sheet_pattern_holes`: 60 copies of
  a 2 x 2 cut in a 60 x 40 panel, `4320`, a flange after them and a
  thickness edit.
- **The intersection check of many holes.** The check that a folded
  sheet's charts do not intersect compares every pair of faces whose boxes
  meet; the walls of hundreds of holes each meet their panel's caps, and
  the check took most of a minute. The charts with their holes filled (the
  interior loops that carry no port) hold all of the sheet's material, so
  where they do not intersect the sheet does not either: they are checked
  instead, and only where they intersect (material through a window) is
  the sheet itself checked (`geometry/src/sheet_chart.cpp`).
- **Double bends in the Bend panel.** The panel's Bend Type (Bend, Double
  Bend) writes `port_mode: "double"` and Whole Face the `fixed` boundary;
  editing a double bend (the `.ipt` import's) loads both, so an edit keeps
  it a double bend. The angle of a double bend has no Flip (the jog turns
  towards the other face). UI test `tools/ui-sheet-double-bend-test.sh`.

### Implementation notes (mitcad#461)

Contour Flange, Convert to Sheet Metal and Perforate
([their API](../core/model-api/src/commands.md#contour-flange-convert-to-sheet-metal-and-perforate)),
built from the same charts, bends and trims as the features above, so their
sheets unfold, refold and have a Flat Pattern:

- **Contour Flange** (`sheet/folded/contour.rs`). The chain of sketch lines
  is the sheet's surface on the rules' reference side (the front for
  `front`, the back for `back`); the material lies on the chosen side. A
  corner turning towards the material has that surface outside the bend:
  the lines on both sides are set back from the corner by the outer
  setback `(r+t) tan(a/2)`; one turning away has it inside, set back by
  `r tan(a/2)` (`a` the turn). Each panel is the rectangle of its line's
  straight length by the width; each bend is built as a flange's bend
  with its tangent line on the panel's end. A line not longer than its
  setbacks, lines in one line and a turn of 180 degrees are refused. CLI
  `sheet_contour`: the chain (0,0)-(40,0)-(40,30)-(70,30), t=2, r=3,
  K=0.3, 50 wide: setbacks 3 and 5, straights 37, 22 and 25, `8400 + 400
  pi` folded, `8400 + 360 pi` developed, the material on the left, the
  width reversed, back rules, a thickness edit (`4300 + 175 pi`), one line,
  the refusals, a flange on its last panel, reopened.
- **Convert to Sheet Metal** (`sheet/folded/convert.rs`). The picked face
  is the first panel's front; outlines are read exactly from the body's
  edges (the sheet-domain adapter's curve reading). Without bends the
  body must be a plate of the rules' thickness: the sheet of the face must
  have the body's volume and centre of mass (within `1e-6` relative). With
  bends, the faces reached across the bend edges are panels with the
  rules' thickness behind them: a shell of that wall thickness keeps its
  walls, a solid becomes the sheet of its faces. A bend edge is the
  virtual sharp on the front, so both faces are cut back by the setback of
  the side the front is on (outer where the bend turns towards the
  material, inner otherwise); the face across is built where the bend puts
  it and must lie in that plane, its edge along the bend running the whole
  bend. The rules' thickness always governs: MitCad's sheet state has one
  thickness, its rules'; an import makes rules of the thickness its file
  measured. Not supported, each refused with the edge or face: faces that
  meet at an edge that is no bend (corners that would have to be cut open
  with a gap), loops of bends, curved faces, chamfers or rolled bends
  picked as bends, faces ending at an angle to their bend. CLI
  `sheet_convert`: a 40 x 20 x 2 plate (1600, a 3 thick one refused), an
  L of 2 thick walls bent at its outer edge (`6000 + 200 pi`, radius 1:
  `6400 + 100 pi`), a solid block becoming a U channel (`3600 + 240 pi`,
  flat `30 (60 + 3.6 pi)`), the corner and the unconnected bend refused,
  reopened.
- **Perforate** (`sheet/folded/perforate.rs`). The holes are real: cut
  through the material as a developed cut with every bend open, so the
  body, the Flat Pattern, the DXF (`CUT_INNER` circles) and every measure
  have them, and later features work on the sheet with its holes. The
  grid lies in the chart of the stationary panel (default the lowest,
  as the Flat Pattern's) with the whole sheet developed, from the lower
  left corner of the developed material's box, `margin + d/2` in; a hole
  is kept on a panel, off every bend's strip, and `margin + d/2` or more
  from every boundary of the material and of the strips (seams between
  panels of one plane are inside the material). One trim and one solid
  take all holes; fields of thousands of holes are possible but each hole
  is part of the B-rep. CLI `sheet_perforate`: an L contour flange,
  staggered 33 holes (`9400 - 212.5 pi`), square 28 (`9400 - 150 pi`), the
  Flat Pattern (`4700 - 85 pi`), Unfold, a thickness edit, reopened.

### Implementation notes (mitcad#1378)

Hems past 180 degrees, for the `.f3d` rolled, teardrop, rope and double
hems:

- **Bends past half a turn.** A flange's signed angle may be anything
  short of a full turn (`BendMetrics::past_half_turn`); Bend and Fold keep
  the 180-degree limit. The bend's cylinder covers its angle (the
  geometry's chart check allows less than `2 pi`), its developed strip is
  the allowance `|angle| (r + K t)` as for any bend, its outer extent `X
  = r + t`, and its virtual sharps do not exist (`inner`, `outer`,
  `inside`, `outside` refused). Model points past 180 degrees map back to
  their chart coordinates (`chart_coordinates` goes on past `pi`).
- **A flange of its bend alone.** A straight length of 0 (within the
  tolerance) leaves no wall: the bend's moving tangent line is a free
  boundary (the strip's curve `41`), its record naming that line as its
  moving boundary. A rolled hem is such a flange of 270 degrees.
- **Hem forms.** `sheet_flange` with `hem` (`teardrop`, `rope`,
  `double`) works its bends and walls out from the gap, setback, radius
  and the hem's length (`flange_steps`, `hem_steps`; the formulas are in
  `commands.md`). Bend `k` of the flange is its bend slot `k`, the wall
  after it its panel slot `k`; the next bend starts at that wall's far end
  (`51 + 100 k`), or at once on the bend's moving tangent line where no
  wall follows (the rope's two bends meet directly: a seam between two
  bends). The teardrop's angle has no closed form: it is the root of
  `(r (1 + cos a) - g) cos a = (L - (r + t)(1 + sin a)) sin a` on `(0, pi/2)`,
  found by bisection. Checked against the stored bodies of the `.f3d`
  reference hems (t 2.5, on a 17500 mm3 panel 100 wide): teardrop
  (r 2, gap 1, length 10) 20402.855, rope (the same sizes) 20561.79,
  double (r 1, gap 2, setback 1, length 10) 22784.292, rolled (r 3,
  270 degrees) 21131.913 mm3, which these formulas give to the stored
  digits.
- **Tests.** Model: `bends.rs` (`a_flange_bend_turns_past_half_a_turn_with_its_allowance`,
  `hem_forms_give_their_bends_and_walls`), `develop_tests.rs`
  (`chart_coordinates_go_on_past_half_a_turn`). OCCT: CLI
  `sheet_flange_hem_past` (rolled, teardrop, rope and double hems on a
  35 x 20 panel: volumes, bend angles and allowances, flat areas;
  refusals). Import: `import-sheet` `hems_past_half_a_turn_are_flanges_with_their_form`
  and the ctest `f3d.reference_hems` (the six reference hems parametric
  and verified).

### Implementation notes (mitcad#1383)

The Flange panel edits what the `.f3d` and `.ipt` imports give a flange:

- **Hem form.** Hem (None, Teardrop, Rope, Double) with Gap and a double
  hem's Setback reads and writes `hem`. With a hem the panel hides Angle
  and Measured and writes the length mode `outer_tangent` and an angle of
  180 degrees to the side of the loaded angle and Flip (the model takes
  only the angle's sign, and the import's angle is that 180 degrees).
- **Width.** Width (Whole Edge, Offsets from the Ends, From the Middle)
  replaces the Offsets from the Ends check: From the Middle reads and
  writes `width` (`before`, `after`, and `root` with Own Overhang Root),
  which the panel dropped before.
- **Choices.** Measured adds Outer Tangent Length (`outer_tangent`) and
  Position adds Tangent to Side (`side_tangent`), the imports' flanges'
  references, which an edit had replaced by the first choice. The Angle
  takes more than 180 degrees and the Length 0 (a rolled hem) as the
  model does; the model refuses what it cannot build.
- **Test.** `tools/ui-sheet-flange-hem-test.sh`: a teardrop hem loaded and
  kept by an edit, changed to rope and double in the panel and each
  loaded and kept, a flange of 90 degrees whose width from the middle
  passes the edge's ends (root 0, then 3), a rolled hem of 270 degrees
  and length 0; volumes as `cli.sheet_flange_hem_past` and
  `cli.sheet_flange_overhang`, the saved definitions read back.

### Corner joints of a closing flange (mitcad#1410)

A flange whose wall closes against another wall of the same sheet (the
last side of a box section closed by welding) takes a corner joint,
`corner` (commands.md, "Corner joints of a closing flange"). Its length
is then the closed section's inside dimension (length mode `inner`), and
the joint sets the wall's end from the thickness `t`:

- **Open** (`open`): the wall ends on the other wall's inner surface; the
  two walls meet only along the corner line, which a sheet body holds as
  it is (mitcad#1409). The inside dimension is exact.
- **Half Open** (`half_open`): the wall reaches `t / 2` further, over half
  of the other wall's end. Walls that touch over a face give no sound
  solid (OCCT's checker refuses it and booleans and fillets on it fail,
  mitcad#1409), so the flange cuts the other wall back by a seam of
  0.00001 mm (10 nm, `HALF_OPEN_SEAM`) where its wall lies over it. That
  seam is the only clearance: the smallest that is a gap at all, ten times
  the 1e-6 mm modelling tolerance (the contact check accepts walls
  0.0000005 mm apart and refuses them at 0.0000001 mm; gaps from
  0.000001 mm to 0.01 mm all gave a valid body, a cut through the corner
  and a hole). The inside dimension stays exact; the other wall's end is
  the flange's generated curves 60 to 63.
- The other wall is found by geometry: the flat wall normal to the
  flange's wall whose material lies under its last `t / 2` and whose end
  lies on its surface. None, or more than one, refuses the flange. The
  joint belongs on the wall that closes the section, added last.
- The list is open for further joints (flush, overlap).
- **Panel.** Corner (None, Open, Half Open) on the Flange panel, beside
  Hem (one or the other); with a corner Measured is hidden and the panel
  writes Inner Length. Tests: `cli.sheet_flange_corner` (Open and Half
  Open on a 1 mm sheet, 10 x 10 inside, Outside flanges by Inner Length:
  valid bodies, volumes, flat patterns with the walls at the strip's
  ends, a cut through the half open corner and a hole);
  `tools/ui-sheet-flange-corner-test.sh` and the ctest
  `app.ui-windows-sheet-corner` (the corner loaded and kept by an edit,
  Half Open chosen, saved and read back, None showing Measured).

### Implementation notes (mitcad#1442)

- **A second path names the feature that closes it.** A Fold (or a
  virtual-sharp Bend) whose moving side reaches its stationary panel
  another way too, through an earlier bend, is refused (no rigid fold can
  turn one half of that bend and keep the other). `branch()` in
  `sheet/folded/bends.rs` returns the path it found (`SecondPath`: the
  domains from the moving branch's start to the stationary panel); the
  refusal names the features of the bends on it (with none, of its panels
  besides the stationary one; never the feature being made) as the
  browser shows them, and says what to do instead: "This fold would also
  have to bend SheetFold1, whose bend joins both sides of the fold line.
  Fold here before SheetFold1, or make SheetFold1 with a separate line on
  each side." A feature that is not a fold is to be changed so that it
  does not join both sides; a Bend's refusal says to bend before it or to
  choose panels it does not join. The sheet's internal graph is no longer
  named.
- **Tests.** Model: the refusal's unit test in `bends.rs` (the path, the
  features on it, the messages). CLI `sheet_fold_loop`: a U-shaped panel
  (`3600`), a first fold across both arms (`3600 + 2 pi`) and a second
  fold between the arms, refused naming the first.

## Kernel seam

Add one sheet-metal family to `core/model-base/src/kernel.rs`, using default
`KernelError::Unsupported` methods. Put concrete OCCT construction, history and
transport in a new geometry family with corresponding bridge files. Logical
interfaces use two new operations; model-side graph helpers compose them into
the feature commands. Concrete Rust lifetimes and FFI transport wrappers can
follow existing families, but both inputs and labelled outputs are required:

```text
sheet_trim_domain(SheetTrimSpec) -> SheetTrimResult
sheet_shape(SheetShapeSpec) -> SheetGeometry<Shape>

SheetTrimSpec = { operation, named_2d_domains, named_tool_loops, seam_pairs,
                  tolerance }
SheetTrimResult = { closed_domains, curve_lineage, seam_splits,
                   surviving_paired_intervals, diagnostics }
SheetShapeSpec = { thickness, patches, tangent_ports, tolerance }
SheetGeometry = { solids, domain_membership, naming_history, diagnostics }
```

`sheet_trim_domain` performs union/difference/intersection of bounded named
planar material domains, returning closed loops and provenance of generated
segments. Ordinary body Booleans do not currently provide this output. The
operation splits matching seam curves consistently and exposes disconnected
domains; it cannot discard all but the first piece.
Difference and intersection apply to each named subject while reconciling its
paired seams. Union accepts one subject domain so added material has an explicit
owner. Output boundaries retain their actual bounded support and traversal;
lines use normalized support parameters, conics unwrapped radians and splines
their knot parameters. Every surviving seam interval names both retained sides.

`sheet_shape` receives resolved values and immutable domains. A planar patch
contains its tagged semantic ID, resolved front frame, material parity and
trimmed domain. A cylindrical
patch contains its ID, axis/radial frame, `r`, signed angle, K, material-side
parity and trimmed neutral domain. Tangent ports pair oriented boundaries with
front/back correspondence. Construct genuine inner/outer cylindrical surfaces,
their trimmed wires and thickness walls, sew them and validate the result.
Return every solid with domain membership; the model allocates any split
`BodyUid`s and validates ownership before atomic publication. The kernel cannot
assign document IDs, mutate rules, read UI state or write files.

The model owns graph traversal, opened-bend configuration and semantic IDs. It
turns Face/Flange/Bend/Fold into patch/domain edits, uses `sheet_trim_domain` for
reliefs/corners/cuts, and constructs either folded or partially developed
patches through `sheet_shape`. Flat contours come from the same resolved
domains. Frame, port, chart and geometry validation occur before reporting
success. The initial proof must demonstrate that this seam carries enough
information; missing transport information must extend it, not be hidden in
an opaque mutable OCCT object outside the versioned state.

For OCCT construction, the cylindrical chart follows the documented
[`Geom_CylindricalSurface` parametrization](https://dev.opencascade.org/doc/refman/html/class_geom___cylindrical_surface.html),
using surface `U=u/Rn` and `V=v`. OCCT also provides
[`BRepBuilderAPI_MakeEdge` overloads for a 2D curve on a surface](https://dev.opencascade.org/doc/refman/html/class_b_rep_builder_a_p_i___make_edge.html).
These establish the construction primitives, not a ready-made sheet-metal
algorithm. Our implementation maps named chart curves to cylinder pcurves and
keeps their common parameter on both thickness surfaces. General cut curves
need not be conics in 3D; do not independently resample their two wall rails by
arc length. Radial material generators must still correspond.

[`BRepLib` supplies 3D-curve construction and SameParameter operations](https://dev.opencascade.org/doc/refman/html/class_b_rep_lib.html).
Check their success and measured approximation error against the selected
tolerance; retain exact support cylinders while allowing bounded approximation
of general 3D edges/walls. Apply repair operations only to newly built shapes,
preserving immutable input handles. Existing named-solid/sewing helpers can be
reused, but native sheet construction must not depend on importer internals.

The same transport operation is used for internal Cut and explicit
Unfold/Refold. Do not implement a second inverse by heuristically recognizing
faces in the unfolded B-rep. The API must carry curves/domains sufficiently to
rebuild trimmed cylindrical faces and cut walls, not only a list of rigid
transforms. Detailed diagnostic codes distinguish missing reference, bad rule,
short flange, inconsistent seam, non-developable cycle, collision, flat overlap,
unsupported surface and invalid solid. Cancellation and out-of-memory are
global operation failures, never optional per-face diagnostics to ignore.

## Proposed JSON surface

The following definitions illustrate the intended native schema. IDs assume
earlier objects in the document; these are schema examples rather than a
single executable script. Number slots use existing `ValueInput` conventions
and resolve to `ParamId` in saved feature definitions. A missing optional
override inherits the rule; an explicitly invalid value is not an inheritance
request. Unknown enum variants are errors.

```json
{"type":"sheet_rules","thickness":1,"bend_radius":2,
 "allowance":{"type":"k_factor","k":0.4},"reference_side":"front",
 "bend_relief":{"type":"round","width":1,"depth":3},
 "corner_relief":{"type":"rectangular","width":1,"depth":3}}
```

```json
{"type":"sheet_face","rules":"F2",
 "profiles":[{"sketch":"F1","region":"r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}]}
```

Panel/bend references below are typed semantic IDs, not traversal indices.
Boundary selections retain typed source curves even when a coplanar Boolean
removes the corresponding physical edge. A UI selection must resolve to that
semantic reference before creating the feature.

```json
{"type":"sheet_flange","body":"F3.b0",
 "boundary":{"panel":{"feature":"F3","slot":0},
             "curve":{"type":"sketch","sketch":"F1","segment":"c2[c1,c3]"}},
 "angle":1.5707963267948966,
 "length":{"mode":"tangent","value":12},"position":"bend_line",
 "offsets":{"start":0,"end":0}}
```

```json
{"type":"sheet_bend","body":"F3.b0",
 "stationary":{"panel":{"feature":"F3","slot":0},
               "curve":{"type":"sketch","sketch":"F1","segment":"c2[c1,c3]"}},
 "moving":{"panel":{"feature":"F3","slot":1},
           "curve":{"type":"sketch","sketch":"F2","segment":"c4[c3,c1]"}},
 "port_mode":"virtual_sharp","angle":1.5707963267948966,"radius":2}
```

```json
{"type":"sheet_fold","body":"F3.b0","panel":{"feature":"F3","slot":0},
 "line":{"sketch":"F5","curve":"c1"},"stationary_side":"left",
 "angle":-1.5707963267948966,"position":"bend_line"}
```

```json
{"type":"sheet_corner_round","body":"F3.b0",
 "corners":[{"panel":{"feature":"F3","slot":0},
             "incoming":{"type":"sketch","sketch":"F1","segment":"c4[c3,c1]"},
             "outgoing":{"type":"sketch","sketch":"F1","segment":"c1[c4,c2]"}}],
 "radius":2}
```

```json
{"type":"sheet_corner_chamfer","body":"F3.b0",
 "corners":[{"panel":{"feature":"F3","slot":0},
             "incoming":{"type":"sketch","sketch":"F1","segment":"c4[c3,c1]"},
             "outgoing":{"type":"sketch","sketch":"F1","segment":"c1[c4,c2]"}}],
 "distances":{"first":2,"second":3}}
```

```json
{"type":"sheet_unfold","body":"F3.b0","stationary":{"feature":"F3","slot":0},
 "bends":[{"feature":"F4","slot":0}]}
```

```json
{"type":"sheet_cut","body":"F3.b0","stationary":{"feature":"F3","slot":0},
 "open_bends":[{"feature":"F4","slot":0}],
 "profiles":[{"sketch":"F8","region":"r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}]}
```

```json
{"type":"sheet_refold","body":"F3.b0","unfold":"F7",
 "bends":[{"feature":"F4","slot":0}]}
```

All modelling examples use existing `add_feature`/`edit_feature` transactions.
No command success means merely that a fallback solid was retained; feature
status and sheet validity are separate, observable outcomes. Preview reports
include affected bodies, semantic selections, bend configuration, validity and
diagnostics so native and retained UI use the same result.

## Flat Pattern, UI and DXF

Store one optional `FlatPatternDef` per selected sheet metal body in the document's
derived-item collection, following the persisted analysis/settings pattern.
It is evaluated against that body's current timeline result, including edits
after item creation. It does not capture the body at a new timeline feature
position. It stores a fixed panel, an in-plane orientation reference and export
preferences; its display name is not a dependency identity. Proposed commands
are `set_flat_pattern` and `remove_flat_pattern` keyed by `BodyUid`, with
`sheet_state` and `flat_pattern` queries and `export_flat_pattern_dxf` export.
Register pure queries with the MCP allowlist and apply the existing authorized
file-write policy to export.

```json
{"cmd":"set_flat_pattern","body":"F3.b0",
 "stationary":{"feature":"F3","slot":0},
 "x_axis":{"type":"panel_x"},"reference_side":"front"}
```

```json
{"query":"flat_pattern","body":"F3.b0"}
```

A query returns `status`, the source sheet version, fixed frame, closed outer
contours and holes, bend centrelines, optional tangent lines and diagnostics.
It returns no usable export payload for invalid, stale or unavailable states.
Use a cache separate from modelling side effects. Its key includes sheet
version, marker-visible configuration, fixed panel/orientation, reference side,
allowance and tolerance/algorithm versions. Renaming a body changes labels,
not geometry; changing thickness, radius, K or a cut changes the result.
The key also includes per-body validity generation, and lookup/export first
checks current blockers. A failed later feature can leave `sheet_version`
unchanged while making a formerly cached flat pattern unavailable.

The Sheet Metal command group exposes every requested feature. Rule controls
come from the parameter system. The browser shows the body, its rules and its
Flat Pattern child, with an explicit folded/flat display choice. Highlighting a
bend or panel uses semantic selections. Selecting a different occurrence cannot
silently change definition-level editing to an instance-only operation.

Preview is computed from a temporary document checkpoint. Cancel leaves no new
rule, body, item or parameter behind. Long operations are cancellable. Failed
or stale previews disable acceptance/export and identify the failed input.
Every command has retained and native presentation through shared state, not
separate geometry logic. Large bend lists are bounded and scrollable.

DXF output uses explicit units and one selected 2D frame. Export each physical
cut contour once; do not emit duplicate front/back edges or tessellation seams.
Use `CUT_OUTER`, `CUT_INNER`, `BEND_UP`, `BEND_DOWN`, and optional
`BEND_TANGENT` layers. Cut contours are closed with preserved holes. Bend
centrelines are at `u = BA/2` and are clipped to remaining material; tangent
lines lie at `u = 0` and `BA`. Up/down is the signed rotation relative to the
exported reference side; changing that side reverses the classification.
The export includes all bends in their manufacturing configuration, even if
some are temporarily open in the modelling timeline. Round-trip tests through
`core/dxf` verify dimensions, entities, layers, direction and units.

## Lifecycle, persistence and failure policy

| Event | Required observable result |
|---|---|
| Parameter/rule edit | Recompute dependent shapes, sheet domains and flat result; invalidate all old versions together. |
| Suppression / rollback | Use the actual earlier state and its opened bends; no future bend, trim or flat geometry remains visible. |
| Failed sheet operation | Keep the earlier body for normal model error display, but block its current flat export with the failing feature identified. An earlier successful result is not a successful current result. |
| Cancellation | Restore the entire command checkpoint; publish neither partial geometry nor partially edited graph/parameters. |
| Undo / Redo | Restore definitions, allocations, rules and derived-item settings, then valid matching result versions. |
| Deleted target/rule | Apply normal explicit dependency handling; retained unresolved references report failure, never retarget another sheet or all bends. |
| Save / reopen | Recreate the same names, rules, configurations, contours and ownership; missing cache data is recomputable. |
| Copy / import | Remap every semantic reference; never bind an external source ID to an unrelated equal local ID. |

Track failures for the affected sheet metal bodies separately from the last valid
shape retained by recompute. A failure unrelated to a sheet does not block its
flat item. For a feature with automatic participants, determine affected sheet
inputs conservatively before geometry evaluation; failure cannot erase the
evidence that the operation intended to modify them. Suppression is an explicit
bypass and therefore differs from failure.

Store an authoritative affected-sheet footprint and dependency reads with
failed outputs. Automatic participant discovery records body-set and sheet-state
reads before any kernel call; a newly available participant must invalidate
both the cached failure and its blocker set. Preview diagnostic metadata is
not the source of this footprint. Downstream sheet operations read the blocked
state and fail rather than building on the last valid shape and clearing an
earlier error. A blocker disappears only when its responsible feature succeeds,
is suppressed, lies after rollback, or is removed. Query and export use the
same validity rules.

Native feature definitions and flat settings are the source of truth in file
v2 and project snapshots. Old files default to no sheet rules/state/items and
keep existing solid behaviour. Unsupported future definitions fail clearly.
Evaluated sheet data in the disk result store uses a versioned schema and is
accepted only with its matching shape, naming history and kernel/algorithm
version. Restore them atomically or recompute all of them. Initial implementation
may disable disk caching for sheet features until this paired restore exists;
it must not restore only the shape and claim sheet validity. Account for domain
curves and correspondence in cache memory estimates.

## Import contract

Native definitions and stable naming must be settled before decoder/replay
changes. Coordinate the `.ipt` decoder, history grouping and reference matching
with the work in mitcad#60; parallel
native modelling work does not authorize overlapping importer edits.

The `.ipt` integration maps thickness/rule references, material side, signed
angle, radius, length/position enums, offsets, reliefs and source face/edge
references into the native features. Grouped history snapshots validate each
replayed result but cannot supply missing parameters by assumption. Sheet holes
must resolve against the actual material face/chart, including transformed
parts. The `.f3d` integration decodes flange inputs and uses the same native
contract, with explicit errors for undecoded fields.

Implementation (mitcad#211, first part): the decoders write sheet items
(`core/import/SCHEMA.md`, *Sheet metal*) and the import replays them
(`core/import/docs/f3d-sheet-metal.md`, *Sheet metal*; the `.ipt` records in
`core/ipt/README.md`, *Sheet metal*): `.ipt` new-body faces with the
style's rules, flanges on one edge and cuts through the sheet; `.f3d` base
flanges with the design's rule and edge flanges on one edge at the inside
or outside position. Not yet: joined faces, multi-edge and partial-width
flanges, flanges at the edge (adjacent or tangent positions), hems,
corners, bends, folds and cuts reaching a bend; each falls back with its
reason. Since mitcad#242: faces joined in a panel's plane, `.f3d` hems,
side-tangent flanges and widths, straight `.ipt` cuts (through bends as
overlays), and sheets kept through ordinary features by the sheet-domain
adapter (see its [notes](#implementation-notes-mitcad242)).

`.ipt` edges by their names (mitcad#242): the edges flanges, corner rounds
and chamfers and bends select are read from the file's topological names
and found in the history state each item starts from (`core/ipt/README.md`,
*Topological names*); a flange on several edges is one flange per edge read,
with the file's width extents as offsets. No edge is chosen among
candidates; an edge not read makes its item fall back with the reason.

Report parameter replay, geometric match and retained fallback separately.
A stored B-rep may preserve an unsupported item's geometry, but it has no
invented sheet semantics and must not increment parametric acceptance counts.
Public tests use synthetic data. Private corpus reports refer to real designs
only by content ID. Both import paths remain required for parent completion
unless its scope is explicitly changed.

## Delivery and acceptance map

The dependency chain is deliberate. The cross-bend proof starts with the
geometry work and gates later phases; it is not deferred until UI completion.
Each phase publishes its own evidence while the parent tracks the full scope.

| Phase | Deliverable and minimum acceptance evidence |
|---|---|
| mitcad#205: contract | This state, geometry, naming, API and lifecycle contract; explicit unsupported cases; full issue-to-test map. Documentation only is not functional acceptance. |
| mitcad#206: rules and Face | Parametric rules and Face with holes/islands; atomic state; mock lifecycle/cache tests; actual thickness, area, ownership and named faces; old-file and copy compatibility. |
| mitcad#207: folded features | Flange, Bend, Fold, Corner Round and Corner Chamfer; all length/position/sign/offset modes; minimum-length checks; early nonlinear development/cut/refold proof above; persistent names after rule edits. |
| mitcad#208: configurations and Flat | Partial Unfold/Refold, whole-body derived Flat Pattern, graph/overlap checks, fixed-frame stability and valid round-trip correspondence. Actual OCCT L/U/Z cases; repeated instance display and no extra physical body. |
| mitcad#209: cuts and reliefs | Production thickness-normal Cut crossing one/two bends, holes and multiple contours; cuts between Unfold and Refold; rectangular/round/tear and corner reliefs; invalid boundary/short-flange cases; rule edits after cuts. |
| mitcad#210: UI and DXF | Every command and browser rule edit; live preview/Cancel/Undo; native and retained UI coverage; fixed orientation; DXF contour, hole, bend direction/layer/unit round trips; invalid export refused. |
| mitcad#211: imports and final acceptance | Coordinated `.ipt` Face/Flange/Bend/Fold/Cut/corners and sheet-hole replay, `.f3d` flange replay, truthful fallback diagnostics, reference matching and scoped corpus evidence; every parent acceptance row satisfied. |

The focused regression matrix spans each new family's model tests and real
kernel tests. Mock tests validate dependencies, state, errors and transactions;
they cannot prove nonlinear geometry or valid solids. Geometry tests assert
analytic dimensions, radii, thickness, contour/face correspondence and trim
locations in addition to volume and bounding boxes. Required lifecycle cases
are repeated recompute; upstream thickness/radius/K/sketch edits; suppression;
rollback; cancellation; injected failure; Undo/Redo; save/reopen with and without
disk cache; copy/remapping; deleted references; and nested translated/rotated
occurrences. Stable identity tests must compare semantic IDs and geometry.
Model save/reopen tests must explicitly recompute after loading definitions
before inspecting derived sheet or flat results.

Run affected tests in the host's approved isolated build environment under the
normal scoped validation policy. The design document itself needs publication
checks, not a build. Record unimplemented or unvalidated acceptance rows openly;
do not close #125 based on successful early phases.

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

The initial allowance law is a K-factor. A table of measured bend allowances
can be added as another typed law later. The Bend Table export instead
reports the bends already calculated for a particular sheet. The
initial surface family is planar panels and constant-radius cylindrical bends.
Since mitcad#2435 a cylindrical or conical face converts into a curved panel
of its own ([notes](#implementation-notes-mitcad2435)). Doubly curved,
variable-thickness and stretched material, embossing and
arbitrary B-rep-to-sheet recognition are outside this contract. Unsupported
surfaces must produce a specific error, never an approximate flat pattern
reported as exact. Since mitcad#2442 the user may ask for an approximate
development by triangulation of faces that do not lie flat exactly (duct
transitions); such a pattern says so and how far its lengths are from the
faces' ([notes](#implementation-notes-mitcad2442)).

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
| `bend_relief` | A rectangular or round slot and default dimensions for a flange, bend or fold that ends inside the sheet. |
| `corner_relief` | A shape and size for the Corner Relief feature and for the corners where a Flange's bend meets another bend (mitcad#2342). The walls of a flange on several edges still meet by its mitres and corner gaps. |
| `flange_corner_relief` | Optional version flag (mitcad#2342): true when flanges cut `corner_relief` at their corners. Rules saved without it keep their corners until edited; adding or editing rules sets it. |

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
Fragments of one curve can carry the same piece, each a curve with a range of
its own (a cut along an existing side leaves two, mitcad#2117), so a source
span can lie in several by parameter: then the output edge decides, the
fragment its geometry lies on, or the names of every fragment it lies on; a
span on none of them stays an ambiguous alias.

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
| Body pattern/copy or mirror | Preserve semantics only through a sheet-aware copy/handedness adapter with deterministic names; otherwise invalidate the copied result explicitly. Mirrors and patterns of bodies have that adapter, as new bodies and joined (mitcad#1450). |
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

A flange may take several connected edges, one wall each, mitred where
they meet with a corner gap ([a flange on several
edges](#a-flange-on-several-edges-mitcad1872)).

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

A corner beside a bend (mitcad#1765), whose one boundary is the panel's
tangent line to a bend, is a corner of the developed outline: the bend is
opened into the panel's plane (`reconfigure`), the corner is taken between
the panel's other boundary and the bend's side at the vertex, and the tool
is removed from every chart in that plane with `developed_cut`, as Sheet
Cut removes its profiles; the bend then folds again. The straight side the
corner may take runs along the bend's side and on along boundaries in line
with it (the side of the panel beyond the bend). An inside corner there
would add material across the bend and is refused, as is a corner between
two bends. An ordinary Fillet or Chamfer on such a corner's thickness edge
(`corner_at` finds the corner) goes the same way
(`folded::dressup_corners`), so the body stays a sheet. CLI:
`cli.sheet_corner_beside_bend`, `cli.sheet_corner_beside_bend_fillet`.

Rectangular relief is a finite-width slot with a straight end; round relief is
a slot with a semicircular end, with its entered depth measured to the furthest
tip. Both enter the bend region far enough to separate the moving material
from adjacent stationary material. Validate width, depth, available material
and tangency. A tear would slit the bend zone through the thickness and leave
the material on both sides intact. The chart kernel trims positive-area
regions and the unfolded slit would create coincident opposing walls in a
single solid. The flat pattern also needs a single cut line for the slit.
New bend reliefs therefore offer Rectangular and Round. The exact tear
construction must pass the early geometry proof before its UI is enabled.
Any finite clearance required by a valid solid would be an explicit size
in the flat pattern. Saved Tear bend reliefs keep their original rectangular
slots and contour references with a compatibility warning.

Corner Relief acts on the selected junction of labelled bends. It follows
the bound Sheet Rules unless the feature specifies its own relief. Flanges
on several edges use their own mitres and corner gaps. Every
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
  inside material): rectangular slots of the entered width and round slots
  whose depth includes the tip. A saved bend `tear` retains the same
  square-ended slot it made in earlier versions and is shown as **Legacy
  Tear Slot** when editing it.
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
  relief's size is `width`. Sheet Rules keeps the old corner-relief depth
  field in the definition and hides it in the panel. Defaults come from the bound rules'
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
- **Corner relief at flange corners** (mitcad#2342,
  `relief::flange_corner_reliefs`). A Flange cuts the rules' `corner_relief`
  where its bend meets another bend at a corner of the panel both leave: a
  flange on the next edge (the two bends' seams on the panel end at one point,
  not parallel) and the corners between the edges of a flange on several edges.
  The junction is developed as for the Corner Relief feature, with the same
  `J0` and `C`. Both constructions leave the crossing zone open: a later
  flange's trim cuts the earlier bend back to its tangent line, and a flange on
  several edges opens a corner it relieves (mitcad#2411, `flange_edges::Opened`:
  each bend runs on to the other's tangent line, as far as its wall's root
  covers it, instead of ending where it clears the bisecting plane; a reflex
  corner, a bend past a half turn or a corner where that line would end the
  bend sooner keep the bisecting plane). So the relief is measured from `J0`
  outward (mitcad#2411, `relief::Placement::Corner`): Tear adds nothing (the
  bends meet at `J0`), Rectangular removes the crossing zone grown by the
  width toward the panel (`-w <= s <= BA` on both strips: the bends end `w`
  short of each other's tangent line, the panel loses a `w` square at its
  corner), Round a disc of diameter `w` centred at `J0`. Every width cuts, so
  the three types differ at every corner (mitcad#2342 centred them at `C`,
  where a width under the narrower allowance cut nothing). A reflex corner
  (the bends' middles past each other's tangent lines) is not relieved.
  Corners are not relieved when
  the rules' `flange_corner_relief` is not true (a version flag that `resolve`
  sets on add and edit, so saved designs keep their corners until their rules
  are edited), when the process is not metal, by a flange (or at a bend of a
  flange, bend or fold) with its own `relief`, across a recorded corner gap,
  or when a `sheet_corner_relief` names the two bends. That feature relieves
  the corner instead; the flange reads its definition through
  `Read::CornerReliefs`, like the flange-wall plane's `Read::WallFlange`.
  Imports set the flag false. CLI fixtures: `sheet_flange_corner_relief` (two
  flanges on adjacent edges), `sheet_flange_edges_corner_relief` (a flange
  on three edges) and `sheet_flange_corner_relief_types` (the reported case:
  a flange on four edges, the reporter's rules, Rectangular, Round and Tear of
  1 mm and the flag false, by analytic volumes and flat areas): Rectangular,
  Round and Tear, widths under and over the allowance, the flag, a flange's
  own relief and the override, by analytic flat areas; `sheet_flange_adjacent`
  by its volume. The fixtures of the miters themselves (`sheet_flange_edges`,
  `_gap`, `_bend`, `_existing`, `sheet_flange_on_wall` and the
  `sheet-flange-edges` UI script) give their rules `flange_corner_relief`
  false and keep the corners designs made before have. Unit tests:
  `relief::tests` (the grown zone), `flange_edges` tests (where an opened
  bend ends).

### Implementation notes (mitcad#210)

Decisions made while implementing the command UI and the DXF export:

- **Commands on the model API.** The Sheet Metal tab (a group of the Solid tab
  until mitcad#1975)
  (`app/commands/SheetCommands.cpp`, [app/COMMANDS.md](../app/COMMANDS.md#sheet-metal))
  has Sheet Rules, Edit Sheet Rules, Face, Flange, Bend, Fold, Corner Relief,
  Corner Round, Corner Chamfer, Sheet Cut, Unfold, Refold, Flat Pattern and
  Show Flat Pattern, in that order and in sections (mitcad#1778); Export Flat
  Pattern is on the Laser tab. It keeps no sheet model: picks
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
  with that reason. A change that removes material from a folded bend is
  a cut across bends (`sheet/rederive_bends.rs`, mitcad#1628): the result
  must hold no material the input did not; every chart's footprint comes
  from the result's faces on its front surface (a bend's from the kernel's
  `sheet_cylinder_footprint`, the faces' parameter curves mapped affinely
  into the bend chart: `u` the angle times the neutral radius, `v` the
  axial position); in a bend the cut must also leave the back surface
  where it left the front; the lost material of all charts that develop
  into one plane with the touched bends open is merged into one set of
  tools and cut by `developed_cut`, as Sheet Cut does. The rebuilt sheet
  is the body's material: where it differs from the feature's result
  (only inside the touched bends; anything else is refused with the
  volume that differs), the body becomes the rebuilt sheet and the feature
  warns that the cut is normal to the sheet there. A sheet feature on an
  invalid sheet names the feature and its reason in words and points to
  Sheet Cut (`EvalContext::require_sheet`). The Flat Pattern of an invalid
  or blocked sheet gives the same text as its reason, shown in the
  browser's Flat Pattern row, from the one helper `sheet::refusal`
  (mitcad#1636); the ids stay in its `causes`. CLI `sheet_cut_across_bend`.
  The Flat Pattern's other reasons, its export refusals and the refusals
  of `set_flat_pattern` and `remove_flat_pattern` are in words as well
  (mitcad#1639): the body by its name as the browser shows it, or "this
  body", walls and overlays by their features' names; the ids are in the
  `details` of the `flat_pattern` query and in the error's detail. The
  browser's Flat Pattern row says it with one whole message per status
  ("Flat Pattern of Bracket is invalid: …", `is blocked`, `is
  unavailable`), its reasons translated as model messages. Test:
  `sheet_flat_tests::flat_pattern_errors_name_the_body_in_words`.
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
- **Flanges on edges with oblique sides (mitcad#1576).** A flange set back
  into its panel (Inside, Outside, Bend Line, the side tangent) cut the
  panel with a rectangle over the selected boundary only. Where a side met
  the boundary at less than 90 degrees, the panel's new boundary along the
  setback line was shorter than the bend ("one side of the seam ends at
  2.8868 mm and the other at 0.0000 mm" for a 60 degree side and Inside);
  at more than 90 degrees the cut left a sliver of the panel beside the
  bend's end, a step in the outline. An end of the whole boundary (no
  offset, no overhang) now follows the panel's outline from that end while
  it runs straight inward (a side, or a short side and the next one) to
  its crossing of the setback line: the cut reaches as far past the end as
  that outline does, so the side runs on to the line at its own angle, and
  the bend and its wall span the panel's new boundary along the line (the
  developed width is the panel's width there, not the boundary's). An
  outline that turns before the line (an arc, a joined boundary, a side
  parallel to the boundary) is cut as before. CLI `sheet_flange_oblique`:
  plates whose edge meets a side at 60, 75, 105 or 120 degrees at one end
  or both, a 3 mm square side before a side that narrows the plate, and a
  short side at more than 90 degrees before one; Inside, Outside and Bend
  Line each: volume `2 A + 4 pi w + 30 w` (t 2, r 3, a 15 mm wall; `A` the
  plate's area past the setback line, `w` its width there) and the flat
  pattern `A + (BA + 15) w` in one outline whose sides run straight.
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
- **Walls kept apart by planes (mitcad#2444).** That check intersects
  every pair of faces of the solid whose boxes meet, and the charts' sides
  are ruled B-spline faces: on a plain 200 x 100 plate it took 90 to 95
  percent of each Flange preview (0.12 s for a flange on one edge, 0.37 s
  on three, 0.55 s for the fourth wall, in a debug build). Before it runs,
  each pair of the shell's material regions is looked at through its exact
  support function (the most of `m . x` over the region's material: exact
  for straight and circular sides, over the poles of the exact B-spline
  form for other curves; a bend along its outline at its inside and
  outside radius, exactly along straight chart sides, a curved side such
  as a relief's arc in 32 pieces each by its chart box): a plane among the
  shell's caps' planes, the planes of its straight sides, its bends' end
  and port planes, the global axes and the planes halfway between one of
  each region's (the bisecting plane of a corner, which separates its two
  bends and is no side's plane, as walls are cut normal to their surfaces)
  that has two regions no port joins more than four
  tolerances apart on its two sides, or two a port joins at most touching
  within the tolerance, shows that they cannot meet. When that is shown
  for every pair, the check could find nothing and is not run; otherwise
  it runs as before (in parallel, about three times as fast on the plate),
  so what is refused and kept is unchanged. A region on its own is the
  validity check's. With `MITCAD_LOG_TIMING` each construction says how its
  shells were found not to meet (`shells kept apart by planes 1, checked
  for collisions 0`), and names the first pair it could not show apart.
  CLI `sheet_flange_plate_timing` (the reported plate, step by step as the
  panels send it, `mitcad-cli run --timings`). There every shell is kept
  apart by planes and the Flange previews take, in a Linux debug build,
  before and after: one edge 153 and 42 ms, two edges 287 and 34 ms, three
  edges 410 and 49 ms, the fourth wall 607 and 64 ms, an edit's preview 622
  and 63 ms, the Flat Pattern 425 and 52 ms; the body is the same (volume,
  area, faces and edges). OK takes what the preview computed (under 1 ms in
  the model).
- **Only the walls planes do not keep apart are checked (mitcad#2536).** A
  folded box whose tabs go through slots of its base (the corrugated board
  folders, CLI `sheet_board_0427`) has a pair of regions no plane keeps
  apart once a tab is in, and every Fold after it then paid, by the
  construction's path, for the whole check: the sheet built with its holes filled, the self-interference
  check of all of it (up to 230 faces), which the tab then meets in the
  filled base; every region of it sewn on its own, checked and compared by
  booleans pair by pair, only to name the walls that met in a construction
  whose refusal was thrown away; and then the sheet itself built and
  checked whole. 1.7 to 7.8 s for each of the box's Folds, growing with
  each. Now the check looks at the faces of the regions not kept apart from
  the rest of their shell only (a tab and the base it goes through): a
  face of any other region is on its own side of a plane from every region
  of the shell, so it meets nothing, and two walls that meet are two of
  those regions, which are all the naming compares. The construction with
  the holes filled checks its shells and stops (it names nothing, and any
  collision there only means the sheet itself is checked), and is built
  only when the regions the check looks at hold 16 holes or more (a vent
  pattern; a few slots cost the check less than building the sheet a second
  time). The self-interference check no longer runs OCCT's validity check
  of the solid a second time. What is refused and kept is unchanged
  (geometry test `sheet tab through a window`: a tab through a window of
  the base, 2 mm clear, kept with one hole and with 17; across the base's
  material, refused naming the base and the tab's panel). With
  `MITCAD_LOG_TIMING` a construction says how many of its shells' regions
  were checked (`checked for collisions 1, 2 of 7 regions checked`) and
  marks the one with the holes filled (`Timing sheet chart shape (holes
  filled)`).
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
  measured. Every other edge between two panels is a rip (mitcad#1949):
  the panels are cut apart there with the corners of a flange on several
  edges (mitcad#1872): the bisecting plane of the two faces with half the
  corner gap (default the thickness) each way, or one face running on to
  the other's far surface (earlier or later in the order the faces are
  reached), cut normal to the sheet where the plane meets the surface that
  reaches further. The bends at a rip's ends end square where all their
  material clears that plane; the panel before such a bend is cut back to
  its tangent line beside it (a corner relief). A bend runs only as far
  as both faces reach its tangent lines, so a wall whose ends are not
  square to its bend (a vertical side wall on an inclined base) converts,
  running past the bend at one end and short of the edge's end at the
  other. All panels are placed first, from the faces as the body has
  them; then cut at the rips, then back to their bends' tangent lines;
  the bends are made last, between the cut panels. Not supported, each
  refused with the edge or face: loops of bends (leave an edge out of the
  bends to rip it open), curved faces, chamfers or rolled bends picked as
  bends, a rip that cuts a face away whole. CLI `sheet_convert`: a 40 x
  20 x 2 plate (1600, a 3 thick one refused), an L of 2 thick walls bent
  at its outer edge (`6000 + 200 pi`, radius 1: `6400 + 100 pi`), a solid
  block becoming a U channel (`3600 + 240 pi`, flat `30 (60 + 3.6 pi)`),
  then an L with a rip, the unconnected bend refused, reopened. CLI
  `sheet_convert_tray`: a level tray from a 60 x 40 x 20 box (walls `2 +
  sqrt 2` and bends `5 + sqrt 2` short of each corner, flat `4260 - 120
  sqrt 2 + 1.8 pi (160 - 8 sqrt 2)`), picked rips, the rip refusals, and
  an inclined tray (the base rising 6 over 60, vertical walls, a level
  rim) with its flat pattern. The panel is Convert to Sheet Metal in the
  SHEET METAL group (`app/COMMANDS.md`); UI test
  `tools/ui-sheet-convert-test.sh`.
- **Perforate** (`sheet/folded/perforate.rs`). The holes are real: cut
  through the material as a developed cut with every bend open, so the
  body, the Flat Pattern, the DXF (`CUT_INNER` circles) and every measure
  have them, and later features work on the sheet with its holes. The
  grid lies in the chart of the stationary panel (default the lowest,
  as the Flat Pattern's) with the whole sheet developed, from the lower
  left corner of the developed material's box, `margin + d/2` in; a hole
  is kept on a panel, off every bend's strip, and `margin + d/2` or more
  from every boundary of the material and of the strips (seams between
  panels of one plane are inside the material). A sketch Outline is
  clipped to the flat faces it lies on and developed with those panels.
  The grid starts at the developed outline's lower left and the margin
  applies to its outer and inner loops. One trim and one solid
  take all holes; fields of thousands of holes are possible but each hole
  is part of the B-rep. CLI `sheet_perforate`: an L contour flange,
  staggered 33 holes (`9400 - 212.5 pi`), square 28 (`9400 - 150 pi`), the
  Flat Pattern (`4700 - 85 pi`), Unfold, a thickness edit, reopened.
  CLI `sheet_perforate_outline` checks the clipped field and the DXF holes.

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

- **Hem form.** Hem (None, Teardrop, Rope, Double; Flat, Open and Rolled
  since mitcad#2112) with Gap and a double
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

### Implementation notes (mitcad#1450)

- **Copies of sheet bodies** (`sheet/body_copy.rs`, called from the body
  branch of `features/pattern.rs::repeat`). A mirror's or pattern's copy of
  a valid sheet body (orthogonal transform) is a new
  sheet body whose state is the source's carried by the transform
  (`SheetState::transformed`): planar frames moved (a reflected frame's
  normal `x × y` points the other way, so the chart's
  `front_to_back_sign` is turned to keep the material's image), cylinders
  moved with their basis as given (the construction reads its handedness,
  `uv_sign` in `geometry/src/sheet_chart.cpp`; the development's stations
  are linear in it), ports and lineage unchanged (they are chart
  coordinates). The shape is the copy itself; its domains keep their
  identities (semantic selections are body plus domain, as after a split),
  and `pick::parse_face` reads `F6:inst1(...)` as the name inside.
- **Joining a copy.** With `combine` the copy's domains and curves are
  renamed as the pattern's (slots derived from the identity they replace,
  below; ports' tokens after the sheet's) and appended to the sheet; for
  each copy panel
  coplanar with a sheet panel (`planar_map`), the union of their regions
  decides: apart (as many regions), overlapping (less area than both:
  refused), or touching (fewer regions): the copy panel's regions are
  mapped into the sheet panel's chart, every reference to the copy panel
  (ports with their spans moved as the curves move, bends' boundaries,
  opened bends, lineage with the moved curves' generations) made to the
  sheet panel, and the panel united with them by `trim_with_lineage`, whose
  `rebind_ports` binds the ports to the united boundaries. The merged sheet
  is built again and compared with the generic join of the halves.
- **Fold ports on the moving panel.** A Fold's moving panel is a trimmed
  copy of the folded panel; the kernel may return a boundary it keeps
  running the other way (the union of a join returned such lines), so the
  ports the moving panel takes are bound again to its boundaries by
  position (`cut::rebind`) before the branch moves.
- **Tests.** CLI `sheet_mirror_body`, `sheet_mirror_body_join`,
  `sheet_mirror_reported` (see the
  [API section](../core/model-api/src/commands.md#mirrors-and-patterns-of-sheet-metal-bodies-mitcad1450));
  `pick_tests.rs` for the copies' face names.

### Implementation notes (mitcad#1462)

- **Stable slots of a joined copy.** A joined copy's domain and curve
  slots were numbered `100000 k + n` by their rank in sorted order, so an
  upstream edit that added or removed a domain renumbered the others and a
  later feature naming one of them pointed elsewhere. Each slot is now
  derived from the copy and the identity it replaces
  (`body_copy::slot`): the 64-bit FNV-1a hash of `<copy key>/<identity>`
  (`F3.b0/1/F5.p0`; the key `copy_key` is the source body, the inner
  patterns' `feature:index` steps of a pattern of patterns, and the
  element's index), its two halves xor-ed to 32 bits and placed at 100000
  or above (the pattern's own low slots, a united seam's curves, are never
  met). A slot taken already (by a copy joined before, or a clash) gives
  the next free one, in the sorted order of the copy's identities.
- **Change overlays on copies.** `SheetOverlayKind::Change` holds an
  optional `source`, the body the fillet or chamfer changed, when it is not
  the sheet's own; `body_copy::image` sets it, so `cut::with_overlays`
  reads the change from the body it was made on and moves it by the
  overlay's placement (the copy's transform after any before). A sheet
  with such an overlay is copied as a new body and joined like any other.
- **Patterns of patterns of bodies** (`repeat_body_places` and its
  `combine_any` twin). The copy's sheet is the sheet the body had before
  the inner pattern (`body_copy::source_before`: the `Set` state of the
  last result before it that set the body, carried by the rigid moves
  (`SheetChange::Transform`) since; none after ordinary
  modelling), carried by the element's transform after the place's steps.
  With `combine` a copy that touches its body is joined to the body's
  current sheet; one that does not is a sheet of its own.
- **`combine_any`** (`join_any_sheets`). `join_any` also names the bodies
  a copy reached. A copy of a sheet that touches one sheet, whole, is
  joined to it as with `combine` (any sheet of the pattern: the original,
  a copy that stayed a body of its own); the sheets of the bodies the
  copies may join are kept as the joins leave them.
- **Warnings instead of silent invalid sheets.** A scaled copy (scaling
  changes the thickness) stays an ordinary solid, `the scaled copies of
  sheet F3.b0 are not sheet metal bodies`; a join that makes a sheet an
  ordinary solid (a scaled copy joined to it, one copy joining several
  bodies at once, a copy partly apart, a copy that is not a sheet) warns
  `F3.b0 is no longer a sheet metal body: <why>`. These are collected and
  settled when the joined bodies are final (`body_copy::settle`): a body
  the adapter re-derives (`rederive::rederive_joined`, what `adapt` would
  do; flat material joined in a panel's plane) is a sheet and no warning
  is given; the others are warned about and invalidated as before
  (`complete_changes`, the adapter).
- **Tests.** CLI `sheet_copy_stable_slots`, `sheet_copy_overlay`,
  `sheet_copy_scaled`, `sheet_copy_patterns`, `sheet_copy_join_any` (see
  the [API section](../core/model-api/src/commands.md#mirrors-and-patterns-of-sheet-metal-bodies-mitcad1450));
  the slots and the overlay's `source` in `body_copy.rs`'s unit tests.

### Implementation notes (mitcad#1558)

- **Bends across the mirror plane.** `body_copy::joined` united only
  coplanar panels, so a bend that crosses the mirror plane and its image
  stayed two bend domains whose end walls met in the plane, and the
  construction failed ("sheet chart produced an empty material solid").
  The merge now goes over every copy chart: `chart_map` gives the copy
  chart's map into a sheet chart, `planar_map` for panels (and for bends
  opened flat), `cylinder_map` for folded bends whose records agree
  (angle, radius, K, allowance): one axis within the modelling tolerance,
  the same radii, angle and material side, and the same arc (`u` kept) or
  that arc run back (`u -> BA - u`); `v` moves along the axis, reversed
  where the axes are opposite (a mirror across the axis). The union then
  decides as for panels. `redirect` takes any domain: a bend's seams are
  moved into the target bend's axial coordinate (each seam's axial map and
  interval by the affine map that takes its ends on the copy bend to the
  mapped `v`), the copy's bend record is dropped and the target's axial
  interval covers both, an open state goes with the bend.
- **Seams side by side.** The planar union keeps collinear edges split
  (`SimplifyResult(false, true)` in `sheet_trim.cpp`), so along a united
  tangent line the sheet's and the copy's stretches stay two boundary
  pieces, each with its own seam to the bend; the construction pairs each
  and joins them at their common vertex. Where a union does return them
  as one piece, its rebinding (one curve only) leaves the other seam on a
  piece that is gone: `reseat` binds such sides again by position.
- **Combine.** `features/combine.rs` gives a join of sheets to
  `body_copy::combined`: every body a valid sheet (its own component), each
  tool merged in order by `joined` (the generic union so far and the tool
  as the halves' join; a tool's own fillet or chamfer overlay reads its
  change from the tool). A refusal keeps the generic join and gives
  `body_copy::not_combined`: the adapter's re-derivation
  (`rederive_joined`), else an invalid sheet naming the reason, where
  before the adapter's "material added" said nothing of it.
- **Tests.** CLI `sheet_mirror_bend_join`, `sheet_mirror_bend_reported`
  (see the
  [API section](../core/model-api/src/commands.md#mirrors-and-patterns-of-sheet-metal-bodies-mitcad1450));
  the bend charts' maps in `body_copy.rs`'s unit tests.

### A flange's profile sketch (mitcad#1648)

A Flange may draw its wall with a sketch instead of the rectangle of its
length and width (GitHub mitcad/Mitcad#237): `profile` names a sketch
before the flange whose closed region on the bend line is the wall's
outline (commands.md, "A flange's profile sketch").

- **The sketch's frame.** The wall's chart has `x` along the wall from the
  bend's moving tangent line and `y` along the boundary from where the
  bend starts. The profile is read in a frame of its own, independent of
  the bend's span: origin on the moving tangent line at the middle of the
  boundary, `x` along the line against the canonical axial direction `e`,
  `y` along the wall (`flange_profile_frame`). Its normal is the chart's,
  so sketch `(x, y)` is chart `(y, L/2 - start - x)`, a quarter turn, for
  a boundary `L` long and a bend starting `start` along it
  (`ProfileWall::chart`). The 2D outline is what counts: a sketch whose
  plane no longer lies on the wall (an angle edited since) still gives the
  same wall, and the flange warns.
- **The wall.** The region with a straight side on the sketch's `x` axis
  and material above it; that side (one or more lines in a row) becomes
  the generated curve 53, the bend's moving port, as the rectangular
  wall's root, and the rest keeps the sketch's curve names. The side's
  ends give the bend's span and so the offsets (`start = L/2 - x1`, `end =
  L/2 + x0`) with their reliefs; the outline's reach past the boundary's
  ends is an overhang beside the bend (one bend and its wall). The ends of
  a whole boundary keep the profile's span: the outline's following of
  oblique panel sides (mitcad#1576) is left to the profile. A region across
  the line, none or several on it, a side in pieces or past the boundary's
  ends are refused in words.
- **Making it.** `sheet_flange_profile` reads the flange's definition on a
  copy of the document (its values may be expressions), computes the frame
  against the sheet at the marker, and adds a hidden `flange_wall`
  construction plane and a sketch on it with the rectangle of the current
  length and width, its corners on the bend line fixed (one undo step); for
  a definition with a profile it puts that profile's plane back onto the
  wall (a fixed plane becomes a `flange_wall` plane). The Flange panel's
  Edit Profile runs it and opens the sketch while the panel waits
  (app/COMMANDS.md, "Flange").
- **Following the wall** (mitcad#1776, GitHub mitcad/Mitcad#310). A
  `flange_wall` plane (`sheet/folded/profile_plane.rs`) is the wall's
  sketch frame of the first flange whose `profile` is a sketch on it,
  worked out at every recompute from the sheet before the plane and the
  flange's boundary, angle, radius and position; until a flange names such
  a sketch it is its stored frame. The plane comes before the flange, so it
  reads the flange's definition, not its result: the read
  `Read::WallFlange` (the flange's uid and definition fingerprint) makes
  the cache, the resume after appended features and the result store
  evaluate it again when the flange changes or comes to name the sketch.
  A design opened with a profile on a fixed plane of its own (no other
  feature refers to it, no links, not flipped) gets the `flange_wall`
  plane with the same frame (`DocState::settle_flange_profiles`). The
  warning that the sketch no longer lies on its wall stays for a sketch on
  any other plane.
- **Tests.** `sheet_profile_tests.rs` (the mock kernel, which trims no
  sheet domains: the sketch made on the wall with its rectangle, one undo
  step, a side slanted 10 degrees by an angle dimension to the bend line,
  the flange naming its profile, saved and reopened, a profile across the
  bend line refused without ids, offsets refused, the plane moved back for
  another angle and refused for a sketch on no fixed plane of its own; the
  plane following the wall after the Face is narrowed and the angle
  changed, within 1e-6 mm of a profile made anew there; an earlier
  design's fixed plane made a flange-wall plane when opened, or on Edit
  Profile when another sketch shares it);
  CLI `cli.sheet_flange_profile` (volumes and Flat Pattern areas of the
  rectangular and the tapered wall, the Face narrowed and the angle changed
  without a warning, the warning for a profile on the XY plane); `tools/ui-sheet-flange-profile-test.sh`
  and the ctest `app.ui-windows-sheet-flange-profile`.

### A flange on several edges (mitcad#1872)

One Flange may take several straight edges connected at their ends
(GitHub mitcad/Mitcad#337: flanges on adjoining edges of a vertical and
an inclined face, refused one at a time because their walls passed
through each other). `boundaries` follow `boundary`; edge `k` makes bend
and wall slot `k` with curves from `100 k` (commands.md, "A flange on
several edges"). `sheet/folded/flange_edges.rs`:

- **Corners.** Two edges meet where an end of each coincides, or where
  the two ends lie on the two tangent lines of one bend at the same axial
  place (the sides of two faces that a bend joins). The edges make one
  chain or loop; anything else is refused in words. The flange's own
  geometry for each edge (its port frame and the wall's plane from the
  construction at the edge's start) is worked out on the input sheet, so
  the corners do not depend on the order the edges are built in.
- **Miters as half-spaces.** A corner gives each of its two flanges a
  half-space (`Keep`) its material keeps to. Bisect: the plane through
  the point where the walls' mid-thickness root lines come closest,
  normal to `u_i - u_j` (the edges' directions away from the corner),
  `g / 2` off it on each side. A wall running past: the passing wall up
  to the other wall's far surface, the other wall `g` short of the
  passing wall's near surface; both bends keep the bisecting half-space,
  the cut-back one the near-surface one as well.
- **Existing flanges** (mitcad#1877, `existing_walls`). A free end that
  lies at an end of a bend's tangent line, on either side, meets what
  lies across the bend: a wall of another flange (the end where that
  flange's bend starts, or across a bend from a flange's wall) keeps the
  end `g` short of its near surface (the existing wall runs past); a
  flange on the panel across whose bend ends across from the end (its
  stationary tangent line's end, in the middle of the thickness, on the
  other tangent line of the bend across, at most its setback away on the
  side of its wall, where its trim cut the bend across back) keeps it
  `g` short of that flange's wall: of its near surface at an angle, and
  where the walls lie in one plane (or touch face to face) of the plane
  square to its edge through the farthest its wall's chart and its bend
  reach towards the end (a gap of 0 refused). Only the new flange keeps
  to these; the existing ones are never cut.
- **Normal cuts.** A sheet's side faces are normal to its surfaces (the
  chart's region is swept through the thickness), so a true bisecting
  cut of two walls at an angle, oblique through the thickness, is not a
  sheet. Each wall is cut on the straight line where the half-space's
  plane meets whichever of its two surfaces reaches further (a half-plane
  of its chart); each bend ends square where its whole cross-section
  (radii `r` to `r + t` over its angle, `bend_limit`) clears the plane.
  Walls in one plane (both flanges turned to the inside of the bend
  between their faces) are cut exactly on the bisecting line; walls at an
  angle meet, at a gap of 0, along one line (an open corner, mitcad#1409);
  the gap is exact where they come closest.
- **Face and wall.** The bend covers the edge between its limits; the
  face is cut back to the tangent line over the bend and, past a
  shortened end, on to the edge's end (curves `70 + 100 k` at the start,
  `80 + 100 k` at the end), through a bend there by the developed cut
  (`cut_back`, whose tool curves now take a slot). The wall is the
  rectangle of its length over the bend, run on past a mitred end (also
  past the edge's end, up to its length plus the bend's radius, two
  thicknesses and the gap) and clipped by its half-planes; its root over
  the bend is the port `53`, the root beyond the bend free (`55`, `56`),
  the miters `58` and `59`. A free end follows the face's side as a whole
  edge does (mitcad#1576). Unfold and the Flat Pattern develop the walls
  as charts, so the gaps stay.
- **Limits.** Profile, hem, corner joint, offsets and width belong to a
  single plain flange and are refused with several edges or a corner
  gap, as are At Edge (no material to cut back at a corner) and a length
  with no wall; a pattern copy joined to the sheet does not repeat a
  flange on several edges. Walls that still meet are refused by the
  construction naming the flanges, with the advice to widen the gap or
  change the angle, length or position.
- **Places** (mitcad#1877). Each end's rule keeps the places of its
  corner: the side faces the two edges were picked by (`wall_name`, as
  the input sheet has them), an existing flange's wall's front face, the
  corner's point. The refusals at a corner (in `corner_rules`,
  `edge_flange`, `wall_polygon`) carry them as a `FeatureError`'s places,
  in the input body (`flange_edges` puts them there); a refusal of the
  construction that names meeting walls (`panel F6 slot 1`, read by
  `walls_named` before `name_walls` names them) gets, through
  `publish_placed`, each wall's place (one of this flange's walls: its
  edge's side face; another's: its front face) and the places of the
  corner whose rule names them all. The panel paints them on hover.
- **Per-corner Miter in the panel** (mitcad#1877). The Flange panel's
  Corners rows each pick a corner (an edge or a vertex there); the
  `sheet_flange_corners` query (`edge_corners`: the corners of the edges
  by the same `corners` rule, with the middle of the two ends) maps the
  pick to the nearest corner's two edges, written as `corner_miters`. An
  edit's rows keep their two edges until a corner is picked.
- **Tests.** CLI `cli.sheet_flange_edges` (a 40 x 30 box, three edges,
  Inside and Outside at gaps 0 and 1, turned down, the earlier edge
  running past, one corner's own miter, Flat Pattern areas, refusals) and
  `cli.sheet_flange_edges_bend` (the sides of a face and of an inclined
  wall that a 45-degree bend joins, walls in one plane mitred on the
  bend's bisecting line at gaps 1 and 2, the edges in either order; gap 0
  and a wall running past refused); `cli.sheet_flange_edges_existing`
  (mitcad#1877: a flange on an inclined wall's side meeting a plain
  flange on the face's side across the bend, walls in one plane, the
  volume and the 1 mm gap measured, the existing wall uncut, gap 0
  refused, `sheet_flange_corners`); the mock kernel's
  `sheet_edges_tests.rs` (the definition saved and reopened, the gap a
  parameter, refusals before any geometry, the places of a corner
  refusal, the corners found); the UI
  `tools/ui-sheet-flange-edges-test.sh` and the ctest
  `app.ui-windows-sheet-flange-edges`.

### The Hem command (mitcad#2112)

A hem in one step: **Hem** (Sheet Metal, Create, after Flange) folds a
straight edge back on itself in a form and its sizes, and makes a
`sheet_flange` with `hem`, which Flange edits too.

- **Forms in the model.** `hem.form` adds `flat` (an optional gap, 0.1
  times the thickness by default), `open` (its gap) and `rolled` (its
  angle, past 180 degrees and short of a full turn, and an optional
  radius). Flat and open are one bend of 180 degrees whose inside radius is
  half the gap and a wall back over the panel to the length from the
  outer tangent plane; rolled is one bend of its angle and no wall
  (`hem_steps`, `folded_back`). They give the same bends and walls as the
  flanges the imports made of these hems before (a flange of 180 degrees
  of half the gap; a flange of the rolled angle with a length of 0), so
  the imported bodies do not change; the imports now write the forms
  (a rolled hem of 180 degrees or less stays the plain flange), and an
  edit shows the hem. The flange's own radius is not used by these forms.
- **Refusals.** A hem's refusal (these forms' and the teardrop's, rope's
  and double's) is a failure in words whose place is the wall of the
  hem's edge in the input sheet, painted on hover (`hem_refusal`): a gap
  of 0, a length short of the bend's outer radius, a rolled angle out of
  its range or a radius of 0, a gap the teardrop's or rope's radius cannot
  make, an edge that is not straight.
- **Command.** Hem's panel: Edge (a straight edge or side face of one
  sheet), Form (Flat, Open, Teardrop, Rolled, Rope, Double; Flat by
  default), Length (not for Rolled) with a handle on the view from the
  side plane through the edge (Tangent to Side), Own Gap and Gap (Flat:
  off, the default gap; Open, Teardrop, Rope, Double: the Gap), Own Bend
  Radius and Bend Radius (Rolled, Teardrop, Rope, Double; off: the rules'
  radius; Rolled writes it into `hem.radius`), Angle (Rolled, 270 degrees
  by default), Setback (Double), Flip (the side it folds to), Position (At
  Edge, Tangent to Side; Tangent to Side by default), Width as Flange's,
  and Own Bend Relief. The length is measured from the outer tangent
  (`outer_tangent`) and the angle is 180 degrees to the side of Flip.
  Hem is registered before Flange with `canEdit`: a flange with a hem on
  one edge, at the edge or tangent to the side, without profile or corner,
  opens in Hem (double-click, Edit Feature); Flange edits the others and
  offers the six forms in its Hem choice (Flat, Open and Rolled after the
  earlier ones; with Rolled its Angle is the bend's and its Length hidden).
  The sheet process profiles offer Hem for metal (`sheet.hem`,
  `process.rs`).
- **One edge.** A hem goes on one edge: a flange on several edges is the
  mitred plain walls of mitcad#1872, which take no hem.
- **Tests.** Model: `bends.rs`
  (`flat_open_and_rolled_hems_are_the_imports_flanges`: the steps equal
  the imports' flanges', the default gap, the refusals, the forms read
  back, the slots), `process.rs` (Hem metal's). OCCT: CLI
  `sheet_hem_forms` (the open, flat and rolled hems and the flanges the
  imports made of them give the same volumes, bends and flat areas on the
  35 x 20 panel; an edit through all six forms; refusals). Import:
  `import-sheet` `an_open_hem_is_the_flanges_open_hem` and
  `hems_past_half_a_turn_are_flanges_with_their_form`. UI:
  `tools/ui-sheet-hem-test.sh` with the ctests `app.ui-windows-sheet-hem`
  and `app.ui-macos-sheet-hem` (all `tools/ui-scripts/sheet-hem.mitcad-ui`:
  a Flat hem made on a side face, changed to Open, Rolled and Double by
  Edit Feature, the volumes, Unfold's flat volume and Refold), and
  `tools/ui-sheet-flange-hem-test.sh` (Flange's Hem makes a teardrop,
  which then opens in Hem).

### Implementation notes (mitcad#2435)

Convert to Sheet Metal of cylindrical and conical faces, and their Flat
Patterns (`sheet/folded/curved.rs`):

- **Curved panels.** A picked face stored as a cylinder or a cone becomes
  panel slot 0 of a sheet of its own. A cylinder's chart is the existing
  `SheetEmbedding::Cylinder` on a panel domain (`u` the angle times the
  neutral radius, `v` along the axis); a cone's is the new
  `SheetEmbedding::Cone` (`ConeEmbedding`): the development of the neutral
  cone about its apex, chart `(x, y)` at `hypot(x, y)` from the apex along
  the generator and the developed angle `atan2(y, x)`, which is that angle
  over `sin(half_angle)` about the axis; `neutral_depth` (K t) from the
  inner surface. The chart is the cutting blank: its lengths are the
  neutral surface's. The neutral surface follows the rules' K-factor (0.5
  for kerf-bend rules); measured fold allowances, deductions and crease
  tables are refused for a rolled face, as they do not describe rolling.
- **Reading the faces.** Every face of the body on the picked face's
  cylinder or cone belongs to the panel: the reported neck, extruded from a
  loft's round end, is a cylinder split into four faces by the loft's side
  edges, and stored as splines. `Kernel::sheet_face_development`
  (`geometry/src/sheet_develop.cpp`) takes the faces on one surface as
  `face_geometry` recognises it (stored so, or as a spline, extrusion or
  revolution within 1e-6 mm of it) and develops their edges from their 3D
  curves: the angle round the axis runs on continuously along each wire (a
  seam edge is met at both ends of the turn), the faces are turned by whole
  turns to meet at their shared edges, and where they meet again round the
  axis they stay apart (that is the seam of a closed panel). A generator
  (one angle all along) is a line and a circle round the axis (one height on
  a cylinder, one distance from the apex on a cone) a line or an arc about
  the apex, from their exact ends; other edges are cubic B-splines through
  their exact development within 1e-8 mm (as an emboss wraps onto a cone,
  mitcad#473), one for each span where the edge's curve is twice
  continuous (a spline's knots of lower continuity change its speed, which
  no cubic through evenly spaced points follows). Faces stored as splines
  are each recognised on their own (the body's faces are read with
  `Kernel::recognised_face_geometries`), so pieces of one surface are gathered
  within 1e-4 mm of the picked face's axis and radius and checked by their
  points within 1e-5 mm of its surface. The faces' developments are united where they share edges.
  The chart starts at the least angle; the material side is read from the
  picked face's normal. Faces running to a cone's apex, not joined, or on
  several surfaces are refused.
- **Construction.** `geometry/src/sheet_chart.cpp` builds a cone panel's
  caps on two cones moved along their normals (the same parameters at the
  same generator point), its parameter curves and both rails cubic
  B-splines through the exact images at the same parameters (so the walls
  pair the rails pole by pole), refined until within a quarter of the
  tolerance, each boundary read at its own parameter (a line's length, an
  arc's angle; never a rational spline's, whose speed changes at its knots); the chart's `(x, y)` turns against the caps' `(angle,
  distance)`, so its loops are reversed there. Ports never join a cone
  panel.
- **Closed faces and seams.** A face (or the faces) once round its axis is
  opened at its seam edge (or where the faces meet again): a strip `corner_gap` wide (default the thickness, above 0),
  half each side of the seam, of constant width along the seam's generator
  (parallel lines in the development), is left out, and a cylinder's chart
  is rebased to start at 0 (its angle must stay under a turn).
- **Plate check.** The sheet of the whole face (a closed face in two halves,
  each less than a turn) must have the body's volume and centre of mass.
  Where it has not, the body is still a plate when every other face lies on
  the surface the thickness behind the face or is a wall meeting both: then
  walls not normal to the material (a hole drilled along one direction)
  are cut normal to it, as the blank cuts them, and the feature warns by
  how much the sheet differs from the body. Anything else is refused as no
  plate (a solid, or a body of several surfaces such as a whole
  transition: shell it and split the developable part off first).
- **Faces that cannot lie flat.** A face that is no plane, cylinder or cone
  is refused naming it. `Kernel::face_bending` gives the largest Gaussian
  curvature over a grid of points inside the face: above `1e-6 / size^2` the
  face bends two ways and the refusal says the material would have to
  stretch (with the curvature as a radius) and to form the part another way
  or design it of developable pieces; below, the face is a spline that may
  lie flat but is no surface the sheet develops yet. The reported
  transition (a loft from a 250 x 80 rectangle to a circle of 75, 100 high,
  a neck of 30 on it) has twisted spline sides bending two ways (Gaussian
  curvature up to 1/(66.7 mm)^2 on the broad sides, 1/(231.9 mm)^2 on the
  narrow ones): each is refused naming it. Its neck is the part that
  develops exactly: shelled 2 inside and split off above the loft, its four
  faces convert into one blank.
- **Flat Pattern.** `curved::developed` lays the curved panel's chart in the
  plane touching its front at the chart's first boundary point (the chart
  is the development already, so the outline, area and DXF come from it
  unchanged). A curved panel among other walls is not developed.
- **Not yet** (follow-ups, beyond the ticket's Wanted): bends, rips and
  flanges joining a curved panel, panels on several surfaces in one sheet
  (planes and cones of a transition joined), ordinary features on a curved
  sheet (its sheet meaning is lost with a reason; make holes before
  converting), choosing where a closed panel's seam lies, Unfold.
- **Tests.** Model: `curved_tests.rs` (a cone's chart and its inverse,
  rebasing, the back surface, the tangent plane). OCCT: `geometry.sheet_curved`
  (conical panels of K 0.5 and 0.3 against `t (A + d cot(a) phi (rho2 -
  rho1))`, a cylindrical panel, a cone's and a cylinder's face developed,
  curvatures), CLI `sheet_convert_curved` (a tube and a frustum, open and
  closed, with K 0.5 and 0.3: folded volumes and blank areas against the
  analytic ones, DXFs; a hole developed; a sphere and a solid cylinder
  refused), CLI `sheet_convert_curved_reported` (the reported transition,
  skipped without it: the loft's four sides refused with their curvature, the
  neck on the solid refused as no plate, then shelled and split: its four
  spline faces one sheet, folded 2920 pi - 80, the blank 1460 pi - 40, a
  DXF).

### Mitred elbows and eccentric cones (mitcad#2491)

Duct work: the gores of a mitred elbow and eccentric (oblique) cones and
frusta, added to mitcad#2435's cylinders and cones.

- **Oblique cuts at the neutral surface.** An edge of the faces that is no
  generator and no circle round the axis, whose face across it is a plane
  meeting the generators (the sine of its angle to them at least 1e-3), is
  developed where that plane meets the neutral surface: each point moved
  along its generator to the plane (`on_wall` in
  `geometry/src/sheet_develop.cpp`), and a generator that ends at such an
  edge's vertex ends there too. A gore of neutral radius `r` cut at the
  angle `a` then keeps the known stretch-out: the cut `r tan(a) cos(s / r)`
  off a straight line at the developed length `s` from the heel, not the
  front's radius (mitcad#2435 projected every edge of the front along the
  normals; circles and generators, and edges along other walls such as a
  drilled hole's, still are, so mitcad#2435's blanks are unchanged). Each
  gore converts as a body of its own: an elbow modelled as one body is
  split into its gores at its mitre joints (mitcad#2559, the next
  section). The gores' cut walls are not normal to the
  material: the sheet cuts them normal to it, with mitcad#2435's warning.
- **Eccentric cones.** A face that is no plane, cylinder or cone (a ruled
  spline, a loft between two circles) is tried as an oblique cone
  (`oblique_of`): the straight lines along one of its parameters meet at
  one point (least squares, each within 1e-5 mm of it), their directions
  from it lie on a quadric cone (the least eigenvector of their moments),
  and that cone's circular section (a cyclic plane, the one most square to
  the generators, halfway along the face) is its circle; a grid of the
  face's points lies within 1e-5 mm of the cone so found. Twisted ruled
  surfaces and lofts that bend two ways are refused as before, naming the
  face.
- **The oblique chart.** `SheetEmbedding::ObliqueCone`
  (`ObliqueConeEmbedding` in `core/model-base/src/sheet_oblique_cone.rs`,
  `geometry::ObliqueConeEmbedding` and `detail::ObliqueDevelopment` in
  `geometry/src/sheet_oblique_cone.{hpp,cpp}`): the front cone (its apex,
  its circle, the angle round the circle from `radial_start`) and the
  neutral depth. The neutral surface, the cone moved `d` along its normals
  `n`, is developable but no cone: `N(theta, rho) = apex + d n(theta) + rho
  g(theta)`, `g` the unit generator. It develops as `chart(theta, rho) =
  base(theta) + rho (cos phi, sin phi)` with `phi' = |g'|` and `base' = -d
  (n . g'') / |g'| (-sin phi, cos phi)`, `base(0) = d c(0) (1, 0)`, `c = -(n
  . g'') / |g'|^2` (on a cone of revolution `base` is `d cot(half angle)`
  along the generator, the neutral apex, and the chart is mitcad#2435's).
  Model and kernel integrate it the same way: the classical Runge-Kutta
  rule from angle 0 at steps of a turn over 4096 (the kernel keeps the
  nodes, the model integrates when it reads the chart). A chart point is
  read back by its generator: the angle where the point lies on the
  generator's developed line (Newton's steps in a bracket from the
  developed angle). The frame of the generator at an angle (`frame`: the
  isometry from the generator along x from `base - d c` to the chart) is
  the identity at 0 and a turn about the origin on a cone of revolution;
  rebasing the chart to start at another angle maps it by that frame's
  inverse.
- **Construction.** The caps are B-spline surfaces in (angle, distance
  along the generator), of degree 3 through the exact surface points at
  evenly spaced angles (more until within a sixteenth of the tolerance)
  and of degree 1 along the generators; the boundaries are fitted on them
  as on a cone (`cone_rails`). The chart turns against (angle, distance);
  the caps' own normals are read against the cone's outward one.
- **Plate check, seam and Flat Pattern.** The kernel reports whether the
  body is a plate behind the face (`SheetFaceDevelopment::plate`): every
  other face at the thickness behind it along the cone's normals (its
  points' feet on the generators found by Newton's steps, within 1e-4 mm)
  or a wall meeting the face and such a back. A closed face is checked in
  two halves cut along the generator halfway round its developed angle
  and opened at its seam by the same strips as a cone's, in the frames of
  the seam's generator at both ends of the turn. The Flat Pattern lays the
  chart in the plane touching the front at its first boundary point, along
  and across the generator there.
- **Tests.** OCCT: `geometry.sheet_mitred_oblique` (the three gores of a
  90-degree elbow of centre-line radius 60, cut at 22.5 degrees, one of
  them turned into its place: both seams' lengths, the stretch-out at 63
  generators, the heel and the throat within 1e-6 mm; an eccentric frustum
  from a circle of 60 to one of 30 offset 20: apex, seam lengths, both
  edges' developed lengths against the neutral surface's curves in the end
  planes, the developed area; a twisted ruled surface refused). Model:
  `sheet_oblique_cone.rs` (a cone of revolution develops as one, an
  eccentric cone keeps lengths, chart points at a depth and back, rebasing).
  CLI `sheet_convert_mitred` (the three gores as bodies: folded volumes and
  blanks `Lm (39 pi - 1) - 4 19.5^2 tan(a) sin(1 / 39)` and the end gores'
  half of it; the eccentric frustum shelled 1, its half and whole
  converted against the integral of the neutral surface's area; a loft
  from a square to a circle refused).

### One-body elbows and cones of several faces (mitcad#2559)

Follow-up of mitcad#2491: an elbow modelled as one body and an eccentric
cone whose side the kernel stores as several faces convert without a
manual split (`sheet/folded/curved.rs`).

- **Mitre joints.** Before the panel is developed, the picked cylinder's
  or cone's edges to other cylinders or cones whose axes are at an angle
  (the sine at least 1e-3, `MITRE_SINE`) are read as polylines; an edge
  whose points lie within 1e-4 mm of their least-squares plane and that is
  no straight line is a mitre joint (`mitred_gores`, `plane_of`). The
  faces reached from the picked face across joint after joint are the
  gores' fronts (the outer or the inner faces, whichever the picked one
  is); every joint's plane, each once, splits the body (`Kernel::split_body`
  with the plane, the cut faces `<feature>:split`), a plane leaving a
  piece whole where it does not divide it. Each piece must hold exactly
  one front; otherwise (a joint's plane crossing another segment, as in an
  elbow turning through more than half a turn) the feature is refused in
  words. A body whose picked face meets no mitre joint converts as before.
- **The gores' sheets.** Each piece converts exactly as a gore body does
  (mitcad#2491: its front's cylinder or cone developed at the neutral
  surface, its cut ends where their planes meet it, the plate check, the
  seam opened by the corner gap). The picked face's gore keeps the body;
  every other gore is published as a body of the feature
  (`BodyUid::new(feature, k)`, in the order of their fronts' names) owned
  by the body's component, its panel slot `k + 1`. Each is a sheet of one
  curved panel with its own Flat Pattern; the joints are the seams between
  the blanks.
- **Cones of several faces.** On an oblique cone the panel's faces grow
  from the picked face across shared edges (`oblique_fronts`): each
  neighbour that is no plane, cylinder or cone joins when the kernel
  develops it with the faces found so far as one oblique cone (its points
  on the picked face's generators within 1e-5 mm). The kernel already
  develops several faces of one cone together (mitcad#2491's tracks,
  turned to meet at their shared edges); their developments are united as
  a split cylinder's are, and a closed set is opened where the faces meet
  again.
- **Tests.** OCCT: `geometry.sheet_mitred_oblique` adds the eccentric
  frustum whose side is two faces (its sections two arcs each, meeting at
  a quarter turn either side): developed as one panel, its four generator
  lines 70 long, both edges as long as the neutral surface's curves in
  the end planes and the area the one face's, within 1e-5 mm and 1e-2
  mm². Model: `curved_tests.rs` (a joint's ellipse spans its plane, from
  all or half of it; a straight edge or a saddle curve spans none). CLI
  `sheet_convert_mitred`: the 90-degree elbow of three gores joined into
  one body converts from its middle gore's face into three sheets with
  the gore bodies' volumes and blanks (the middle `Lm (39 pi - 1) - 4
  19.5^2 tan(a) sin(1 / 39)`, the ends half of it), each with its Flat
  Pattern; the frustum of two faces, half (the one face's volume and
  blank) and whole (opened along a generator 70 long).

### A lip on the top edge of a wall (mitcad#2445)

A Flange on the top edge of a tray's wall (a return flange, the lip of a
C channel) folded in over the tray passes into the end walls' tops where
its wall runs the whole edge: the lip's Bend Line setback leaves its level
wall 0.13 mm below the walls' tops for t = r = 1. It was refused as two
walls passing through each other.

- **Walls beside an end** (`flange_edges::walls_beside`). An end of a
  flange's edge that meets no existing flange across a bend is checked
  against every other flat panel of the sheet that stands across the
  edge's line: the flange's cross-section at the end (points across its
  bend and along its wall, on both surfaces and in the middle), carried
  along the edge, meets that panel's near surface inside its outline (by
  more than 0.001 mm) before it has gone past the end, or lies past that
  surface by no more than `r + 2t + gap`. The flange then keeps the
  corner gap short of that surface with the keep of an existing flange's
  wall (`short_of`). Panels the flange stays clear of, by however little,
  give none: such sheets build as before.
- **One edge.** `bends::flange` hands a plain flange on one edge to the
  route of a flange with a corner gap (`flange_edges`) when an end meets
  such a panel (`meets_wall_beside`); an open corner joint is allowed
  there, as it only sets the Inner Length. Its refusals of walls that meet
  say what to change (Flip, the Length, the Width; for a flange on
  several edges, the Corner Gap). Every other flange builds as before.
- **The pair named.** The kernel's search for walls that meet
  (`meeting_walls`) runs from the latest region back, so a refusal names
  a new wall and the wall it meets.
- Test: `cli.sheet_flange_lip` (a 200 x 100 tray: a lip folded in on the
  long wall, its ends 1 mm short of the end walls; out; on a short wall
  both ways; the flat patterns; a lip that crosses the tray refused naming
  itself and the wall it hits); the model test
  `a_lip_keeps_the_gap_short_of_a_wall_beside_its_end`.

### Implementation notes (mitcad#2442)

Convert to Sheet Metal with `development: triangulated`: faces that are
not exactly developable laid flat approximately, as duct work lays out a
transition (`sheet/folded/triangulated.rs`):

- **An explicit choice.** Only with `development: triangulated`. One flat,
  cylindrical or conical face still goes through the exact development
  (mitcad#2435 and the planar convert), with an information notice that
  the approximation was not needed. Further faces (`faces`) are laid flat
  with the picked one only by triangulation; bend and rip edges are
  refused with it.
- **The strip.** `Kernel::sheet_face_grid` (`geometry/src/sheet_face_grid.cpp`)
  samples each face over its surface's parameter bounds: points, unit
  normals out of the body (a little way in where the surface has none, at
  an apex) and the face's own area; each piece of a face the kernel split
  (a loft from a rectangle to a circle splits a side where the circle has
  a vertex) is a face of the strip of its own. The faces form one strip between two
  edges (the *rails*, a loft's sections), cut by *rungs* from rail to
  rail: across the edges the faces share (a rung of one face within 0.01 mm
  of a rung of the next, in any order and either way round), else the
  face's parameter lines that run round it (a face closed on itself), else
  its straighter parameter lines (a loft's or a ruled face's straight
  lines). A face that does not continue the strip is named. A strip that
  closes on itself is opened at the picked face's first rung (or the first
  face's that the strip runs through), the corner gap (default the
  thickness, above 0) wide, half each side along the rails. A face cut out
  of its surface, or with holes, is laid out by its boundary since
  mitcad#2558 (below).
- **The triangles** (`triangles`, default 64, 2 to 4000) are shared among
  the faces by their mean rail lengths, at least two each; a face's rungs
  lie every 8th line of a grid sampled 8 times finer along the strip and
  16 steps across it. Each quadrilateral between two rungs is split along
  its shorter diagonal, and the triangles are laid flat in order, each
  keeping its three sides' lengths (the circle intersection of the two
  known corners), turned as the faces' front shows it. A rung two faces
  share is the mean of their points (since mitcad#2558 with the difference
  shared out along each face). A rung of no length (the rails meet) is
  laid at the strip's start or end (mitcad#2558) and between its ends
  (mitcad#2574); a pattern that crosses itself is refused.
- **The neutral surface** is the rules' K-factor times the thickness from
  the faces' inner side, the side whose surface (the front, or the surface
  the thickness behind it) has the smaller area; K 0.5 for kerf-bend rules,
  measured allowances refused, as for curved panels. The body must be a
  plate behind the faces: its volume within 3 % of the thickness times the
  faces' area at the middle of the material ("pick every face of its front,
  or split the body first").
- **The deviation** (`SheetApproximation`, stored in the sheet state) is
  measured, at the neutral surface, against the faces sampled 8 times
  finer along the strip and 16 steps across it, each length corrected for
  its chords' error (Richardson's extrapolation, the error falling with the
  square of the step): `edges` is the largest difference of the pattern's
  rails and end rungs (the seam's before it is opened) from the faces'
  lengths; `across` the largest of the lines of the faces between the rails
  (the pattern's points on each rung at the share of the face's line across
  it), the rungs between (straight in the pattern, curved on a face that
  bulges), and the diagonals not laid (the pattern's against the faces'
  chords: the twist a warped face needs). The convert says them in an
  information notice; the Flat Pattern gives them as `approximation`
  (`method`, `triangles`, `edges`, `across`, `seam`, `folds`), the
  `sheet_bodies` query without the folds, and the Browser's Flat Pattern
  row in its tooltip.
- **The sheet.** Its one panel's chart (slot 0) is the pattern, a polygon of
  generated curves `g<2000000+>`, on the plane touching the strip's first
  point, the first rung along the chart's y axis, seen from the front. The
  folded body is the body as it was: the kernel builds no folded sheet from
  the pattern. Sheet features refuse such a sheet (`folded::read`), ordinary
  features take its sheet meaning away with the reason, and the Flat
  Pattern's points have no exact place on the body (`flat_point` refuses).
- **Fold lines.** Where two triangles meet at 0.01° or more, with the
  angle (positive: folded towards the front). The DXF export writes them
  as bend lines on `BEND_UP` and `BEND_DOWN` with `folds: true` (the
  application asks when it exports such a pattern); a closed strip's seam
  is the outline's two end edges.
- **Since.** Holes, faces cut out of their surfaces and rails meeting
  where the strip starts or ends: mitcad#2558; the plane triangles of a
  square-to-round built of planes and cones in one strip with its cones,
  and rails that meet between the strip's ends: mitcad#2574 (both below).
- **Tests.** Model: `triangulated_tests.rs` (a cylinder's face laid flat as
  its rectangle of chords with its folds, a square-to-round face and a
  rectangle-to-round transition with an offset whose stated deviation is
  the measured one, faces given in any order and way round, the seam
  opened, faces that do not join the strip, more triangles closer). OCCT:
  `geometry.sheet_face_grid`. CLI `sheet_convert_triangulated` (both
  transitions shelled and converted, their Flat Patterns approximate, a
  DXF with folds, refusals, sheet features refused, a tube asked to be
  triangulated developed exactly). UI: the Linux UI test
  `sheet-convert-triangulated` and its Windows and macOS twins.

### Implementation notes (mitcad#2558)

Holes, faces cut out of their surfaces and fans in the triangulated
development (`sheet/folded/triangulated.rs`):

- **Boundaries.** `Kernel::sheet_face_grid` takes `edge_steps`: above 0,
  each piece's wires come with it (`SheetFaceWire`: the outer wire and the
  holes, `BRepTools_WireExplorer` order, edges of no length left out), each
  edge at `edge_steps` (64) equal steps of its parameter on the face, its
  points' places in the grid, the points and the normals. A face whose
  outer wire runs along its grid's four bounding lines (within 10⁻⁵ of the
  grid's size) fills it, holes or not; an edge off them makes the face *cut
  out of its surface* (the area test of mitcad#2442 is kept only for a
  kernel that gives no boundary). The plate test takes each face's own area
  (less its holes) grown or shrunk as its grid's is.
- **Laying out.** A boundary point at the neutral surface is laid out with
  its strip's quadrilateral (by its place along the grid), through the
  triangle in which its barycentric coordinates on the triangle's plane are
  least negative; a point on a rail at a rung is that rung's end. Holes are
  holes of the pattern's region (generated curves after the outline's). The
  outline of a strip with a face cut out of its surface is joined end to
  end (within 0.1 mm) from the other faces' rails and end rungs and that
  face's edges: an edge along a rail through the rungs' ends it passes, one
  along the strip's first or last rung straight, a rung shared with the
  face beside it left out, any other edge as its laid-out points. A hole
  that crosses the outline or another hole, and an outline that does not
  join, are refused. Fold lines are clipped to the material.
- **Measured.** Each hole's edge and each edge of a face cut out of its
  surface: the pattern's polyline as drawn against the edge's length at the
  neutral surface (Richardson's correction from every second point), in
  `edges`; such a face's rails and end rungs (no edges of it) go to
  `across`. The Flat Pattern's `approximation` gives `holes`.
- **Fans.** A rung whose ends lie within 0.01 mm at the neutral surface is
  one point (the rails meet). At the strip's start the first quadrilateral
  is one triangle laid from the line to the next rung's second-rail end,
  which is the pattern's y axis (`Layout::axis` 3; the chart's frame
  follows); at the end it lays as before. Rails meeting between the ends
  were refused (laid since mitcad#2574, below). A fan from a point (a rail
  of no length) lays as before.
- **The deviation against the count.** At a rung two faces share at an
  angle `a`, their points `d` behind it lie `2 d sin(a/2)` apart (1.4 mm at
  a right angle 1 mm behind). The strip ran through their mean, and the
  measure moved each face's line there within its last fine step: the
  pattern took the difference over the face's last strip, the measure over
  an eighth of it, so their difference grew as the strips got shorter (64
  triangles 0.207 mm, 98 triangles 0.360 mm on the CLI fixture's
  transition). It did not come from how the triangles are shared out
  (each face's share never falls as the count grows). Now each face's points
  at the neutral surface are moved towards the mean evenly along the face
  (`Neutral`), the stations and the lines measured alike, so the pattern
  near such a rung does not depend on the count and the edges' deviation
  falls with it. The pattern still gives such a rung no bend allowance of its own
  (a sharp corner of the faces is laid as the faces' neutral lengths meeting
  at the mean).
- **The deviation across does not fall (mitcad#2585).** `across` is the
  largest of the inner rungs', the diagonals not laid and the lines
  between the rails. The faces of the offset transition are not
  developable (straight lines from a rectangle's side to a quarter circle
  at the same share of each), so a pattern that keeps the rails and the
  rungs stretches the lines between the rails: the middle line of a long
  side about 1.38 mm. Fewer, wider triangles cut across the twist and the
  pattern's line through few rungs is short, which hides part of that
  stretch; with more triangles `across` climbs towards it and settles
  (1.129 mm at 24 triangles, 1.346 at 48, 1.364 at 64, 1.380 at 98, 1.384
  at 128; the diagonals' part falls, 0.857 to 0.048 mm). It is the faces'
  own departure from developable, not an effect of how the triangles are
  shared out, and no count can make it fall.
- **Tests.** Model: `triangulated_tests.rs`
  (`more_triangles_do_not_give_a_worse_pattern`: the offset transition 1 mm
  behind its faces at 24, 48, 64, 98 and 128 triangles, the edges'
  deviation not rising by more than 0.002 mm from one count to the next and
  the one across settling, no rise larger than the one before and no count
  above the finest; a round hole and
  a face cut out of a cylinder, their laid lengths within the stated
  deviation and the folds only on the material; a fan whose rails meet
  where it starts and one from a point, the stated deviation the arcs'
  chords' error). CLI `sheet_convert_triangulated`: the offset transition
  with a 16 mm hole through both long sides (two holes, `edges` under
  0.5 mm, a DXF). A fan thickened from a surface loft between an arc and a
  line from one point is not in it (mitcad#2585): Thicken fails on that
  surface to either side and half way (the kernel's offset of the face
  whose two edges start at one point), so such a body cannot be made for
  the fixture yet; the fans are tested in the model.

### Implementation notes (mitcad#2574)

Rails meeting between the strip's ends, planar faces in a strip, and the
outline of a strip that mixes a face cut out of its surface with others
(`sheet/folded/triangulated.rs`):

- **Rails meeting midway.** A station between the strip's ends whose rung
  has no length (its ends within 1e-9 mm at the neutral surface, where
  `Neutral` has already made the rails one point within 0.01 mm) splits
  the strip into two fans (`Layout::pinches`). The quadrilateral after it
  is one triangle, laid as the first of a fan from a point: the next
  rung's second-rail end at its distance from the point, then its
  first-rail end by the triangle's sides, both turned about the point so
  that the middle of the new fan's corner there points against the middle
  of the corner of the fan before. Each corner is under 180 degrees, so
  the two fans meet at the point only. The outline is the strip's as
  before, through the point twice, its two lobes running round the same
  way (the region's area is their sum); the self-crossing check lets the
  sides that end at such a point touch there (`crosses_itself_but`). The
  two fans share no triangle edge, so there is no fold between them.
  Both fans' rails, rungs and lines across are measured as any face's.
  Where two faces share such a rung, every way across the next face
  matches it, so `chain` takes the one that runs round as the face before
  does (the sign of (along x across) . normal one line in from the rung):
  the rails stay on their sides, and the outline touches itself at the
  point rather than crossing there.
  A rung of no length followed by another is refused ("the strip's edges
  meet along a stretch of it"); a degenerate diagonal (the rails crossing
  between two rungs) now says "the strip's edges cross between two of its
  lines across: use more triangles". A meeting point inside a face is a
  station only where the face's grid has a rung through it (the kernel
  samples a face at equal steps of its parameters), so the typical case
  is two faces meeting at a point, as a loft whose sections touch there
  makes them.
- **Planar faces.** `plane_rungs` takes the faces' first look sampled with
  their boundaries (`edge_steps` 64 now in the first look too): a face is
  planar when its normals agree within 1e-9 and its points lie within
  1e-6 mm of the plane through its first point. A straight edge of its
  outer wire (every sampled point within 0.01 mm of the chord) is a rung
  when another face of the strip has a line with the same ends (either
  way round, within 0.01 mm): a planar face's straight edge, any other
  face's grid's four bounding lines. Two such edges are the first and the
  last rung; one alone is the first, the last a triangle's opposite corner
  (the rails meet there) or the edge across a face of more edges.
  `plane_grid` builds the grid between them: the first rail the wire's
  edges from the first rung's start to the last rung (in reverse), the
  second those from the first rung's end, each resampled at equal steps of
  its length (a rail of no edges is its corner, as a triangle's apex),
  the lines across straight, the normals the plane's, the area the face's.
  Its outer wire's points are placed exactly on the grid's bounding lines
  (the rails at their share of the rail's length, the rungs at their share
  of the rung's), so the face fills its grid and is not cut out; a hole's
  points are placed through the triangle of a cell they lie in. The size
  is the kernel's for the face's orientation (columns and rows swapped
  where the strip runs along its rows). A face without a closed outer wire
  of three edges or more, or without a shared straight edge, keeps the
  kernel's grid as before.
- **The mixed strip.** A strip of an ordinary face and a face cut out of
  its surface joins its outline from the ordinary face's rails and end
  rungs and the cut face's edges, the rung they share left out; a model
  test now covers it.
- **Tests.** Model: `triangulated_tests.rs`
  (`rails_meeting_between_the_ends_split_the_strip_into_two_fans`: two
  twisted fans meeting at the origin, the split station, the second fan's
  corner opening away from the first's, each rail against the faces'
  length within the stated deviation, the outline through the point twice
  and the lobes' areas adding up;
  `a_square_to_round_of_planes_and_cones_lies_flat_in_one_strip`: four
  plane triangles as the kernel gives a plane (a rectangle of axes turned
  0.4 rad holding the triangle) and four oblique corner cones, which do
  not chain as sampled; their grids from the edges chain round, planes and
  cones in turn, the sides laid at 2 A, the quarters as chords, the stated
  deviation the measured one;
  `a_strip_of_a_cut_out_face_and_an_ordinary_face_has_one_outline`). CLI
  `sheet_convert_triangulated`: a rectangle with one rounded corner
  extruded with a 10 degree taper and shelled (four trapezoid planes and a
  cone in one strip, `edges` under 0.5 mm, a DXF), and two fans thickened
  from a surface loft between two arcs and two lines that meet at the
  origin (`edges` under 0.5 mm, a DXF).

### Lofted Flange (mitcad#2512)

A transition made directly as sheet between two sketch profiles, for
ducts, hoods and hoppers (`sheet_lofted_flange`,
[its API](../core/model-api/src/commands.md#lofted-flange-mitcad2512);
`features/sheet_lofted.rs`, `sheet/folded/lofted.rs` and
`lofted_geometry.rs`).

- **Profiles.** A sketch's region (its outer boundary) or curves of a
  sketch joined end to end, both closed or both open, on sketch planes. The
  second is turned to run the way the first does (round the same way, or
  from the end nearer the first's start). The profiles lie on the sheet's
  front; the thickness lies behind it, outwards of a closed transition,
  along the surface's normals for an open one (Flip: the other side).
- **Regions (strips).** The transition is a sequence of regions from the
  first profile to the second, each between a piece of the first (a point,
  a segment or an arc) and a piece of the second; its lines at the same
  fraction of both pieces are straight lines of the surface (rulings). A
  region of points and segments is flat (a triangle or a trapezoid), a
  point against an arc a cone, two arcs a cone between circles in parallel
  planes (a cylinder for equal radii). Every region develops without
  stretching.
- **Matched profiles.** Profiles in parallel planes that turn one way
  (convex: lines, arcs and corners all turning left) are joined where they
  run in the same direction: a line against a line is a trapezoid (their
  segments are parallel), a line against a corner or a point of an arc a
  triangle, a corner against an arc a cone, an arc against an arc a cone
  between the circles; corners against corners fold. This is the classic
  developable transition (a rectangle to a circle is four triangles and
  four corner cones, tangent to each other): **die formed** it is the part,
  and its blank is exact. On the **press brake** each cone is cut into flat
  facets: triangles from the corner to the arc's chords, or trapezoids
  between both arcs' chords at the same angles (parallel, so flat), by a
  count per region (default 4) or as few as keep the chords within a chord
  tolerance. Every facet is exact, so the blank is.
- **Other profiles** (on angled planes, not convex, ellipses or splines):
  the press brake cuts them into polylines the same way and joins them by
  triangles, each time across the shorter diagonal. Die formed they have no
  such surface and are refused in words, unless the Approximate development
  is chosen (below, mitcad#2560).
- **Seam and gap.** A closed transition opens along a ruling: the shortest
  (sampled over each region and refined), or the one through the point at
  the `seam` fraction of the first profile's length (a corner's: the one
  after its fan). Both profiles lose `gap / 2` (default the thickness) each
  side of it, measured along them. Where the two cut points fall in one
  flat region its new edge joins them; where they fall in different regions
  (the cut on the circle of a rectangle-to-circle transition lies in a
  corner's cone, the one on the rectangle in a side's triangle), triangles
  and cones from the farther cut point fill the part between (still
  developable; they meet the next region at slight folds, 2.45 degrees for
  the test's rectangle 120 x 80 to a circle of radius 30, 70 up). Two
  arcs' region is trimmed at the same angle on both arcs (a cone from the
  farther point fills the rest), so the gap there is at least `gap` on both
  profiles.
- **Depth and folds.** The surface `h` behind the front is the regions
  moved along their normals; where two regions meet at a fold both end on
  the mitre (`(n1 + n2) / (1 + n1 . n2)`), so a flat region stays flat and
  exactly `h` behind its front. A curved region meeting a fold keeps a flat
  sliver between its own moved ruling and the mitre. Folds are signed as
  bends are: positive towards the back; a fold's inside is on the front
  when the surface turns away from the material. Folds sharper than 170
  degrees are refused; where the moved surface would run backwards (the
  material on the inside of a sharp corner's fan, a rectangle's corners
  with the material inwards) it is refused as the material folding over
  itself.
- **Blank.** The chart is the development of the neutral surface, `K t`
  behind the front (the rules' K-factor; kerf-bend rules at the middle;
  measured fold allowances are refused, as for a rolled face, mitcad#2435).
  Flat bands are laid down exactly; a curved band is laid down as a strip
  of thin triangles at 1024 and 2048 steps, extrapolated (Richardson), its
  edges cubic B-splines through 129 stations with their tangents. The
  chart is checked not to overlap itself. Since the area of a developable
  surface moved along its normals grows linearly with the distance (folds
  meeting on mitres too), the folded body's volume is the thickness times
  the middle surface's area: with K 0.5 the blank's area times `t`.
- **Folds are sharp** unless Bends is on (below, mitcad#2560). The facets
  meet at folds without a bend radius (the bend table reports radius 0 and
  no allowance); a profile's sharp corner folds sharply (a rectangle to a
  rectangle folds about 89 degrees at its corners).
- **Body.** The folded body is built of B-spline patches sewn into one solid
  (`Kernel::patch_body`): the front and the back of every band, the walls
  along both profiles and at the seam or the open ends. Flat bands are
  bilinear patches (exactly planar); curved ones cubic along the profiles
  (through 129 exact stations with their tangents, within about 1e-6 mm)
  and straight across.
- **Sheet state and Flat Pattern.** One panel (slot 0) with a
  `SheetEmbedding::Lofted` chart: the neutral surface's rulings in the body
  and in the chart, the lines the Flat Pattern marks, the neutral depth and
  K-factor. The Flat Pattern lays the chart in the plane touching the first
  ruling: the folds are its bend lines (`bends`, one line each, with their
  angles, up or down), die formed the lines where flat regions meet formed
  ones are `facet_lines`; the DXF has them on `BEND_UP`/`BEND_DOWN` and
  `FACET_LINE`, a drawing's flat view draws them as bend lines and thin
  lines.
- **Not yet** (follow-ups): Sheet Cut and the other sheet features on its
  sheet (refused in words), cuts that reach the sheet's edge, bends where
  folds meet in a point, import from other files.
- **Tests.** `lofted_tests.rs` (chains; a rectangle to an offset rectangle
  as four exact trapezoids; concentric circles laid flat into an annular
  sector, every station on its circles about the apex; eccentric circles'
  rulings and profile lengths kept; a rectangle to a circle in triangles and
  corner cones, faceted, more facets a larger blank, inwards refused; the
  seam and the gap's lengths; angled planes triangulated; a profile turning
  both ways not matched), `cli.sheet_lofted_flange` (the Done-when cases:
  blanks against exact areas, volumes against `t` times the middle
  surface's area, bend and facet lines, DXFs, refusals) and the UI test
  `tools/ui-sheet-lofted-flange-test.sh` with its Windows and macOS twins.

#### Holes, bends and the approximate development (mitcad#2560)

- **Holes and cuts** (`sheet/folded/lofted_cut.rs`, called by the sheet
  adapter of ordinary features). The cut's walls are the result's faces
  that are not the lofted flange's own; their edges on the front (every
  sample on the transition's front within 0.1 µm, found over the rulings'
  triangles moved to the front, each ruling at its own depth) are laid
  into the chart: on a flat facet by the facet's own map, exactly (a
  circle stays a circle), elsewhere as a spline through 65 laid points with
  their tangents. Every front edge moved the thickness along the material
  direction must lie on a back edge of the walls (0.01 mm, the imports'
  tolerance): the walls run through the sheet at right angles to it. The
  front edges join into closed loops, each taken out of the chart by a
  sheet trim (the chart's curves keep their identities); the body stays
  the feature's result. Refused in words, the body then without its sheet
  meaning: material added, slanted walls or a pocket, a cut reaching the
  sheet's edge (not yet). The inverse of the chart's map
  (`lofted::located`, `chart_coordinates`) also serves `flat_point` on the
  lofted body.
- **Bends** (`bends`, press brake). At each fold between facets a cylinder
  strip tangent to both: the front's radius R is the rules' radius where
  the fold's inside is on the front and that plus the thickness where it is
  on the back; the facets are cut back `R tan(θ/2)` along the fold (the
  setback), the cylinder's lines run parallel to the fold, and the bend's
  ends are where they meet the profiles' planes (ellipse arcs: the bend
  rounds the profiles there). The joins are smooth, so the surface at
  any depth is the facets moved along their normals and the cylinders'
  radii changed by the depth: exact. The facets lie flat at K t behind the
  front, each bend at K t from its inside (`(1 - 2K) t` deeper where its
  inside is on the back, each ruling stored with its depth), so the bend's
  width in the blank is its allowance `θ (r + K t)`. The Flat Pattern's
  bend has the rules' radius, that allowance, its centre line and its two
  tangent lines. Where two folds meet in a point of a profile (a sharp
  corner's fan, the tip of a triangle against an arc, the press brake's
  triangulated transitions) bends of one radius would run into each other:
  no cylinder pair closes there without stretching or a relief, so it is
  refused in words (fold sharp, or join profiles whose facets all have four
  sides: two rectangles, two circles, two rounded rectangles). A facet too
  narrow for its two setbacks is refused too. Beside the seam two arcs'
  region is cut at one angle on both arcs, so no cone's fan of facets meets
  in the seam's cut point.
- **Approximate development** (`development: triangulated`, die formed).
  Profiles without a developable surface are joined by the ruled surface
  between the points at the same share of both profiles' lengths, from the
  seam (`seam`, or the shortest line) less `gap / 2` each side; a band
  between every two places where either profile has a corner or another
  curve begins, so each band is smooth (a crease along the line from a
  corner). The front's bands are B-spline patches (exact lines across,
  cubic through 129 exact stations along), sewn and made solid the
  thickness behind them (`Kernel::thicken`, the side found by a point
  inside). The bands are the faces of mitcad#2442's strip: their grids (the
  triangles shared by their lengths, 16 steps across) are laid flat at K t
  behind the front, and the feature says how far the pattern's lengths
  along the edges and across differ from the surface's. The sheet is the
  triangulated pattern's, as Convert to Sheet Metal makes it (a plane
  chart, `approximation`), so sheet features refuse it and ordinary
  features take its sheet meaning. Profiles with the exact surface are
  made exactly, with a remark.
- **Tests.** `lofted_tests.rs` (an L's bend: the facets less their
  setbacks and the arc at the middle; a rectangle's corner bends, their
  depth with the material inwards; folds meeting in a point refused; two
  arcs cut at one angle at the seam; a point of the body located in the
  chart and back, a circle staying a circle; the ruled transition between
  a square and a circle on angled planes, its bands joined and on the
  profiles, laid flat with the stated difference measured) and
  `cli.sheet_lofted_flange_cuts_bends` (a hole of diameter 10 through a
  rectangle-to-circle's facet: the blank exactly its area less; the L and
  the rectangles with bends of radius 2 against the facets less their
  setbacks and the bends' arcs, volumes t times the blank at K 0.5; the
  rectangle to the circle with bends refused; a square to a circle on
  angled planes die formed refused, then laid flat approximately).

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
| mitcad#209: cuts and reliefs | Production thickness-normal Cut crossing one/two bends, holes and multiple contours; cuts between Unfold and Refold; rectangular and round bend reliefs, and rectangular, round and tear corner reliefs; invalid boundary/short-flange cases; rule edits after cuts. |
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

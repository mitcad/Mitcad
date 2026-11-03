# The .f3d import: fallbacks, outcomes and retries

Items that fall back to the file's stored bodies, the outcomes on the
timeline, items suppressed in the file, retries on the file's exact state
and the geometric tolerance rules around them. Part of the [.f3d
import](f3d.md) notes; the [import documents](../README.md#documents) list
the other topics.

## Fallback

- A modelling item suppressed in the file (its result number is -1 and
  the first or second of the three state bytes before its health
  reference is set) comes in as a suppressed feature with its
  definition, unchecked, or as a placeholder (*Items suppressed in the
  file*, mitcad#180). One whose third byte is set made its change in
  other components only (its parts there, mitcad#169) and is skipped at
  once, as an `.ipt` part's feature whose state the history does not
  hold is (mitcad#96).
- A modelling item whose operation the history holds but which changed no
  body, and which none of its definitions reproduces, failed in the file
  and is skipped too (*Matching rules*, mitcad#156); so is one that names
  an earlier item's operation, having none of its own (mitcad#343).
- An item with a known state that cannot be translated, fails, or does not
  give its state takes that state at once: one base feature (`base` with
  `replaces`; the changed bodies keep their ids, so later features
  continue on the stored bodies).
- Any other failing item waits. The next feature is tried on the bodies as
  they are and on each of the next history states put in by one base
  feature. The first combination that reaches a state settles it: the
  waiting items become `fallback` (they made that state, or added bodies
  to the state the next feature reaches) or `skipped` (the history shows
  no change by them). Items still waiting at the end take the last state.
- Without a history the first failure replaces every body with the stored
  design and later modelling items are skipped.
- **Final comparison:** the replay is compared with the stored bodies by
  volume; with `compare`, bodies whose measures differ by more than 1e-4
  also by surface deviation and boolean differences
  (`Kernel::compare_shapes`), each for at most ten seconds and all for at
  most a minute (`CompareOptions::seconds`: the deviations are sampled
  first, and the booleans, which on coincident or nearly coincident
  shapes can take minutes or not return, stop at the limit and are then
  missing). When every sample lies within 0.01 mm of the other body the
  booleans are left out (`CompareOptions::booleans_above`, mitcad#69), and
  boolean differences larger than the deviations allow (four times the
  volume they span over both surfaces: the booleans took nearly coincident
  bodies as apart, a relative difference of 2) are unknown, not a
  difference. Bodies that still differ beyond the matching tolerance are
  replaced by the stored ones (a warning names the base feature).

**States that do not rebuild.** A rolled-back ASM state whose conversion
loses faces or does not build is unknown (the report counts them): no base
feature takes it, and an item with that state is taken unchecked (its
first definition that builds). `mitcad-f3d-inspect history` names the
problem edges. Two ASM quirks are handled by the conversion:

- ASM puts the seam of a full cylinder on the line where it touches planar
  faces (a hole tangent to a wall), so the edge is used twice in each
  direction; it is taken as closed.
- B-spline edges whose ends are up to 0.004 mm off their vertices build
  with wider tolerances (gaps up to 0.01 mm accepted).

**Bodies the history already holds.** An item whose state lies behind the
replay (a base feature took the bodies past it; ASM states of reordered
items or other components can precede items the timeline puts earlier)
continues on those bodies: its first definition that leaves them unchanged
(a join of material they already hold) is taken, verified by them. Without
a history the same holds after the fallback to the stored design.

## The outcomes on the timeline

The report's outcomes stay with the features (mitcad#170, the model's
import notes, [commands.md](../../model-api/src/commands.md#the-imports-outcomes-on-the-timeline)),
made from the report itself at the end of the import, so that the
timeline and the report say the same:

- Each fallback records the base features it made (one per component that
  changed, `Importer::note_bases`) and the items they stand in for
  (`Importer::stands_in`: the item that settled it and the waiting items
  it took). Their note is `Stands in for <item> (<the item's note>)` for
  each item, the waiting items' `; in the fallback of <base>` left out.
  A fallback item whose base feature no call site recorded is found by
  its `features`. Base features whose uid a later undone try reused are
  left alone (only base features get these notes).
- A base feature made for another reason (the bodies brought to a state
  exactly, the stored bodies at the end) gets the report's warning about
  it (`Importer::base_warning`, which writes both).
- A `partial` item's features get `Partly imported: <its note>`.
- An item skipped as suppressed in the file (one whose definition did not
  come in, mitcad#180), as one the file keeps no result for, or as
  failed in the file gets a suppressed `placeholder` at its place in its
  component (`Importer::placeholder`), its note the item's note; items
  of no geometry (`Class::NoGeometry`) get none. The placeholder is not
  in the item's `features` (it builds nothing), so the report and the
  coverage counts are as before.
- An item suppressed in the file that came in as suppressed features
  (mitcad#180): its features get its note (`suppressed in the file; not
  checked against the file's history`); a suppressed sketch's says
  `suppressed in the file` after what it says otherwise.

The FreeCAD import does the same from its report
(`timeline_notes::freecad_notes`: fallbacks with their notes, partial
sketches and features) and adds a placeholder for a feature suppressed in
FreeCAD (`suppressed in FreeCAD; not replayed: ...`).

## Items suppressed in the file

mitcad#180 (`suppressed.rs`). **The state bytes.** The three bytes before
an item tail's health reference (`_f3d.flags`) are set only on items with
result number -1 (both corpora, 9043 items). With result -1:

| Bytes | Items in the corpora | What they mean |
|---|---|---|
| `000100` | 50 (extrusions, fillets, sketches, revolves, threads, a chamfer, a canvas) | suppressed itself (the user's action: the design API, the timeline menu, a configuration's row or a suppressed group's member give the same bytes) |
| `010000` | 64 (fillets, extrusions, sketches, revolves, threads, a chamfer) | suppressed through an item it depends on (a fillet of a suppressed hole's rim, the extrusion of a suppressed sketch) |
| `000001` | 5 (4 extrusions, a revolution) | the change is all in other components, in the item's parts there (mitcad#169); each has its part |
| `000000` | sketches, construction geometry, assembly items; modelling items at the end of the timeline | no result to keep, or after the timeline marker (never computed) |

Which of the two suppressed bytes an item has was settled with the
controlled variants: an item and its dependent are suppressed
together by suppressing the first, and then the dependent has `010000`
and the first `000100` (suppressing the dependent too afterwards leaves
`010000`); suppress the dependent alone and it has `000100`;
two independent items suppressed one by one both have `000100`. A
dependent's other bytes are the same as when it is suppressed itself
(the two models of a fillet differ in these two bytes alone). In the
corpora 24 of the 27 designs with suppressed items begin with a `000100`
item, as an item suppressed through another cannot come first; the other
three are versions of one design, whose first suppressed item is a fillet
with `010000` that follows a coil (its parent is not seen: unexplained).
Of the suppressed fillets, 32 of the 39 with `010000` have a face of an
edge that no history state holds (the parent's, or no edge between
the faces named), and none of the 9 with
`000100` has (the census). Items with these bytes also have
`01 01 00` where the others have `01 00 00` after the health reference
(the middle byte is 1 for either), and -1 where the others have the
number of their start state (the result number before theirs). An item
that failed in the file has the bytes and numbers of a healthy item; the
history tells it (mitcad#156, *Fallback*). `decode::item_state` reads the bytes,
`decode::item_suppression` tells the two apart (`Suppression::Direct`,
`Indirect`), the stream decoder writes `isSuppressed: true` for a
suppressed item, and `TimelineItem::file_state` and
`TimelineItem::suppression` give the import the state from either (an
external dump's `isSuppressed` has no bytes and counts as direct).

**After the timeline marker**. Two more state bytes stand in
front of an item's last reference (`_f3d.after`: the two bytes before the
`6CCF41D5` reference that follows the health reference's `5FBE538E`
one). They are `00 00` on every ordinary item; the first is 1 on a
suppressed item (the middle one of `01 01 00` above) and on most items
after the timeline marker; the second is 1 on every item after the
marker (rolled back in the file). The controlled variant of a rolled-back
hole and fillet has `00 01` on both (the items keep their result
numbers, which the history no longer holds). In the corpora 52 items have
the second byte (10 of them with a result number), among them every one
of the 26 modelling items that have result -1 and no state byte (the 13
removal items, which have no result of their own, are no such items),
and they are the end of the timeline in 15 of the 16 designs that have
any (the sixteenth lists a component inserted from another design after
the root's last item). The decoder writes `isRolledBack: true` on such
items and the timeline's `original_marker_position` and
`restored_marker_position` (the position of the first of them).

**Suppressed features.** A modelling item suppressed in the file is
translated as any other (its inputs are decoded as on other items), its
definitions tried in their order (those a proven rule decoded first), no
guesses: no history state checks them, as the file keeps no result for
the item. The first that builds is taken: `parametric`, not verified,
note `suppressed in the file; not checked against the file's history`;
then it is suppressed. Items suppressed in a row (an extrusion and the
fillets of its edges, a sketch and its extrusion) stay built until the
next item that is not suppressed, so that each is translated on the
bodies the ones before it make, as in the file; then the run is
suppressed, the last first, and the bodies are those before it (a
warning says so if not). An item whose definitions do not build on the
run's bodies is tried on the bodies without them (it may have been made
while they were suppressed); when it builds there, the run before it is
suppressed and a new one starts with it. Suppressed sketches and
construction geometry come in as usual and are suppressed with their run.
Unsuppressing a feature in MitCad builds it where it stands (the items
before it in its run unsuppressed too, where it uses them, as in the
original application).

**Placeholders.** Where no definition builds (no translation, no inputs
decoded, only guesses, every definition fails, or the import gave up on
the item: stopped, out of time or memory, after the fallback to the
stored bodies), the item is `skipped` with `suppressed in the file; its
definition did not come in: <why>` and gets the placeholder of mitcad#170.
Suppressed assembly items get a placeholder, suppressed items of no
geometry none.

The report counts the suppressed modelling items
(`suppressed.suppressed_as_features`, `suppressed_as_placeholders`; the
text: `suppressed in the file: n modelling items as suppressed features,
m as placeholders`, with `(k of them through another suppressed item)`
for those suppressed through another, `suppressed_indirectly`, whose
placeholders' notes end `(suppressed through an item it depends on)`);
the corpus test prints their totals.

**Rolled-back items**. Where the timeline ends with items
after the file's marker (*After the timeline marker* above; not when
an item that is not rolled back comes after them), a modelling item after
it is taken as a suppressed one is: its definitions in their order, no
guesses, the first that builds, `parametric`, not verified, note `rolled
back in the file (after its timeline marker); not checked against the
file's history`, else a `skipped` placeholder `rolled back in the file
(after its timeline marker); its definition did not come in: <why>`. The
items stay built on each other while the import runs; at its end the
marker is moved before the first of them (`close_rolled_back`), so they
come in rolled back (status `rolled_back`) and the final bodies, which
the import then compares with the file's, are those before them.
Sketches and construction geometry after the marker come in as usual,
after it. The inputs are looked for in the state after the last item
before the item, as a suppressed item's (`inputs::no_result_state`).
The report counts them (`suppressed.rolled_back_as_features`,
`rolled_back_as_placeholders`; the text `rolled back in the file: n
modelling items as features after the marker, m as placeholders`). Before
this the items were tried against states the file does not keep and ended
as placeholders or fallbacks; MitCad moves the marker with its timeline
commands, so unrolling them builds them where they stand.

**Inputs.** The decoder finds a suppressed item's named inputs (fillet
and chamfer edges, faces, bodies) as any item's, in the state just
before it; the file keeps no state of its own for it, so that is the
state after the last item of its component before it
(`inputs::starts`, `Timeline::state_at`), and the notes say `before item
n (suppressed in the file)`. A fillet's edges between faces that a
suppressed item before it made (a fillet of a suppressed extrusion) are
in no state the file keeps: such a fillet stays a placeholder (`a face
of the edge is not in the history state`). A placeholder of a fillet or
chamfer gives why its edges found by their names did not build; the
history's guesses of its edges are not worked out (none could be taken).

Limits: a suppressed item after a pending fallback is translated on the
bodies before the pending items, and a later base feature that replaces
those bodies overrides what it builds when unsuppressed; a feature
suppressed in the file is not checked, so a definition that builds but
differs from the file's (another direction, other regions) is not seen.

## Large designs, fifth round (mitcad#119)

A profile of a 256-item design with stored bodies of 14 662 and 23 430
faces and what it changed are in [docs/performance.md](../../../docs/performance.md):
the checker's two quadratic steps on those bodies (port patches 0120 and
0121), face lookups through a point that pass over faces by their boxes
(`Kernel::face_boxes`), the edges of bodies kept within a budget of edges,
sketch snaps that project only the edges near each linked curve, face
counts from the shape's own map, and a quicker hash of entity indices and
circle seams worked out in parallel while reading. The design imports in
82 s instead of 149 s with eight threads and 169 s instead of 301 s with
one, with the same reports.

## Fallbacks without a B-rep round trip (mitcad#244)

A fallback's base feature holds the stored bodies as B-rep data. It used
to write each body's data and read it back to evaluate the feature, which
also named every face again and measured and checked the read body anew:
on the large design above about 2 s per fallback on its two large bodies.
Now the base feature's first evaluation adopts the shape the data was
written from (`Kernel::adopt_shape`; `Document::add_base_features` hands
the shapes to the evaluation beside their data): the same faces in the
same order with the names the data would get, sharing the stored body's
B-rep instead of a copy, and keeping what was measured of it that does
not depend on names (volume, box, face measures and boxes, edge curves,
the checker's verdict). The data is still written, for the design file;
an evaluation later (a design opened from its file) reads it. On that
design the reports are byte for byte the same; the import took 156 s
instead of 170 s with one thread and 76 s instead of 86 s with eight, and
its peak memory went from 2.5 to 1.9 GB and from 3.5 to 2.9 GB.

A retry of an item on the exact state before it replaced every body of
the replay, also those it already had; it keeps them now (next section).

## Retries on the file's exact state (mitcad#283)

An item whose definitions give no state on the replay's bodies is tried
again on the history's state before it exactly (in half its time), when
the bodies before it carry an earlier item's approximation (they are the
state's only within the matching tolerance), or when it is a fillet or
chamfer the geometry kernel failed on them (the stored state's edges are
split otherwise, and a rounding that fails on one builds on the other);
also an item the history names no state for, on bodies that carry an
approximation (`retry.rs`).

The replay's bodies that are already the state's were imported
correctly, and they stay: those that measure as a body of the state
(`Sig::same`), and those near one (`Sig::matches`) whose faces lie on its
faces within the geometric tolerance (`geometric_tolerance`, 0.01 mm:
the faces each has and the other lacks, `Kernel::new_faces`, measured
against the other's faces, `geometric::body_distance`). Only the bodies
that differ from the state (and the state's bodies the replay lacks, and
its sheets) are brought to it by a base feature, and the item is tried
there. Where nothing differs there is no retry: an item that still fails
takes the file's bodies as any item that fails (*Fallback*), and the
bodies before it stay parametric. Before mitcad#283 the retry replaced the
bodies the replay had within the tolerance (and, for a fillet or chamfer
the kernel failed on bodies that were the state's exactly, every body).

Replacing those bodies too is the user's choice (`import_f3d`'s
`retry_bodies`, `mitcad-cli import-f3d --retry-bodies keep|replace`;
`Options::retry_bodies`): `keep` (the default, also of the corpus tests),
`replace` (as before), or, in the application, `ask`: the first time a
retry would replace a body that matches, after the retry keeping them
gave no state, the import asks through the document's job
(`RecomputeMonitor::ask`, `RetryChoice`), and the answer holds for the
rest of the import, its later tries (after a hang) and the documents it
refers to. While the question waits the import ticks its watchdog.

The report's item gets `exact_retry`: the state, `keep` or `replace`,
the replay's bodies `kept` and `replaced` (`<name> (<uid>)`), and whether
the item `matched` there (else the replacement was undone, or none was
made); the text report a line under the item. The design's
`retry_question` names the item asked at and the answer.

Checked on a design of 256 items (content id `0b758b01`): its six
retries gave no state before either (all fell back), so the outcomes are
the same (220 parametric of 256, 31 of 31 final bodies exact); the retries
now keep the bodies within 0.01 mm (0.2–2 µm here) and replace only those
off by more (0.17 mm, 2.1 mm). On five smaller designs whose fillet
gave its state only on the stored bodies (content ids `59a1e52b`,
`7aa27c24`, `3a5fe9db`, `e4424061`, `4fe294cf`), the fillet now takes the
file's bodies by default, the bodies before it staying parametric: those
bodies are the file's within 0–3.4 µm, but an earlier item split their
faces otherwise (a revolution's or extrusion's face in two, a chamfer's
faces; one more face than the stored body, nine more on one), or the
fillet runs over an earlier fillet's own surfaces (one design), and the
geometry kernel fails the fillet on the replay's edges. What these need is
the fillet building on the replay's split faces (or the replay's faces
merged where the file has one), not the stored bodies.

## Fallbacks keep the bodies imported correctly (mitcad#285)

Every other base feature that brings the replay's bodies to a history
state or to the stored design keeps the bodies that match it as the
retry above does (`Stay::Matching`, `Importer::pair_within`): an item's
fallback, the pending items' bodies (one at a time or in one step with an
item), the state after an approximate extrusion or a guessed pattern or
mirror, the state at an item that failed in the file, and the stored
bodies at the end. A body that is the state's body exactly, or whose
faces lie on it within the geometric tolerance, was imported correctly and
stays as the replay made it, parametric; only the bodies that differ by
more (the bodies of the items that failed among them) are replaced.
Before, each of these replaced every body that was not the state's
exactly. The bodies kept carry their difference on: the distance the
replay carries from the state is measured after such a fallback.

What found bodies of the next states by their measures exactly now
treats a body kept within the tolerance as its stored body
(`Importer::stored_counterparts`), which a later state that leaves it
alone holds exactly: the combines, splits, removals and fillet edges
guessed from the bodies the next states change (a combine took a kept
body for its target before). Moving bodies into a new component takes
the same body, else the nearest within the matching tolerance.

Checked on 27 designs whose items take these paths (content ids, items
parametric before → after): `0b758b01` 220 → 220 of 256 (final bodies
31 of 31 within the tolerance, 29 of them exactly; the two others are
the replay's own, kept), `b596234a` 53 → 55 of 66 (the component move
and an extrusion after it now find the replay's bodies), `a1c980ab`
22 → 22 of 26 (a combine that fell back is now parametric, an extrusion
falls back, below), `f9d6d78a` 68 → 67 of 78, `9e53857a` 24 → 23 of 28,
`ad0df696` 13 → 12 of 14, and the other 21 the same; 1278 → 1277 of
1502 items in all, with 29 base features that bring bodies instead of 44.
What now falls back:

- An extrusion that joins a small cylinder to a body a chamfer made 2.6 µm
  off the file's (the chamfer's body has one face more than the stored
  one, 1.5 · 10⁻⁵ of its volume): on the replay's body the tool stays
  apart from it (it comes in as a solid of its own); the join's near-contact
  fusing reaches 1 · 10⁻⁵ mm only. Needs the join to fuse within the
  import's geometric tolerance where the tool touches nothing, or the
  chamfer's face as the file splits it.
- A join of an extrusion on a body whose faces each lie on the file's (no
  face off; its volume 1.8 · 10⁻⁵ off, the stored faces' measure): the
  geometry kernel's boolean gives an invalid face (intersecting wires) on
  the replay's body and a valid result on the stored one. Needs the join
  retried with a fuzzy value when its result is invalid.
- A base feature of the file whose state is the one before it: it was
  counted parametric because its fallback replaced a body of the replay
  that was 1.4 µm off (one face more than the stored body); nothing
  replaces that body now, and the item is skipped as one that changes
  nothing in the history.

## Joins within the geometric tolerance (mitcad#298)

A body kept within the geometric tolerance (above) can make a join that
works on the stored body fail on the replay's. The import therefore gives
the document its geometric tolerance as the join tolerance
(`Document::set_join_tolerance`, kept in the design file; commands.md
*Join tolerance of imported designs*), and the joins of its features go
through `Kernel::join_within`: where the exact join fails, a tool that
touches no target joins the targets within the tolerance, a tool solid
left on its own beside a target within it is merged, and an invalid
result is tried again, each with the smallest fuzzy value that works
(from 1e-5 mm up). Joins that work exactly are as before. Checked on the
designs of the section above (content ids, items parametric before →
after):

- `a1c980ab` 22 → 23 of 26: an extrusion joining a small cylinder to a
  body that a chamfer made a few micrometres off the file's (the chamfer
  is built 0.1 % smaller than asked: at its exact size it takes a face
  away, which the geometry kernel cannot build, so the body keeps one face
  more). The cylinder's start face lies within 1e-6 mm of the body's face,
  so it touches, but not on it for the exact boolean, which left the
  cylinder a solid of its own; it now merges with a fuzzy value of
  1e-5 mm. The joined body is the replay's own (4.4 · 10⁻⁵ of its volume
  off the file's, the chamfer's difference), where a base feature brought
  the stored body before. One linked curve of a later sketch no longer
  lies within the snap distance (2 µm) of the replay's edge, which
  carries the chamfer's difference, and is kept as fixed reference
  geometry (one projection fewer).
- `9e53857a` 23 → 24 of 28 and `ad0df696` 12 → 13 of 14: a join of a
  plate whose face lies on a face of the body, against the line where
  two pipes of the same radius meet (the body's faces all lie on the
  file's; its volume 1.8 · 10⁻⁵ off). The exact join's result had an
  invalid face (intersecting wires); with a fuzzy value of 1e-5 mm it is
  valid and matches the file's history. A later cut in the first design
  still falls back as before: its result on the replay's body is invalid
  (an unorientable face), a cut, which keeps its exact behaviour.
- `0b758b01` 220 → 220 of 256, the final bodies as before.

## Faces the replay split where the files have one (mitcad#284)

The extra faces of those five designs had three causes. A revolution or
extrusion joined onto a cylinder of the same radius left two faces of one
cylinder where their seams lay half a turn apart: the face merge after the
boolean failed to rebuild the merged face's seam (OCCT port patch 0220);
the faces now merge, as in the files. A chamfer as wide as a ring beside
its edge gave the edges along the ring's far circle p-curves half a turn
off (a tolerance of the ring's diameter) and a rounding of them failed;
and a rounding as high as a boss's wall left a sliver of the wall (built
0.1 % smaller): both are fixed by the port's patches 0144 to 0149
(mitcad#263). Checked by content id (features parametric, all verified
against the file's history; final bodies within 1e-5 of the file's):

| design | before | after | what still falls back, and why |
|---|---|---|---|
| `59a1e52b` | 12 / 13 | 13 / 13 | |
| `4fe294cf` | 23 / 24 | 24 / 24 | |
| `3a5fe9db` | 24 / 25 | 23 / 25 | the boss's foot is now rounded exactly (its wall vanishes, as in the file); rounding its top edge then, between the top and the foot's torus, fails in OCCT's walk for any radius (a boss with an exact foot rounding reproduces it); a chamfer as wide as a face is still built 0.1 % smaller, and a later rounding fails at a vertex beside its sliver |
| `7aa27c24` | 12 / 13 | 12 / 13 | the chamfer of a pocket's floor edges takes away both faces of its edges (the wall and a ledge, both 1.5 wide): OCCT builds it only 0.1 % smaller, and the rounding after it fails on the slivers |
| `e4424061` | 7 / 8 | 7 / 8 | the replay is the file's within 0.5 µm (the file's edges there carry that as their tolerance); the rounding of a square hole's vertical edges fails in OCCT's walk at their upper ends on an earlier rounding's corner torus, and builds on the stored body |

## Roundings and chamfers left from mitcad#284 (mitcad#300)

Three causes, each in OCCT's fillet (port patches 0350 to 0352):

- A rounding whose contact runs along the far edge of a face it consumes
  (a boss's foot rounded as high as the boss) marked the edge it leaves
  between the blend and the face beyond (the boss's top, at a right angle)
  as tangent, and so did a rounding with the walls of a hole cut through it
  before. Rounding such an edge later failed in the walk for any radius:
  the boss's top edge, one closed edge, was taken for a contour ending at
  its vertex; at a hole's corner the walk went on past the corner's top onto
  the earlier rounding's torus as if it continued the hole's wall. The
  stored bodies carry no such mark (they split the boss's top circle in two
  arcs, which is why the rounding built there). Patch 0350.
- A chamfer of a pocket's wall foot, a closed chain of lines and arcs, as
  wide as the ledge under it and as high as the wall (both faces of its
  edges vanish): along the lines the contact on the ledge's boundary was
  lost, and the stripe's start was a point where its ends had been made
  vertices. Patch 0351. Where the edge is convex (a lip over a wider
  pocket, as here: the chamfer folds back under the top), the top was given
  the wrong side of its edge with the chamfer, an invalid shape. Patch 0352.

Checked by content id (features parametric, verified against the file's
history):

| design | before | after | what still falls back, and why |
|---|---|---|---|
| `3a5fe9db` | 23 / 25 | 24 / 25 | the boss's top rounding (Fillet4) is now parametric. Two chamfers as wide as a tab and a strip of the plate's top (Chamfer1, Chamfer2) are not built at their exact size: the strip's far side is three edges end to end, and the strip goes on past the chamfered chain's ends as a face of the same plane; Chamfer1 is built 0.1 % smaller (2 µm short at the strip's corner). On the stored body before it, Chamfer2 now builds exactly (0352; before, an invalid shape); on the replay's body, 2 µm off at that corner, its exact try gives an invalid shape that healing repairs to a wrong volume, and it falls back (before, it was built 0.1 % smaller and the rounding after it, Fillet10, failed on its sliver; now Fillet10 is parametric on the file's body). What is needed: Chamfer1 built at its exact size (a contact along a chain of edges of the consumed strip, the strip's coplanar continuation at the chain's ends) |
| `7aa27c24` | 12 / 13 | 13 / 13 | |
| `e4424061` | 7 / 8 | 8 / 8 | |

The 249 fillets and chamfers of the mitcad#107 failure dump give the same
outcome with the patches as without, through MitCad's dressup and through
OCCT's fillet alone (161 built by the dressup, 166 by OCCT, 113 of those
valid).

## Measuring points against free-form faces (mitcad#442)

The geometric check's first measurement of `db73863f` item 27 (a
rounding of both crest edges of a thread cut as a groove: 49 new faces,
16 points each, against the file's body, whose thread flanks are splines
of 2484 mm² over the whole thread) took 19–40 s of the item's 60 s, so
the item fell back when the closer definitions it looked for after it
ended past its time. Sampled under a debugger, every stack in
`analysis_boundary_distances` was in OCCT's distance query from a point
to one face (`BRepExtrema_DistShapeShape`), called by the boundary
index's `nearest` for each point not placed on a face by its projection
(a point of the replay's rounding lies within 1e-4 mm of the file's
spline approximation, not within the 1e-5 that places it): building the
query's search grid over the whole flank (a third of the samples) and
classifying each extremum it found, one a turn of the thread, with a
classifier set up anew (the rest). The large-spline path that lets go of
a projection after each call was not the cause: nothing it lets go of is
rebuilt per point.

A free-form face is now measured from what the boundary index keeps of
it (`BoundaryIndex::free_form_distance`, `core/cpp/bridge/analysis.cpp`):
the distance to its edges, then a local search of the surface
(`Extrema_GenLocateExtPS`) from each of its grid's points nearer than
their neighbours and nearer to the point than the edges plus the grid's
reach (any extremum nearer than the edges has a grid point within the
reach), the nearest first, each foot point taken where the face's kept
exact classifier puts it on the face. Cones, tori and a point with more
than 64 such grid points (about as far from much of the face) keep OCCT's
query. Over the design's import, measured beside OCCT's query point by
point (about 20 000 points), 80 distances differed by more than 1e-7 mm,
each one nearer than the query's, with its foot point on the face by
OCCT's own classifier: the query had missed or misplaced an extremum (by
2e-3 mm at most for distances under 5 mm). The import's report is the
same.

| `db73863f` item 27, debug build | before | after |
|---|---|---|
| first measurement of its 49 new faces | 18.8 s | 2.5 s, 2.5 s |
| its geometric check | 19.2 s (20.9 s sampled) | 2.9 s, 2.8 s |
| the item | 29.8 s, parametric | 10.1 s, 10.7 s, parametric |

`core.bridge` (`test_boundary_distances_off_free_form_faces`) measures
points off a wavy spline of many knot spans and off its prism's extruded
sides, in two calls: every distance OCCT's query's within 1e-6, and the
query itself not run (`analysis_whole_face_searches`).

## Import into a document with existing bodies

History replay records the receiving document's body UIDs and shape versions
before importing (`src/body_scope.rs`, mitcad#624). Those solids and sheets
are outside this import: references, candidate signatures, geometric shape
checks, fallback replacement and the final report see only replayed bodies.
Identical geometry cannot make an existing body stand for an imported one.
The same scope follows candidate workers' document copies. Its guard is tied
to the actual document and restores the preceding scope when dropped, so
nested imports and separate documents with the same UIDs remain independent.

Implicit boolean participants are restricted to imported solids. An initial
join without an imported target creates a new body, and a candidate that
would change or remove a receiving body is undone and rejected. The `.ipt`
wrapper also limits its measured imported-body report to new UIDs. The mock
tests cover matching and different existing solids, existing sheets, verified
and fallback replay, worker copies and undo/redo; `core.exchange` checks the
same case with the synthetic hidden steel cube and an existing aluminum cube.


## Final bodies that differ from the stored ones (mitcad#1175)

The replay matches a state within 1e-3 relative (`history::RELATIVE`),
so a final body that close to the stored one came in without a word. The
final comparison (`compare_final`) now adds one warning,
`DesignReport::body_differences`, that names (the first five of) the
final bodies that are not those stored in the file: a stored body the
replay has none for, a body whose volume differs by more than 1e-6
relative (`BODY_VOLUME_TOLERANCE`) or whose faces differ in number (the
report's `bodies[].file_faces` and `mitcad_faces`), and a replayed body
the file does not store. Like every warning it is among the import's
notices, which the application shows after the import. Where a fallback
brought the stored bodies, they agree and nothing is said.

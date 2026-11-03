# The .f3d import: learning the undecoded inputs

The learning dump and the audits of the decoder's inputs against the
history and the API reference dumps. Part of the [.f3d import](f3d.md)
notes; the [import documents](../README.md#documents) list the other topics.

## Learning the undecoded inputs

Every item whose definition the history settles (*What the history
settles*) is a labelled example of what the stream decoder does not read
yet: the item's raw record and the answer the history accepted
(mitcad#96). The learning dump collects them, and a tester checks rules
from the record's bytes to the answer on them, without geometry.

**Dump.** `mitcad-cli import-f3d part.f3d --learn DIR`, the `learn` field
of the `import_f3d` command, or `MITCAD_IMPORT_LEARN=DIR` (read by
`mitcad-ffi`, so `test_f3d_import --corpus` writes it for every design):

```bash
MITCAD_IMPORT_LEARN=$HOME/f3d-learn build/rel/core/test_f3d_import --corpus \
    --time-limit 300 --hang-limit 300 --jobs 5 --memory 4G
```

The lines are derived from the designs: keep the directory outside the
repository, like the corpora. Per design, `DIR/settled/<design>.jsonl`
holds the items accepted against their known history state and
`DIR/unsettled/<design>.jsonl` the others that had candidates (none
accepted, or accepted unchecked), plus fillet and chamfer items whose
translation failed or offered no candidates (mitcad#106). Their decoded
edge match diagnostics remain available even without history fallback;
`candidates` is empty and `accepted` and `answer` are null when none were
offered or accepted. A rule must not contradict unsettled examples either.
A rerun replaces a design's files. The dump's own work (the
bodies, inputs and sketches it records when an item's definitions are
translated: seconds on a large item) is left out of the item's time and
the import's time limit (`learn::Clock`; the deadlines move by it), so
that a dump run gives no item up that a run without it keeps; candidates
are kept once by a hash of their definitions, and at most 1000 per item
(an item can have tens of thousands, which took gigabytes). The import reports to
`core/import-replay/src/learn.rs` at four points: the candidates translated
(`Importer::translate`), ranked (`Importer::rank`), the one accepted
(`Importer::accepted`) and the end of the import; the raw records come
from `mitcad_f3d::design::learn`.

One line (`schema` 1):

| Key | |
|---|---|
| `file`, `item`, `name`, `type`, `class`, `class_version`, `object_id`, `f3d` | the design, the item's timeline index, its type and the decoder's raw item fields |
| `status`, `outcome`, `note` | `settled` or `unsettled`; how the item came in |
| `translation_error` | present only when a fillet or chamfer translation failed: the most recent failure text, including the input position and decoder/replay reason when available. Retained if a later translation offers candidates |
| `known` | what the decoder made of the item (the dump IR's item without `_f3d`) |
| `record` | the item's object and the input objects it refers to, three references deep: `objects[]` with `path` (`item`, `item>0897AF07#0>C46D3EEB#0`: the class's first eight digits and the position among the parent's references to that class), `id`, `class`, `module`, `version`, `len`, `hex` (up to 16 KiB), `sub` (sub-chunk start) and `tokens` (`header`, `root` with its references and attributes, `ref` with `id` and `class`, `str16` with `text`, `sub`). Other timeline items, sketches and their entities, parameters, components and the like are not followed |
| `context.sketches` | per sketch the candidates use (`T<index>`): `regions[]` as MitCad computes them, in the file's ids (`key`, `outer` and `holes` curve lists, `area` mm², `centroid` and an `inside` point in sketch mm), the `frame` the import placed the sketch on (`origin`, `x_axis`, `y_axis`, `normal`; model mm, the space of an extrusion's stored direction) and `curves` by the file's ids (`object`, `class`, the `u64` root attributes such as `crv_primary_id`, `crv_secondary_id`, `pt_tag`; `type`, `construction`, `geometry` from the dump, sketch space in cm). Curves MitCad made itself appear as `m<id>` |
| `context.bodies`, `context.edges` | the bodies before the item (`uid`, `volume`, `area`, `center`, `faces`; at most 200) and the edges the candidates name (`mid`, `length`, `body`), mm |
| `context.resolved` | the bodies, faces and edges the decoder gave for the item (by JSON pointer into `known`, e.g. `/detail/edgeSets/0/edges/2`) as the replay's (`{"kind": "body", "uid"}`, `{"kind", "body", "name"}`; `null`: not found), so that a decoded input can be compared with the answer |
| `context.edge_matches` | additive dressup matcher diagnostics for decoded edge inputs at those same pointers (mitcad#106): `status` (`matched`, `decoder_unresolved`, `missing_in_replay`, `ambiguous_in_replay`), `decoder_found` (the decoder's `found` text), `count` and either `matches[]` (`body`, `name`, `mid` mm, `direction`) or ambiguity `candidates[]` (`body`, `name`). A matched result also has `chosen_from_multiple`: at least one match selected among multiple distinct replay edges (absent in older dumps). The first 64 decoded inputs are considered, with at most 16 matches or candidates per edge; `count` retains the full number. Bodies and names use the file's timeline indices. This uses the actual dressup matcher, including circles and split edges; `context.resolved` retains its older midpoint matching for compatibility. The recipe tail's order of the faces round the edge's ends (`_f3d.end_faces`) tells equal mirror-image edges apart in the decoder (EDGE_ORDER.md) |
| `candidates[]` | in the order tried, the first 1000 (`cost.candidates` counts all, `accepted` is the rank among all): `defs` (MitCad feature definitions in the file's terms: feature uids as timeline indices `T<i>`, profile regions as `{"sketch": "T<i>", "region": <index in regions>}`, sketch curves by the file's ids), `note`, `guess`, `input_provenance` (`read`, `inferred`, `unknown`), `predicted` |
| `accepted`, `answer` | the accepted candidate's rank and the candidate itself |
| `cost` | `candidates`, `rank`, `seconds` from the translation to the acceptance |

**Tester.** `tools/f3d-learn/learn.py` (Python 3, standard library;
`--data DIR`, default `MITCAD_IMPORT_LEARN`) reads only the dump:

- `stats`: settled and unsettled items per type with their cost;
- `show` / `fields`: examples with their objects, tokens and the gaps
  between them, and one object's fields over all examples;
- `test`: a rule as a Python expression (`--expr`, compared with a target
  such as `regions`, `curves`, `edges`, `flip`, `operation`, `extent`,
  `participants`, `axis`, `def.<key>`), a check (`--check`) or a file
  defining `predict(ex)` or `check(ex)` (`--rule`); prints how many
  examples it explains, contradicts or does not apply to, the
  counterexamples, and with `--unsettled` whether its predictions for
  the unsettled items are among their candidates;
- `search`: rule families over every field of every object (fields are
  addressed `gK+N` / `gK-N` in the K-th gap, `@N`, or `rK` for the K-th
  reference, read as `u8` … `f64` or `sign`): `eq` (the field is the
  value, or the number of selected entities), `map` (each value stands
  for one answer class), `idin` (the field holds an id of an answer
  entity and of no other), `idset` (the object holds the ids of the
  answer's entities), `bits` (bit i selects region i);
- `survey`: per type the record's objects, their lengths and gaps, and
  the fields that vary with their values and how purely they go with a
  target.

Its unit tests (`tools/f3d-learn/test_learn.py`, the ctest
`tools.f3d_learn`) run on a synthetic dataset; `core/import-replay/src/learn.rs` and
`mitcad_f3d::design::learn` have their own.

A rule that explains every settled example (or every counterexample with
a reason) and agrees with the unsettled ones goes into `core/f3d` with a
unit test on a synthetic record; the import then tries the decoded
answer first, the guesses after it.

Decoded so far (mitcad#96, [*What the history settles*](f3d-history.md#what-the-history-settles) says how the import
uses them): the profile loops of extrusions and revolutions
(`_f3d_profile_loops`), a revolution's operation, profile count and
sketch-line axis, a circular pattern's axis line (`_f3d_axis`) and the
direction of each rectangular pattern direction from a construction axis,
a mirror's `isCombine`, a hole's through-all flag where the file keeps
a depth (`_f3d_through_all`, a lead), and fillet and chamfer edges
(the item tail's anchor, recipes that name the edge itself or carry a
second list, face selections, inputs of the same names, the replay's
edges where the middle does not tell, a two-distance chamfer's sides:
[*Named inputs*](f3d-history.md#named-inputs)), and items without a result ([*Fallback*](f3d-fallbacks.md#fallback)). A mirror's plane and the patterns'
quantities the decoder read already (the dump confirms them on every
settled item). For extrusions also the flag byte after the extent codes,
the input slots' roles, the object of an extent up to an object and a
symmetric extent's length (`ExtrudeFields`, `decode::extent_slots`; in
the dump `_f3d.extrude.flag`, `slot_roles`, `full_length` and
`extentOne.entity`). Then (mitcad#104): both sides of a two-sided
extrusion up to objects, a combine's body removals in other components
([*Components*](f3d-components.md#components)) and an oblique stored extrusion vector.

### Input decoding audit (mitcad#186)

The audit starts from the existing 11 October 2026 learning snapshot on
`bd34bc6b`: the complete main corpus and the 78 largest backup designs.
The snapshot is retained unchanged while rules and corrections are
checked. Its counts include unsettled dressups with no candidates; those
records still contain useful input and translation-failure evidence.

| Input group | Settled / unsettled examples | Reading or remaining uncertainty |
| --- | ---: | --- |
| Circular patterns | 83 / 14 | All 97 have decoded seeds. The direct path now resolves sketch-line axes as well as origin/construction axes. Compute/symmetry flags have no contrasting accepted labels here, so this snapshot cannot establish their byte meanings. |
| Rectangular patterns and mirrors | 18 / 20; 18 / 6 | All 38 rectangular patterns have seeds and a first direction; use those on the direct path, including body/face seeds. Keep either decoded mirror input when the other is missing. A stored-vector priority rule matches 13 settled axes, contradicts one and cannot compare four; flipping by the distance sign contradicts ten. Neither rule is adopted. Spacing/extent and symmetry also lack contrasting labels. |
| Fillet/chamfer edge sets | 808 / 176; 194 / 9 | Bind size-parameter holders to their set records, then the counted selection list, rather than flat input order; bind fillet options by the same holder. The input/option consistency check explains 802 settled + 173 unsettled fillets and 191 + 9 chamfers, with no contradictions; nine settled and three unsettled examples are not comparable. Of 1,191 known sets, 1,178 have the checked framing: 1,170 retain their selections and eight previously missing sets in four items are recovered. Independently checked on 43 sets in 42 reference features. This verifies decoded inputs, not successful kernel replay. |
| Extrusion direction, extent and participants | 1054 / 95 | Role-8 slots exist in 925 examples. They exclude termination references wrongly included in 80 old lists; six extras identify distinct replay bodies. Comparing against explicit accepted participants gives 254 matches and five existing resolver disagreements (795 not comparable); alternatives remain for unresolved replay. No new direction/extent rule was established. |
| Extrusion profiles and sketch selection | Same 1054 / 95 extrusions | Exact loop-to-region curve membership explains 572 settled choices, contradicts nine and abstains on 473; it is insufficient for curve pieces and child loops and is not adopted. A previous-sketch fallback is marked inferred. The groups overlap and must not be added together. |
| Thread inputs | 42 / 1 | All 43 records agree with their decoded ThreadInfo values and modeled/full/location flags. Candidate thread parameters are unchanged across all alternatives; 12 tube and two offset acceptances change geometric representation only. Fourteen items have a nonzero phase obtained from later history, an inferred input. Unsupported layouts remain undecoded; handedness is decoded from the controlled variants (below). |

The old candidate `guess` flag controls replay policy: some candidates
need history, including alternative geometric representations. Accepted
rank also describes the search, not whether an input was read. In this
snapshot, rank greater than zero or `guess` selects 72 circular patterns,
18 rectangular patterns, 18 mirrors, 64 fillets, 14 chamfers, 80 extrusions
and 14 threads. Those are screening counts, not counts of decoding
failures: the 14 threads comprise 12 tube and two offset alternatives.

The completed baseline later supplied all 48 main-corpus and 325 backup
designs. Rules were rechecked on that larger existing dump without another
geometry run. Counts below include every record, including empty candidate
lists; design versions are separate observations.

| Group in the full baseline | Settled / unsettled | Independent recheck |
| --- | ---: | --- |
| Circular patterns | 117 / 22 | All 139 retain decoded seeds. Compute and symmetry labels still provide no contrasting evidence for a byte rule. |
| Rectangular patterns and mirrors | 26 / 30; 22 / 8 | All 56 rectangular patterns and 30 mirrors have decoded seeds. Stored-axis prediction: 18 matches, one contradiction, seven unscored; distance-sign prediction: seven matches, 12 contradictions, seven unscored. Both remain rejected. No sketch-axis examples occur here; synthetic tests cover that path. |
| Fillets and chamfers | 1379 / 282; 294 / 17 | Input/option checks explain 1368 + 278 fillets and 290 + 17 chamfers, with zero contradictions. Of 1978 sets, 1947 have comparable framing: 1939 retain their selection, eight recover a missing selection. All 1578 previously read fillet-option sets retain their holder link; 1563 support an unambiguous option comparison, all matching. |
| Extrusion participants | 1730 / 163 | Role 8 occurs in 1485 items; 122 old lists include termination references, still only six distinct extra replay bodies. Prediction gives 331 matches, the same five resolver disagreements and 1394 unscored settled items; 35 unsettled predictions occur among candidates, 96 do not and 32 are unscored. |
| Extrusion profiles | Same 1730 / 163 | Exact curve membership gives 1041 matches, the same nine contradictions and 680 unscored settled items; unsettled membership is 60 among, one outside and 102 unscored. The proposed replacement remains refuted; no `isThinExtrude` is decoded. |
| Threads | 77 / 3 | All 80 raw field/option checks and cross-candidate parameter comparisons agree. Twenty-one nonzero phases came from history; those on whole cylinders with the axis up are now read from the seam. The legacy screening count of 31 is 29 tube and two offset alternatives. |

Full-baseline settled legacy screening counts are 106 circular, 24 rectangular,
22 mirror, 143 fillet, 23 chamfer, 100 extrusion and 31 thread items.
The conservative fillet field/raw-option guard described below extends
to 1237 settled and 217 unsettled items, with no contradictory raw options.
Reading counted participant references directly also recovers three lists
that the learning dump's diagnostic tokenizer mislabels as string data;
this changes applicability counts, not the prediction results or decoder.

The report and learning candidates carry separate `input_provenance`:
`read` requires accounting for every needed input, `inferred` records at
least one heuristic or history-derived input, and `unknown` leaves an
incomplete audit explicit. A stored-body fallback has no accepted-input
provenance. This metadata does not change candidate ordering, the `guess`
policy or the history comparison.

The initial `read` path is deliberately narrow: constant-radius G1
fillets with an explicit positive radius, tangent-chain flag, rolling-ball
corners and complete native edge references, all uniquely resolved in
the replay. Choosing the best of several possible replay edges is
`inferred`, even if the history accepts it. Splitting the same selected
edges into tangent-chain kernel operations preserves their provenance.
The field and raw-option guard holds on 703 settled and 148 unsettled
fillets, with no contradictory raw options. These are eligibility counts,
not final `read` counts: the old dump does not retain every considered
replay edge needed to establish uniqueness.

Other paths remain `unknown` unless a known inference applies. In
particular, no native extrusion in the snapshot has a decoded
`isThinExtrude`, and the profile mapper combines exact and heuristic
paths. `unknown` is not counted as read merely because rank zero worked.
Omitting dressups from a decoded mixed pattern selection, using the only
solid for an unresolved shell body, and taking a thread phase from later
history are examples of inputs reported as inferred. The corrections
above use decoded selections directly while retaining alternatives for
inputs or replay mappings that remain open.

Verification used an immutable optimized macOS build on `bb45f056` plus
the decoding batch, the original 48+78 design selection, 300-second
limits and two workers with 6 GiB each. All 126 design reports completed.
The historical Linux baseline is older, so these differences also include
intervening main changes and platform effects:

| Outcome | Main corpus before / after | Selected backup designs before / after |
| --- | ---: | ---: |
| Parametric | 1132 / 1182 | 2733 / 2832 |
| Partial | 38 / 38 | 93 / 93 |
| Fallback | 210 / 161 | 699 / 602 |
| Skipped | 261 / 260 | 705 / 703 |

Final bodies remain within 0.1% of their stored volumes, with none off,
missing or extra. Three versions of one fillet become fallback relative
to the old Linux run. A clean same-platform `bb45f056` control reproduces
the same self-intersecting-wire failure at `a9e82a9a` item 45, without
this change. In contrast, `64052a50` item 16 is fallback on that control
and parametric with the decoded rectangular inputs, matching history and
retaining the final bodies. Paired learning comparisons exclude three duplicate
(file label, item, type) keys rather than overwrite them. On items settled
in both runs, legacy screening gives the following before/after counts;
this is not a causal measurement of input decoding alone:

| Group | Paired settled items | Rank > 0 or legacy guess, before / after |
| --- | ---: | ---: |
| Circular | 83 | 72 / 72 |
| Rectangular and mirror | 36 | 36 / 36 |
| Fillet and chamfer | 997 | 75 / 74 |
| Extrusion (both input groups) | 1052 | 80 / 72 |
| Thread | 42 | 14 / 14 |

The subsequent provenance batch passes 180 import unit tests, scoped
clippy and formatting, and the C++ import test. Three targeted models
(`64052a50`, `11d7acd2`, `d03dcc34`) retain identical per-item results and
final-body reports after removing only the new provenance field: 16
accepted items are read, four inferred and 34 unknown; 47 other items
have no accepted-candidate provenance. This includes a read tangent-chain
representation, a history-selected fillet and a fillet missing options.
These are targeted counts, not extrapolated totals for the 126 designs.

After integrating the performance changes of mitcad#181, a separate six-design
run on `2c0c22b5` exercises normal import without learning's eager candidate
collection. Five retain their outcomes; `a9e82a9a` gains 21 parametric items
from previous fallbacks. All 117 final bodies match within the exact-volume
threshold; no item outcome worsens. The original frozen-run counts above
remain separate. A regression test also preserves alternative extrusion
profiles needed by a later pattern, with and without learning enabled.

Two held-out body patterns (`143d219b` item 60, `b2b33a5b` item 3) also
produce direct candidates from resolved named inputs. The existing volume
ranking can still accept a history-derived candidate first. That remaining
candidate-ordering limitation is separate from input decoding; a direct
candidate's presence does not establish that it was tried or accepted.

The learning tool keeps records with no candidates. `test --unsettled`
also evaluates `check(ex)` rules, honours the selected class version and
fails on rule errors. A prediction for an item with no candidates is
reported as unscored, rather than silently omitted or treated as a
contradiction. A candidate match on an unsettled item is a consistency
check, not a history-verified answer.

Accepted suppressed definitions (mitcad#180) retain their input provenance
and learning answer too. They stay unverified and in the unsettled dataset;
suppressed placeholders have no accepted-candidate provenance.

The final integration with mitcad#178, #180 and #181 passes 186 import
tests, 98 f3d unit tests, the f3d corpus/reference-model tests, formatting,
scoped clippy, the warning-free optimized macOS build and the C++ import
test. The geometry ctest passed after the performance merge. These scoped
checks do not replace or relabel the frozen coverage evidence above.

#### Extrusion options from one-option variants (mitcad#186)

Thirty-four reference models each differ from their siblings in one
extrusion option (sides, extent, through all, thin wall, profile selection,
direction, operation, distance). Their records were lined up byte by byte
and the findings tested as rules on the census learning dump (1730 settled
and 163 unsettled extrusions of the main corpus and the backup designs;
design versions counted separately) and on the variants' own dump (45
settled). The decoder reads them in `core/f3d` (`decode::extrude_fields`,
dump keys `_f3d.extrude.*`); the import takes them as the first candidate.

| Option | Encoding | Census (settled: explained / contradicted / n/a; unsettled) | Variants |
| --- | --- | --- | --- |
| Operation | the first `u32` after the root part: 1 join, 2 cut, 3 intersect, 4 new body. Code 3 was not read before: an intersection came in as a new body and its item was a fallback | 1730 / 0 / 0; all 163 unsettled among the candidates | explained |
| Extent kind per side | after the seven reference slots `u32 one`, then (unless one is 2, up to an object, which has none) nine bytes and `u32 two`: 1 distance, 2 up to an object, 4 through all; with the codes `extent_a` 1 one side, 2 two sides, 3 symmetric. The newer writer keeps `u8 0 \| u32 a \| u32 b` in the nine bytes (`a` 2 for a side through all, `b` 1 on two sides and symmetric extents); earlier the decoder dropped `sides` for those records | 1729 / 1 / 0; the one is a one-sided through all whose accepted answer was both sides; unsettled: 161 among the candidates, 2 not | 45 / 0 / 0; `sides` is read on every variant |
| Symmetric length | the byte `LL` of `00 LL 00 00 00` before the thin-wall block: 1 the whole length, 2 half of it each way | 14 / 0 / 1716 | 6 / 0 |
| One-sided distance direction | not a byte: the sign of the distance parameter. The negative-direction, the negative-distance and the UI-typed variants store the same direction vector and flag as the positive one; only the parameter's expression and value differ (`-( 20 mm )`, `-20 mm`, both -2 cm). The two encodings are one geometry. So a one-sided distance goes along the sketch's normal times the sign of its parameter, whatever the stored vector says | 1485 / 1 / 244; the one is a class version 6 extrusion | explained |
| Two-sided distances | each side by its parameter's sign, along the normal | 22 of 23 settled `distance/distance` went along the normal | explained |
| Through all and up to an object | the flag byte decides the side in every writer: 1 against the sketch's normal, 0 along it. The census had 76 of 77 settled one-sided through-alls with flag 1 going against the normal, and every up-to-an-object extrusion with flag 1; with flag 0 the history's accepted answer went against in 15 of 22, which the API reference dumps refute: `isPositiveDirection` is the flag byte 0 on all 78 through-all extents (see the round-2 subsection below). The through-all variants store a different vector sign for the two directions but build the same body, so the vector does not decide it | as stated | positive and negative through all both: flip false, accepted after the history |
| Thin wall | class version 8, after the side kinds: `[6 × 0] [slot] [00 LL 00 00 00] [u8 thin] [u32 location] [slot] [5 × 0] [u32 1] [ref input slot]`, found back from the first input slot. The first `slot` is the second side's parameter holder or one absent byte; the second is the wall thickness's parameter holder (a thin extrusion) or one absent byte (a solid). `thin` 0/1; `location` 0 side one, 1 centre, 2 side two; the thickness is the parameter of role `Side1WallThickness` (its holder is the second slot) | 1714 solid explained, 0 contradicted, 16 n/a (class versions 5 and 6 have no block); no thin extrusion in the census, so it cannot contradict a thin reading | all 5 explained: solid, side one 2 and 3 mm, side two, centre |
| Thin wall side | the file's side one lies inside the profile (60 × 40 × 20 mm, 2 mm: 7.68 cm³ volume), side two outside (8.32), the centre half each way (8.00). The model's `Thin.location` side one is away from the material, so the import swaps side one and side two | | volumes match |
| Profile selection | `_f3d_profile_loops`: per profile its loops are the outline of the union of the selected regions (the outer loop, holes where unselected material is enclosed); with more than one region selected, its children are the individual regions' loops (two or more are listed even where the union is one face). The curve pieces numbered by the cuts of the curves within the loop. The even-odd rule on the pieces selects the regions | 1612 of 1634 settled items with a decoded set agree with the accepted regions; of the 22 others 11 add only regions below 1 mm², 5 name curves the sketch no longer has (a stale profile), 4 miss or differ and 2 add larger regions | all six selections (the left region with and without its hole, the right region, the disc, and their unions) agree |

The refuted exact rule of the earlier audit compared curve membership
instead of the pieces; the pieces and the even-odd rule above explain the
selections, the remaining differences are tiny regions the file may or may
not select (the history cannot tell) and profiles older than the sketch's
last edit, whose records name curves the sketch no longer has.

*Read provenance.* An extrusion candidate is `read` when its profile
selection is exact (every record of its loops found in the sketch with its
piece numbered, no curve-overlap fallback), its operation code, start
support and thin-wall block are read, the participants are read (none for a
new body, else the decoded slot's bodies), and its extent comes from the
stream: a distance, symmetric (with its length byte), two-side or two-sided
through-all extent by the side kinds and the parameters' signs, a
one-sided through all with its flag byte (0 or 1), or an up-to-an-object extent
whose object the stream names and whose side comes from where the object lies
(or the flag). Alternatives stay `inferred`, every other extent `unknown`.
On the 45 extrusions of the variants (before: 5 fallbacks, the four thin
variants and an intersection, with 39 `unknown` and one `inferred`
accepted) every one comes in parametric, 42 `read`, and the three
through-all cuts without the flag byte stay `unknown` or `inferred`. A thin extrusion of a two-sided extent is not
read (its second wall is not decoded: no variant).

Not decoded: the second wall of a two-sided thin extrusion (location two
and thickness two, no variant), the
meaning of the words `a` and `b` beyond the correlations above, and the
two doubles near the end of the record (differ on every file: not geometry).

#### Threads, suppression and the timeline marker (mitcad#186)

Decoded from the controlled one-option variants (15 thread and 16
suppression models, each differing from its siblings in the option alone,
checked against the design API's dumps) and verified on the census (363
designs, 1,132 + 5,169 parametric items).

| Option | Encoding found | Census |
| --- | --- | --- |
| Thread side (external / internal) | the flag byte after the angle (1 external, 0 internal), as read before; the class (`6g`/`6H`) and diameters follow it | 78 + 3 threads agree with their decoded ThreadInfo |
| Thread kind (cosmetic / modelled) | the modelled byte after the diameters, and a list of one `(1, id)` pair before the class (`u32 n`, n × 16 bytes, `(1, id)` of another object of the file; none on a cosmetic thread, one per face on a modelled one) | as before |
| Thread designation, class, length | strings (`M10x1.5`, `6g` / `4g6g`), the full-length byte and the thread length and offset parameters (`10 mm` / `3 mm`; a full thread keeps the default `10 mm` and `0 mm`) | all agree |
| Thread handedness | one byte after the object's two references following the pitch diameter (1 right-handed, 0 left-handed; the hole reference, a null byte for a thread item, comes before them): the left-handed variants of a cosmetic and a modelled thread differ from their right-handed ones in this byte (and the list) alone | 81 threads (versions 4 and 5) have 1, none 0; the census has no left-handed thread, so the rule cannot be contradicted. Version 1 and 2 layouts are not read. Implemented: `ThreadInfo.isRightHanded`, read first by the import; a thread that is not modelled, whose faces the decoder named, whose handedness is read and whose class MitCad lists has `input_provenance` `read` (a modelled thread keeps `inferred` for its start angle) |
| Thread start angle (phase) | not found: no field of the thread's object, its sub-objects or the body object its list names holds it (checked for the angle in degrees and radians on the 20 census threads with a nonzero phase); the variants all start at 0. It stays inferred from the history | n/a |
| Suppressed itself or through a dependent | `000100` or `010000`, see [*Items suppressed in the file*](f3d-fallbacks.md#items-suppressed-in-the-file) | 24 of 27 designs consistent, 3 (one design) not explained; 32 of 39 `010000` fillets have a face no state holds, 0 of 9 `000100` |
| Suppression by API, timeline menu, configuration, group | the same bytes (the hole's object in the API, UI and configuration variants differs in ids and a random value only) | n/a |
| Configuration table | `DesignConfigurationTable.<id>.dsgcfg` is JSON: columns (a part number property and one per configured object: `configuredObject` with the feature's object id and the title, a boolean column for a suppression), rows (`Configuration n`, a cell per column) and the table view's order; the active row's id is a string in the design stream, in the object named `DesignConfigurationRoot` (two models that differ only in the active row differ in this string and in the item bytes of what the row changes). The active row's effect is in the items' own bytes | no census file has a configuration table, so the import does not read it |
| Rolled back | the second of the two bytes in front of an item's last reference, see *After the timeline marker* | 52 items, 15 of the 16 designs have them as the end of the timeline |

#### Patterns and mirrors: the data object's settings (mitcad#186)

The audit above found no contrasting examples for a pattern's compute
option, symmetry, distance type and suppressed elements. One-option variant
models (a design per value of each option, made from scripts; the flip of a
rectangular pattern also by hand) show them in the bytes of the pattern's
data object, the item's first reference after its root part
(`core/f3d-design/src/build/patterns.rs` has the layout; the decoder gives
the dump IR's `isSymmetric`, `isSymmetricInDirectionOne/Two`,
`patternComputeOption`, `patternDistanceType`, `suppressedElementsIds` and
a mirror's `isCombine`):

| Option | Encoding (found by the bytes that differ between the siblings) |
| --- | --- |
| Compute option | `u32 compute \| u32 kind \| i32 -1` ends the head of the circular, rectangular and mirror data objects: 0 optimized, 1 identical, 2 adjust; `kind` 0 faces, 1 features, 2 bodies. Four or eight bytes lie before it (after the `u32 n \| n` body records); the oldest layout has neither them nor the `-1` |
| Circular pattern symmetric | the byte after the object's root part, 1 for a symmetric pattern |
| Mirror joined | the same byte of a mirror's data object: `isCombine`; always 0 for a mirror of features (the old reading, no new body, gave 1 for 12 of the corpora's feature mirrors) |
| Total angle (360, 180, 90) | no byte: the angle is the parameter `TotalAngle` and the item's float, as before |
| Rectangular distance type | the byte after the `-1`: 0 spacing, 1 extent |
| Rectangular symmetric | one byte each for direction one and two, right after the two distances' parameter holders |
| One or two directions | one or two direction inputs, a null byte instead of the second. A one-direction pattern keeps the second direction's quantity and distance (defaults), as the API reports them |
| Axis kind | the direction input names the entity: an origin axis, an edge, or a sketch line (whose entity input has no target object: decoded from the sketch curve's tags, new in this change) |
| Flip | no flag, and the stored vector does not change: the sign of the distance parameter (`-20 mm` in the API's and in the UI's pattern) |
| Suppressed elements | `u32 s \| s` `u32` at the object's end. A circular pattern's numbers count the original as 1: the API's id 3 is stored as 4 (one variant). A rectangular pattern's element numbers follow no order the variants explain, so only an empty list is read |
| Objects (features, bodies, faces, with fillets or chamfers) and mirror planes | already decoded; the variants confirm the object type, the plane kind (origin, construction, face) and the object list |

The first four rows hold in the corpora too: the layout parses in all 259
circular patterns, rectangular patterns and mirrors of both private corpora
(a decoder run over every file); `kind` agrees with the decoder's own reading
of the objects in 137 of 137 circular patterns, 54 of 54 rectangular patterns
and 30 of 30 mirrors of the census learning dump, and a rectangular pattern's
second direction agrees with the decoded directions in 54 of 54. Settled and
unsettled items, learning dump (before the import used the settings):

| Rule | Explained | Contradicted | Not comparable |
| --- | ---: | ---: | ---: |
| Circular symmetric flag = accepted answer's `symmetric` | 110 | 2 (one design, two versions, 180 degrees: run again, the import accepts `symmetric` true there, so the old answer was ambiguous) | 24 (22 unsettled, one with the flag set, which is a design the import now settles; 2 unread by the tool) |
| Circular compute (0 / 1 / 2) | the code is never another in 112 settled and 22 unsettled: the accepted answer is the default (`adjust`), so the history cannot test it; 15 of the 22 unsettled have a non-default code against 17 of the 112 settled | | |
| Rectangular distance type = accepted `distance_type` | 25 | 1 (extent with four elements; the answer was a spacing) | 0 |
| Rectangular symmetric, second direction | 3 unsettled have the symmetry set and 3 a second direction; none of the 26 settled | | |
| Rectangular extent | 17 of the 28 unsettled are extent, against 2 of 26 settled (one with two elements, where extent and spacing agree) | | |
| Mirror joined flag = the old reading (no new body) | 14 of 14 mirrors of bodies, 4 of 16 mirrors of features | 12 of 16 mirrors of features (the flag is 0, the old reading 1) | |
| Suppressed list | empty in every one of the 134 circular and 54 rectangular patterns: only the variant gives a non-empty one | | |

Against the original application's own dumps of the real designs (the
API reference dumps; `mitcad-f3d-inspect design --json` joined by
design and timeline index), every decoded setting agrees and none is
contradicted: 90 circular patterns (symmetric 3 true / 87 false; compute
78 adjust / 9 identical / 3 optimized; suppressed lists all empty), 22
rectangular patterns (distance type 14 spacing / 8 extent; symmetric in
direction one 1 true / 21 false, in direction two 2 false; compute 14 adjust /
6 identical / 2 optimized; suppressed lists all empty) and 13 mirrors (joined
6 true / 7 false; compute 11 adjust / 2 identical). The compute option of a
mirror, which the variants cannot show, is therefore read from the layout it
shares with the patterns on real data too. The same dumps confirm the
flip: in all 30 rectangular patterns the decoded signed distance equals the
API's `distanceOne` and the stored vector equals its `directionOne` (never
flipped by the sign), a second direction is present exactly where the API has
one (2 of 30), and the entity kinds agree (2 construction axes, 27 edges; one
edge not resolved by the decoder).

The rules the audit found rejected were not rules of a stored flag: the
distance's sign *is* the flip, so "the axis flipped by the distance sign"
(10 or 12 contradictions) counts the flip twice, and the stored vector is the
entity's direction (an axis' or an edge's) without it. Of the 26 settled
rectangular patterns, 19 answers with an origin axis or a stored line have
that axis equal to the stored vector (the signed distance flips them); the
one that does not is a pattern whose edge direction stored as -x was accepted
as +x with a negative distance (history-ambiguous or a reversed stored edge:
undecided). A direction named by an edge, face, sketch line or construction
axis is therefore also tried by its stored vector after the entity (an edge's
direction in the replay need not point where the file's does).

A symmetric pattern does not mean what the model assumed: the variants show
that a symmetric rectangular pattern of three elements puts them at -1, 0 and
+1 and a symmetric circular one of four over 90 degrees at -60, -30, 0 and
+30 degrees (the same spacing, the quantity in total, the extra element of an
even quantity before the original). The model's `symmetric` now means that
(it was the quantity on each side, 2q - 1 elements: an assumption of the
study, K41, never checked); the guesses for patterns without stored
directions follow.

The import tries a pattern or mirror whose objects, axis or plane, counts
and settings are all read first, with `read` provenance (not the dressup
variant, which stays after the history's suggestions); the guesses follow.
A rectangular pattern whose direction names an edge, face, sketch line or
construction axis gets a second definition with the stored vectors as fixed
directions (read too), after the first.

On the 35 pattern and mirror items of this group's variant models: before,
21 were accepted as inferred and 11 as unknown (none read), two were
fallback and one skipped; after, all 35 are parametric with `read`
provenance (the two with fillets or chamfers repeated on the copies at rank
2, the rest at rank 0). Of the 14 census designs whose patterns or mirrors
were unsettled with a non-default option (the unsettled items with the
extent distance type, the symmetry flag or a second direction set, and a
symmetric circular pattern) and that were imported again, 31 pattern and
mirror items are comparable with the learning dump: 24 were unsettled
(fallback or skipped) before and 8 are after (one design's import stopped:
its debug build overflows in the solver's analysis, as before this change);
the 16 that came in are 15 rectangular patterns and the circular one, and
of the 23 settled in all, 20 are read and 3 inferred. The ones that stay unsettled
are four mirrors (other causes) and four rectangular patterns (a copy
reaches no body in two, the history does not hold the result in two; one
of them keeps defaults of a second direction without an entity).

### Primitives, coils and option constants (mitcad#25)

Controlled one-option variants (`MITCAD_F3D_MODELS`: 19 primitives, 13 coils,
and chamfer, hole, loft, sweep, pipe, thin-wall, through-all and font
variants) of the newer writer (2705; its class version 9 parameters carry
one more `u32 0`, without which no parameter of such a file was read).
Every field below was read against the dump of the same model.

| Group | Bytes | Variants / real designs |
| --- | --- | --- |
| Box, cylinder, sphere, torus: operation | `u32 operation` (1 join, 2 cut, 3 intersect, 4 new body, 5 new component), `u8 1`, then a run of references (holders, the frame input, a definition) | 19 variants; 8 primitives of one design |
| Their placement | after the run `u8 flag`: 1 the identity, 0 and a row-major 4x4 matrix in cm. The frame input (`7B6C3C2D`) holds the matrix of the plane's or face's frame *with its origin* (none: the XY plane). A point of the primitive's own frame is at `frame * placement * p`; a box's width (`DistanceX`) is along x, its length (`DistanceY`) along y, its height along z | box on the XY plane, on an offset plane, on the XZ plane and on a face at another point; sizes; join and cut; cylinders and spheres likewise; all verified against the stored bodies (the bounding boxes of the dumps' bodies) |
| Torus position | `u32` after the placement: 0 on the centre line, 1 outside, 2 inside | 4 variants |
| Coil | `u8 flip` (1: left-handed), `u32 operation`, `u32 type` (1 revolutions and height, 2 revolutions and pitch, 3 height and pitch, 4 spiral), then the holders and the frame input, the placement, then `u32 position` (1 inside, 2 on centre, 3 outside) and, after one more reference, `u32 section` (1 circular, 2 square, 3 triangular outside, 4 triangular inside) | 13 variants; 1 real design (4 copies): all read |
| Chamfer set | tangent chain and flip bytes (already read) agree with the dumps on the six variants; the flip means which face takes the first distance, the face of the coedge that runs the edge's way, which the replay's topology has, so the import still tries both readings | 6 variants |
| Hole direction | the first null reference behind the header, then `u32 type` (0 simple, 1 counterbore, 2 countersink) at +9 and `u32 flipped` at +13 (class version 7 records) | 39 holes of the dumps: all agree; none of the 17 holes of older writers is decoded |
| Loft | after the rails `u8 isClosed`, `u32 isSolid`, `u8` | 9 variants; 14 real lofts all open and solid |
| Sweep | `u32 orientation` after the ten slots: 1 parallel, 2 perpendicular | 8 sweeps with dumps; 18 real sweeps perpendicular |
| Pipe | `u8 isHollow` before `u32 operation`; `u32` after the first input slot: 0 circular, 1 square, 2 triangular | 5 variants; 2 real pipes circular |
| Thin extrusion, two sides | `u8 1 | u32 location | holder | u32 location two | holder two | u32 1 | ref` | 6 variants |
| Through all, newer writer | the flag byte of the older writers is 1 exactly where `isPositiveDirection` is false; the direction given to `setOneSideExtent` changes nothing | 4 variants |
| Text style | the second flag byte after the horizontal alignment: 1 italic (bit 2) | 4 variants |

What the variants showed about geometry: the stored coil's section lies in
the plane through the axis (its size is the circle's diameter, the square's
side, the triangle's height; the radial extent is placed on the diameter),
which MitCad's coil now builds by the helix's screw motion; a pipe's square
size is its side and a triangle's its height with the path at its middle.

What the loft, sweep and font variants showed, and the kernel work that
followed (each checked against the stored bodies, import report
*verified*):

- *Lofts through three or more sections without end conditions or rails*
  (open or closed) are piecewise cubic Hermite surfaces: each point of a
  section follows a cubic piece through its matches in the sections, whose
  tangent at a section is the derivative of the parabola through it and
  its neighbours, the first and last pieces being the parabolas that
  arrive with the inner tangents; a closed loft is the periodic form.
  The stored surfaces' centre lines and volumes agree to 7 digits (an open
  ring of four circles 9503.318 mm3; closed 12880.53, which the kernel
  built as no solid before the closing curve and the corner matching were
  right; the old C2 interpolation gave a ring 12 % fuller and an open one
  17 % too large). The matching of the sections' corners takes a section
  turned a quarter round the axis a corner off, so each section is turned
  to lie nearest to the one before it.
- *Surface lofts* (`isSolid` false) are sheet bodies of the side faces
  (`surface`), a new body; the volume of an open shell is 0.
- *Direction end conditions*: the angle is measured along the loft, so at
  the last section a positive angle arrives moving outward (the takeoff
  into the loft tilts inward); the end section's loft was 49 % too large
  with the other sign and is 1.6 % small now, the start's 1.2 % large.
  What stays: the stored takeoff fields (the derivative across the
  section, row by row of the circle's polynomial form) carry a fourfold
  ripple (the free end's 8 % down at the diagonals, its direction wobbling
  by 2.6 degrees; the direction end's 1.5 %), a property of the stored
  fit that no rule of the section's geometry gives, so these lofts stay
  fallbacks of the history (0.5 % accepted).
- *A tapered sweep* leans each side of the profile by the angle (the
  outline offset by the distance times the tangent; a square stays a
  square, a circle is a cone), as an extrusion's taper does; MitCad scaled
  the whole profile (10650.14 mm3 for the reference model, exact now).
- *Sketch text height*: the file's capital height is that of the glyph A
  (the apex is flat in a sans-serif face but reaches the roman face's
  overshoot, 1387 against 1356 units of 2048 for a roman face's H), which
  explains the 0.9776 scale of one roman face (exact for the sans-serif
  ones); all three bodies of the reference text agree to 1e-6 now.

Not translated, with the cause found: a chamfer without its tangent chain
where a round continues the edge (the kernel always follows chains), and
four primitives that stand on faces of a component inserted from another
design.

### Twin edges and a thread's start angle (mitcad#106, mitcad#186)

Two groups the corpus alone could not teach were read from controlled
one-option variant models (a shaft with a milled flat, a block with a bore
breaking through a face, each with the fillet or chamfer on either mirror-image
edge or both; a revolved shaft with its seam turned 0, 45 and 90 degrees, a
thread with offsets 0, 1 and 2.5 mm) and checked on the census.

- **Twin edges.** Edges between the same two faces with the same faces at their
  ends fit the same recipe names. The recipe's tail lists the faces round each
  end vertex counter-clockwise seen from outside the body
  ([`core/f3d/EDGE_ORDER.md`](../../f3d/EDGE_ORDER.md): layout, proof). The
  decoder writes them as `_f3d.end_faces` and takes, among edges that fit
  equally, those whose faces are in that order (`NamedState::edges_fitting_ordered`),
  or one of edges at the same place; the edge then has its geometry in the
  dump and its input is `read`, not chosen by the history. The layout holds
  for all 1,559 distinct tails of the corpora
  (`tools/f3d-learn/rules/edge_tail_grammar.py`: 1,932 fillet and chamfer items
  explained, none contradicted, unsettled ones included). Of 157 edge inputs
  in the 538 designs that did not fit one edge, 29 now do; no edge found
  before changed, and the 13 new edges in the 321 reference designs that the reference
  dumps list are the selected edges, not their mirror images. The other 128
  remain: end faces the recipe names as an earlier state had them, closed
  edges, vertex fans that repeat a face, patterned copies with the same order,
  and the n inputs of one item with the same names (each would choose alone and
  two could take the same edge; they take the n edges that fit in order, as
  before).
- **A thread's start angle.** The variants differ in the seam of the threaded
  cylinder only, and the angle found in later states follows it (0, 45, 90
  degrees) while the offset (0, 1, 2.5 mm), length and location change
  nothing. The file's helix starts at the seam of the cylinder's surface at
  the end of the face the file's axis starts from. The decoder gives a whole
  cylinder's seam direction (`_f3d.seam`); with the file's axis pointing up
  (MitCad's helix starts at the low end of the axis pointing up) the import's
  angle is the seam's direction about the axis from the world X (Y) reference
  (`thread_phase`), and the measurement in later states is skipped. Of the
  census' 20 modelled threads of this kind 18 agree exactly and two within
  2.3 degrees; with a right-handed thread only (a left-handed one, a part of a
  cylinder and an axis pointing down go on being measured). For an axis
  pointing down the file's helix starts at the other end; the angle there
  follows the face's length and the pitch by a formula that agrees with the
  census threads of this kind whose length is a whole number of turns
  (a half-turn from the seam) but not with the two that are not (the
  face is cut obliquely there), so it is not adopted. The learning rule
  `tools/f3d-learn/rules/thread_seam_phase.py` checks a dump made with the
  decoder that writes the seam. A modelled thread whose angle was read, whose
  faces were named and whose handedness was read has `read` provenance.

### Stored edge curves (mitcad#306)

An edge input (`5662F619`) refers, besides its recipe, to the curve of the
edge as it was when the input was made: after the reference's two GUIDs a
list `u32 n | n refs` of curve objects (classes deriving from `518AD6D6`),
in the component's coordinates, cm and rad
(`core/f3d-decode/src/curve.rs`). After the root part:

| Class | Layout |
|---|---|
| `50AEE8B4` line | `f64[3] start, f64[3] vector` (start to end) |
| `B0B3FA59` arc | `f64[3] centre, f64[3] axis, f64[3] reference direction, f64 radius, f64 start, f64 end` (angles from the reference direction towards axis × reference) |
| `98E8921B` elliptical arc | `f64[3] centre, f64[3] major direction, f64[3] minor direction, f64 major radius, f64 minor radius, f64 start, f64 end` (the parameter t of centre + major cos t · major direction + minor sin t · minor direction) |
| `2319971D` spline | `u8 a, u8 b, u32 degree, f64, knots, weights, poles, f64, u8, u8, f64 start, f64 end` (the edge's parameter range); knots and weights `u32 n, u32 n, u32 8, n × f64` (no weights: 0), poles `u32 n, u32 n, u32 8, n × f64[3]`; `a`, `b` and the two `f64` (small tolerances) are not read |

Every curve ends with its base classes' `u8 0, ref, u8 0, u8 1, ref`, both
references null (one byte 0) in edge inputs (a sketch profile's curves refer
to the profile there). On the reference models (`MITCAD_F3D_MODELS`) the
117 curves the fillet and chamfer edge inputs store are all edges their
sets list in the reference dumps, ends and middle
(`stored_edge_curves_match_the_reference_dumps` in
`core/f3d/tests/design_models.rs`). On the corpora (`MITCAD_F3D_CORPUS`, the backup
corpus and `MITCAD_F3D_MODELS`, 856 distinct designs) the edge inputs store
15,636 curves, all of which decode (7,145 lines, 6,527 arcs, 459 elliptical
arcs, 1,505 splines); 57 edge inputs store none, and 139 store two or three
(an edge the input saw in pieces, each piece a curve: the stored curves of
one spline then have ranges far apart on one parameter).

The decoder writes the curves as `_f3d.curves` (`Line3D`, `Arc3D`,
`EllipticalArc3D` with `normal` = major × minor direction, `NurbsCurve3D`)
and uses them in two ways:

- **Check.** The edge found is compared with the curves (mm, within 0.01 mm)
  and `_f3d.curve_fit` says how it lies: `the stored edge` (its ends are the
  curve's), `a piece of the stored edge` (on it, within its ends: a later
  item cut it), `on the stored curve, … beyond its ends`, or `off the stored
  curve by …` (with `(curve k of n)` for an input of several). Of the 13,952
  edge inputs found in the corpora 13,763 lie on their curves within their
  ends (13,710 the stored edge, a closed edge on its closed curve wherever
  its seam lies; 53 a piece of it), 45 on them beyond their
  ends and 144 off them: 68 for items suppressed in the file (the design
  changed after the item was suppressed: mostly the same edge, moved by 1 or
  10 mm), 54 found a few states before their item (the edge moved since),
  22 in the state before their item (splines whose names fit an edge
  4 to 40 mm away, and elliptical arcs 0.1 to 0.4 mm off; not settled).
  The check does not reject an edge on its own: an edge that moved is still
  the one the names give.
- **Decide.** Where the names fit several edges, the edge that is the stored
  edge, else the only piece of it, is the input's (`found` ends with `(by
  its stored curve)`), before the recipe's tail, the n inputs of the same
  names and the inputs of a set that fit alike: those take the edges in list
  order, and on the corpora 52 inputs had taken another of the edges (most
  the mirror image or the next copy of a pattern of the edge they name); 6
  inputs the names left open are now found. The affected designs imported
  with the decoder's dumps before and after: three items whose edges the
  history had to settle (`inferred`) are `read`, no other outcome and no
  body or history check changed.
- **Several edges** (mitcad#106). An input stores a curve per edge it stood
  for; where two or more edges its names fit lie on its curves within
  their ends (on one body; edges on one curve only as its pieces, together
  no longer than it), a fillet's or chamfer's input is all of them: the
  first is the input's, the others edge inputs of their own after it in its
  set (`found` ends with `(one of the edges its stored curves give)`; an
  edge the set has already is not added again). This comes before the one
  edge above. On the corpora it found the 86 inputs the names and curves
  had left ambiguous (none is left; `core/f3d/EDGE_ORDER.md`, *The curves
  an input stores*), the sets taking 139 edges more; 30 inputs of rings and
  of the same names that had taken the set's edges one each in list order
  now take those their own curves give (the sets' edges are the same). An
  input still ambiguous says how each edge that fits lies to its curves.
  Eleven affected designs imported with the decoder's dumps before and
  after: nine items whose edges the history had to settle (`inferred`)
  are `read`, no other outcome and no history check changed.
- **Inputs that found one edge.** Inputs of one fillet or chamfer with other
  names and other stored curves that found the same edge cannot both have
  found theirs. Those whose curve the edge does not lie within (it runs
  beyond its ends, or lies off it) are left not found with the reason; an
  input whose stored edge was longer than the edge now is (mitcad#305) keeps
  it, and two inputs of the same stored curve (two names of one edge) keep
  theirs. On the corpora 9 inputs are left out this way: 7 arcs of a
  pattern's copies whose names fit one edge 11 states earlier (1 mm off all
  of them), and two inputs whose names fit another input's edge, 28 and
  40 mm from their own curves. An item suppressed in the file keeps such
  edges: it is looked for in the state after the item before it, which the
  file never had, and there pieces of one edge its inputs saw can be one
  edge (on the corpora two or three pieces of one line or elliptical arc,
  each input taking the whole edge, in five items of five designs, versions
  of three; leaving them out took the definitions of such items away).

The inspector's `edge-curves` command lists the curve classes of a file, each
edge input with its fit, and the inputs of other names that found one edge.

### Where a circle starts (its seam)

A sketch circle record always names the sketch's x axis as its reference
direction, but the closed edge the file's bodies hold for it starts
elsewhere: on the x axis, on the opposite side or at another angle, with no
rule of the sketch's frame, the extrusion's direction or its operation (in
the designs checked the opposite side and the x axis split about 60 to 40;
every edge's parameter runs from -pi, so the start vertex is opposite the
curve's major axis). The replay started every circle on the x axis, so the
cylinder's seam edge and the vertices at it were on the other side: a plane
through three seam vertices, a tangent plane at one, a seam edge in a
fillet's set and the items after such an item failed.

The decoder reads the start from the bodies (`_f3d_seam`, SCHEMA.md): for
each whole sketch circle an extrusion uses, the closed edges of the state the
first such extrusion made are searched for one on the circle's axis and in a
plane across the sketch's normal with its radius; at the sketch's plane
first, else at the other end of the extrusion if the ends agree; its start
vertex's angle from the sketch's x axis is the circle's start. The model's
sketch circle has `start_angle` (radians, default 0, so MitCad's own
sketches are unchanged), carried into the profile (`SegmentGeometry::Circle`
`start`), the path curves of sweeps and the kernel's circle (the axes of the
circle are turned by it, so the one edge, the cylinder's seam and its
vertices are there). Of the 5722 sketch circles of the 170 designs that had
problems in the census, 3508 are found in the bodies (3146 start elsewhere
than the x axis, in 128 designs); the rest are circles no extrusion uses or
that the bodies hold only split. An extrusion up to a surface that the
sweep crosses twice (a cylinder entered from outside) now ends at its first
crossing.

Run again on the 125 of those 128 designs that finished both runs (one run
with the seams read, one without, the same build): 3 items went from
fallback or partial to parametric (two dressups whose edge sets name a
seam edge, a plane through seam vertices), 3 got worse where the
geometry kernel is sensitive to where the seam lies (a chamfer that fails,
a modelled thread whose volume is 0.7 % off, and a fillet after an item
that now reaches the kernel and fails there instead of being skipped
unchecked); parametric items read from the file rose by 2.

### Construction datums, sketch constraints and dimension classes (mitcad#186)

Read from one-option variant models (`diff_cplane_*`, `diff_caxis_*`,
`diff_conline_*`, `diff_constraint_*`, `diff_dim_*`, plus the reference
models `cons_axes`, `cons_points`, `sk_text`, `sk_spline_control`,
`con_smooth`), each with the API dump of the original application as ground
truth, and checked on the real designs against their API dumps (`f3d-api-ref`).
The learning dump holds no datum items, so there are no `learn.py` rules; the
checks are the reference-model tests in `core/f3d/tests/design_models.rs`
(`construction_definitions_match_the_reference_dumps`: 123 definitions agree,
none differ or are left undecoded; `sketch_dimension_and_constraint_types_
match_the_reference_dumps`: 1212 sketch lists agree) and counts against the real
designs' API dumps, below.

| Option | Where it is stored | Verified |
| --- | --- | --- |
| Plane definition | the `u32` after a one byte flag behind the item's root part (`_f3d.datum_code`): 0 offset (also parallel through a point), 1 angle, 2 three points, 3 mid plane, 5 and 10 tangent, 6 along a path, 7 two edges, 9 tangent at a point. Items made by older writers store a tangent plane as 1 or 5; the face as the line of an angle makes it a tangent plane | codes against definitions in the real designs: 164 planes, every code one kind (1 and 5 tangent or angle) |
| Vertex input | a `FACE_REF` input whose recipe is `vertex_recipe_data`: the faces that meet there, one entity each; the position is found in the history state at the item's place (`NamedState::find_vertex`) | 3 vertices of the three-point plane models and the point models: all positions as the API's |
| Input wrapper | class `2CA5A1CD` before each wrapped input, the role of the input after its slot key object (not read) | skipped in the item's inputs |
| Point definition | code 0 at a vertex or point (one input), 3 at the centre of a circle (an edge), 2 where three planes meet; its position from the key point input | `cons_points`, the vertex variants, the corpus' one point |
| Axis definition | no code: by the inputs: an edge or a sketch line, a face (round: through it; planar, with no other input: perpendicular to it at the axis' own origin), a face and a point (normal to it at the point), two points, two planes | `cons_axes`, ten `diff_caxis_*`, 4 of 8 axes of real designs (the others: normal to a sketch profile at a point) |
| Constraint bits | `0x200` smooth; bit 31 the control polygon of a control point spline; bit 40 the box of a sketch text; bit 41 a text and a curve (unverified). The last three constrain no geometry (the API lists none) and leave the sketch whole | `con_smooth`, `sk_spline_control`, `sk_text`; real designs' sketches: 1259 agree with the API's constraints by type |
| Rectangular pattern constraint of newer writers | the same record, the sketch reference over 200 bytes into it | 2 constraints, values as the API's |
| Line kind | construction and centre line flags, read already | 36 `diff_conline_*` and `diff_constraint_*` models: every type on normal, construction and centre lines decodes, with the flags |
| Dimension classes | ellipse radii: the one set byte of the last eight flag bytes, 1 or 4 major and 2 or 8 minor; class `A9E23439` the distance of a line from a circle or arc; a point or line with a reference to a face or plane outside the sketch | all `diff_dim_*` models; real designs: 1261 sketches' dimension types agree with the API's (two sketches of one design differ by index) |

Not decoded: the point of a perpendicular axis is not an input, only the
axis' origin; four axes normal to a sketch profile at a point; the face or
plane of a dimension outside the sketch (an `error` reference). MitCad has
no line-to-circle or point-to-surface dimension, so those dimensions are left
out of the sketch (`partial`). A smooth join with a fitted spline cannot be
kept either (the spline's control points are fixed, not shared with the
arc's end). A vertex at a circle's seam is found now that the replay's circles start where the file's do (see "Where a circle starts" below), so a tangent plane at such a vertex and a plane through three of them are parametric.

The import takes a definition's first reading as `read` where the file names
every entity outright, the others as `inferred`, and keeps a datum only
where it lies where the file has it (planes: normal and distance; points;
axes: line and direction, the reverse direction as a second candidate).
A planar face is its whole plane for the point of an axis perpendicular to
it (`commands.md`), so that a point off the face projects as the original
does. New plane definition `offset_through_point`.

On the 27 designs of both private corpora that hold the planes left fixed,
the axes and the one point (a targeted run, not a coverage run): of 33 fixed
planes 9 are now parametric and read, of 8 fixed axes 4 (inferred: the stored
direction is the reverse of MitCad's rule) and the point (read); 40 planes
of 93 datum items are `read` (none was before). What stays fixed: items with no ASM state to
find a vertex in, faces the replay cannot name, planes along several sketch
curves, a mid plane and an offset whose inputs the file does not name, and
four axes normal to a sketch profile.

#### Against the API reference dumps

The reference dumps hold the original application's own answer for 321 of
the corpus designs (5,224 timeline items; the per-step walk, so dressup
edges and every input are as the application read them). The decoder's dump
IR has the schema of the dumps, so `tools/f3d-learn/apiref.py` joins the
two per timeline item (a global alignment on type, name and index: 5,184
items paired, 40 only in the reference, 64 only in the decoder's dump,
mostly items it names by their class) and the learning dump's accepted
answer by item index. It counts each field of each input group once per
item or sketch element as agree, differs, decoder missing or reference
missing, and the accepted answer against the reference in the same way;
entities compare by what they are (faces and edges by geometry, sketch
curves by their geometry in the sketch, features by name), numbers with a
tolerance of 1e-4 cm. Output: a table per group, and per field the
mismatches with the design, item, the decoder's or the accepted value and the
reference's.

```bash
python3 tools/f3d-learn/apiref.py compare --api DUMPS --ir IR --census LEARN \
    --index DUMPS/index.tsv --root CORPUS_ROOT --out OUT
```

Decoder against the reference (items or elements; measured on the build with
the one-option decoding of extrusions, patterns, threads and datums):

| Group | Reference values | Decoder agrees | Remaining |
| --- | ---: | ---: | --- |
| Timeline items | 5,184 | names 4,735, types 5,082 | Names: 449 differ (368 occurrences, whose names carry the component and a counter; 43 sketches, 34 of them renamed by the user under a name the file keeps outside the item; 20 joints, as-built joints and rigid groups and 14 constraint sets, which the application numbers by kind). 165 construction planes, axes and joint origins were shown under the file's internal names and now agree. Suppression (80 items) and the rolled-back marker (43 items) 5,184 of 5,184. Health (12 errors, 52 warnings) is not stored in the items |
| Extrusions | 1,281 | operation 1,281, extent type 1,281 (symmetric included), distances 1,148 (their sign is the side), taper 1,281, direction 1,219, extent kinds (distance, through all, symmetric, up to an object) 1,281 and 32 second sides, through-all direction 78 of 78 (and 4 of 4 second sides), symmetric length 7 of 7, `hasTwoExtents` 1,281, start kind 1,281 and the profile plane 1,188 of 1,192, up to an object: chain 55 of 55, nearest solution 55 of 55, direction hint 55 of 55 (and 2 of 2 second sides) | up to an object: the face 52 of 55 and 2 of 2 second sides (the three others are one design whose faces the name lookup resolves to other faces); no thin extrusion occurs in the corpus (the thin flag is read on 1,265 of 1,281, the 16 others are class versions without the block); the accepted answers: through-all side 62 of 68 |
| Extrusion profiles | 2,623 regions | exactly 2,114, held by a decoder region with extra curves 295, by one outline 2 and in another loop structure 1 | 211 missing, 149 extra regions: 134 and 95 in 37 items whose stored loops name curves the sketch no longer has (822 of 27,589 records), 77 and 54 in 33 items whose records all resolve (the application splits and merges loops differently; see the round-2 subsection); 211 profiles name no curves in the reference; the participants the operation visibly changed are among the decoder's in 913 of 968 |
| Revolutions | 101 | operation, angle 101, axis 101 (the accepted answers 71 of 81), solid 101, symmetric 101, project axis 94, participants 34 of 39 | project axis of the 7 class version 1 revolutions (no byte); regions 63 exact, 7 held, 10 missing |
| Circular patterns | 105 | axis 101 of 104, quantity, angle, compute option (87 adjust, 10 identical, 8 optimized) and symmetry (3) 104 of 105 | the accepted answers say adjust and not symmetric: compute agrees 73 of 84, symmetry 82 of 84 |
| Rectangular patterns, mirrors | 39; 17 | direction, quantity, spacing, compute option, distance type, symmetry 31 of 39; mirror plane, compute option 15 of 17, `isCombine` 15 of 15 | 8 patterns of occurrences carry no inputs; the accepted answers: rectangular compute 7 of 12 |
| Fillets | 1,158 | edge sets 1,158, edges 1,026 (the reference lists no edge for 46 sets), options: tangent chain, continuity and corner flag 1,158 | 86 edge lists differ: 43 other edges (27 are equally long edges elsewhere on a symmetric or patterned part, which the names and the recipe's tail do not settle), 21 whose names fit several edges equally or none, 11 in items whose references the application reports lost and 11 in items rolled back after the timeline marker, for which the file keeps no state (*Fillet and chamfer edges against the reference*) |
| Chamfers | 193 | edge sets 193, edges 186 (the reference lists none for 2), sizes in the sets | 5 edge lists differ; tangent-chain and flipped options are not decoded (the reference has none false); the accepted answers' edges differ in 6 of 176 sets, a two-distance chamfer's side in 11 of 46 |
| Holes, threads | 10; 77 | threads: every decoded field of 77 (designation, class, diameters, flags, length, offset, location) | holes: extent (all 10 through all), position (face, both offsets, edges 7 of 7 and 7 of 7) agree; handedness 75 of 77 and 2 of 10 holes (the class version 1 thread object has no handedness byte; all are right-handed); no direction, default direction, taper or hub fields (constants in all 10 holes and 77 threads) |
| Shells, combines | 4; 33 | operation, keep-tools 33, thickness 4; shells: tangent chain, type, faces 4 of 4; combines: target and tools 25 of 31 | combines: 6 of 31 whose bodies have no producer in the decoder or share a name with another body; `isNewComponent` (false in all 33) has no contrasting byte |
| Construction planes, axes | 123; 8 | geometry 123 and 8, definition type 123 and 8 of 8, offsets, angles, distances, line inputs 8 of 8, path inputs 21 of 21, plane faces 42 of 48, profile inputs, point inputs of the axes 4 of 4 | 6 plane faces, the vertices of 5 three-point planes and 2 tangent faces are not found by name in the history (ambiguous or stale names; the same designs' faces); `isParametric` (true in all) is not stored per item |
| Sketch curves | 29,762 | geometry of every curve (4,394 of 4,394 circles); type 29,762 (the 108 control-point and fitted splines included); construction 29,762, centre-line 16,237, fixed (717 set) and linked (18,577 set) flags 29,762 of 29,762 each | none left: see [Sketch curve flags, spline kinds and sketches without a link](#sketch-curve-flags-spline-kinds-and-sketches-without-a-link-mitcad186) |
| Sketch points, constraints, dimensions | 39,658; 9,919; 3,867 | points 39,622; constraints 9,888; dimensions 3,859 | 31 constraints (23 of them rectangular-pattern constraints); 36 points, the corners of text boxes; driving flag 3,862 of 3,862 (63 offset dimensions without a parameter are driving, not driven) |
| Parameters | 12,332 | 10,509 by name; expressions 10,372, values 10,383, units 10,457, roles 10,174 | half of the 1,823 missing are in five assembly designs |

The accepted answers (settled items only) mostly agree where they carry a
field: extrusion operation and extent 1,116, distances 995, regions by area
1,038 of 1,066; circular axis 74 of 74 comparable, quantity and angle 84 of
84, revolution operation and angle 81 of 81, axis 71 of 75. Where the history
cannot tell, the guess is wrong-but-accepted: compute option 18 of 108
patterns and mirrors, a through-all extrusion's side 6 of 68; the edge list
of 63 of 906 fillet sets and 6 of 176 chamfer sets (the first comparison
counted 252 and 63: it compared a closed circle by its middle, which the
file puts opposite its seam and a replay elsewhere, read only the first
definition of an answer made of one feature per tangent chain and counted
the pieces a replay splits an edge into as other edges; the comparison now
accepts the circle, joins the definitions and the pieces and takes a face
selection and a spline's middle as they are meant). Participants are not exposed by the
reference (its value is an object handle): they are derived from the
bodies whose volume the step changed, so a participant that changes nothing
is not counted.

#### Round 2: extents, holes, revolutions, shells and datums (the API reference dumps)

The comparison above was run on the extrusion, hole, revolution, combine,
shell and datum groups, and each mismatch classified as the comparison
tool's own noise, an input the decoder reads wrongly or not at all, or an
input the file does not hold. Every rule below was found on bytes (the
controlled one-option models and the corpus' records), checked against all
the reference dumps, and read by the import before the history's guess.
Fields agreeing, before and after (the tool's own fixes, below, are part of
the change):

| Group | Agree before | Agree after | Differ / decoder missing after |
| --- | ---: | ---: | --- |
| Extrusions | 16,784 | 21,956 | 281 / 1,836 (mostly the 1,194 profile areas, which are computed, not stored; the regions; 45 participants; 16 thin flags) |
| Holes | 182 | 256 | 0 / 118 (constants of all 10 holes: default direction, taper and hub fields) |
| Revolutions | 721 | 1,057 | 18 / 147 (99 profile areas, 7 project-axis bytes, 26 unresolved profiles) |
| Combines | 66 | 116 | 8 / 37 |
| Shells | 6 | 16 | 0 / 0 |
| Construction planes | 448 | 482 | 17 / 129 (123 `isParametric`) |
| Construction axes | 15 | 27 | 1 / 8 |

| Input | Encoding found | Against the reference dumps |
| --- | --- | --- |
| Through-all direction (every writer) | the flag byte after the extent codes belongs to the side through all, whichever side that is: 0 along the sketch's normal (`isPositiveDirection` true), 1 against it; also on a symmetric through all (flag 0) and a two-sided extrusion's second side | 78 of 78 first sides and 4 of 4 second sides; the accepted answers had 62 of 68. The import's first candidate follows the byte, the opposite side and both sides stay guesses, and the extent is `read` with either byte value |
| Symmetric extent | extent code 3 with the side kinds `[1, 1]` (a distance, whose length byte says whole or half) or `[4, 4]` (through all, the whole length by definition); the dump gives `SymmetricExtentDefinition` as `extentOne` and `symmetricExtent` | 11 of 11 types, 7 of 7 lengths. The import keeps the other length as a guess |
| Up to an object | the slot role 17 (18 where the faces join a chain) names the object: `isChained` is the role; the byte after the object's reference is `isMinimumSolution`; `directionHint` is the stored vector turned round by the flag byte; two sides both up to an object keep their slots second side first; a role 5 slot with bodies as its input is an extent up to a body | chain 55 of 55, nearest solution 55 of 55 (one is 1), hint 55 of 55, the faces 52 of 55, the two second sides 2 of 2, up to a body 2 of 2 |
| Extents' other fields | `isSymmetric` false on a distance; `hasTwoExtents` by the extent code; the start kind byte 0 is a `ProfilePlaneStartDefinition` whose plane is the sketch's x-y plane in sketch coordinates | 1,167 of 1,167 distances, 1,281 of 1,281, 1,188 of 1,192 |
| Hole extent | a distance unless the hole has a slot of role 5: bodies as its input mean through all, a plane or face means up to it (the controlled models: a distance `[4]`, through all `[5, 4]`, up to a plane `[5, 4]`). The byte the earlier decoder read as the through-all flag is 0 on four of the ten through-all holes of the dumps; it is 1 where only one of the two position edges is referenced (6 of 6) | 10 of 10 through all (the accepted answers 6 of 10) |
| Hole position | the direction input's face, the two edges (the slots of role 33, in order) and the two reference dimensions | face 10 of 10, offsets 10 of 10, edges 7 of 7 and 7 of 7 (three first edges are stale in the application, which gives an error there) |
| Revolution record | after the operation `u32 2 | u8 | u8 project axis | u8 solid | u8 symmetric | u8 two sides | [second angle holder or 00] | angle holder` (class version 1 has no project-axis byte); the axis input may be a cylindrical or conical face or an edge; the participants are the slot of role 4 or 8 | solid 101, symmetric 101 (1 symmetric), project axis 94 (the 7 others are version 1), axis 101 (the accepted answers 71 of 81), participants 34 of 39. The import reads the extent of a full turn, a symmetric one or two angles, and a face axis is fixed where the face was found |
| Shell options | the two bytes after the root part: tangent-chain flag, shell type (1 rounded, 0 sharp) | 4 of 4 shells and the 4 controlled models |
| Datum profiles | a profile source among a plane's or axis' inputs is a profile of its sketch | planes 2 of 2, axes 4 of 4 |

Tool noise found and fixed in `apiref.py` (and its unit tests): a face's
plane has no side (the same plane with either normal: 12 extent faces, 2
shell faces, 11 plane inputs); bodies are compared by the item that made them
and their place among its bodies, not by the application's names (50 combine
bodies and 2 extent bodies); an empty list of participants says nothing; a plane's
`pathEntity` is the decoder's list of path edges; a hole's thread is a
reference in the application and a reflected record in the dump; a solid
extrusion's wall location is the application's default; two regions over
the same curves in another loop structure, and regions an outline holds, are
counted apart.

Profiles. Of the 211 regions missing and 149 extra: 134 and 95 are in 37
items whose stored loops contain records for curves the sketch no longer has
(822 of the 27,589 loop-curve records: 727 ids exist nowhere in the file,
95 name a curve that was split afterwards under another secondary id; the
profile is older than the sketch's last edit, so nothing in the record
names the current curves). The other 77 and 54, in 33 items, have every
record resolved; in the ones examined the application's loops merge holes that
touch along a curve, or list the regions of a union one by one where the file
keeps the union's outline, or list a curve whole that the records split into
pieces. The decoder reads the stored loops as they are; the import's even-odd
rule on the pieces (above) selects the regions.

Not read. Combine `isNewComponent` (a byte after the keep-tools byte is 0 on
all 33; no contrast). A hole's direction and default direction (the stored
vector is the start face's outward normal on all 10 holes and the application's
direction its opposite; a flipped hole would go with it, and no byte for that
is known). A revolution's direction of a partial angle (two partial
revolutions in the dumps). Thread fields of a tapered or hub thread, and the
handedness of the class version 1 thread object (all constants in the dumps).
The faces of 6 planes, 5 three-point planes' vertices and 2 tangent faces,
three extent faces of one design: the names are found in no state or in
several bodies (a face named by tag and operation only, in a history of six
bodies that share the names).
#### Sketch curve flags, spline kinds and sketches without a link (mitcad#186)

Every sketch curve class ends with the sketch-curve part (class `9FF71F46`),
whose last bytes are the block `01 | u32 n | n references | 13 bytes | f32
1.0, f32 1.0`. The block holds offset chains (`n` is 0 for all but the
curves an offset touches) and moves with them; the flags lie a fixed
number of bytes before its `01`, by the class's version (`curve_flags` in
`core/f3d-decode/src/sketch.rs` has the table; versions 7, 8 and 9 occur).
The earlier rule (kind 22 bytes before the pair) was right for version 9 and
no references only, and gave nothing for curves with a reference (the
construction lines of offsets) and the older layouts. Against the API
reference dumps, 29,762 curves of 321 designs:

| Field | Set in the dumps | Before | After |
| --- | ---: | ---: | ---: |
| `isConstruction` | 585 | 29,729 agree | 29,762 |
| `isCenterLine` | 26 | 16,237 of 16,237 lines | 16,237 |
| `isFixed` | 717 | not read | 29,762 (717 set) |
| `isLinked`, `isReference` (always equal) | 18,577 | not read | 29,762 (18,577 set) |

A linked curve comes in as a fixed reference entity (MitCad's `fixed` and
`reference` flags, as the model gives its own projections) and the sketch's
import note counts them. The model has linked projections (`Projection`:
a topological name and the entities made from it), but the curve does not
say what it follows: no source reference was found in the curve, the
sketch or any object that refers to the curves (the API's referenced entity
is a face for 1,491 of 1,512 linked curves of five sampled designs, an edge
for 20 and a sketch entity for one). Importing them as projections needs that source in the
file and the face or edge resolved to a replayed body's name; neither is
decoded, so the source is found in the replay by where the curve is
([*Sketches on other components' faces, linked curves as projections*](f3d-components.md#sketches-on-other-components-faces-linked-curves-as-projections-mitcad225),
mitcad#225).

Linked curves on the model's edges. The numbers stored for a linked curve
are single-precision rounded, up to 1e-5 mm and more off the geometry it
follows, while the replay's edge is exact: a join along the wall the
curve follows then no longer touched it, and a tool face tangent to the
circle gave an invalid boolean. Before the sketch is added, each linked
line, circle, arc and ellipse that an edge of the replay's bodies projects
onto within 2e-3 mm goes to that projection (`snap.rs`; its shared points
with it; the note counts them: "(n on the model's edges)"). A snap must
not change the sketch's regions (mitcad#227): the curve is moved only when
the curves it meets, and how (a crossing, an end on a curve), stay those
of the file's numbers, for it and for every curve that moves with it (one
sharing a point, or whose point the file holds coincident; a point the
file holds on the curve goes with the curve, as the solver takes it);
corners two curves share are the solver's and not counted. A curve whose
contacts would change stays where the file stored it, and the note says
"n left where the file stored them". Elliptical edges
and splines are not matched (the kernel lists no curve for them). Of the
35 designs this was tried on, it made 9 extrusions in 9 designs
parametric, and the sketches of four designs that could not be solved
(fixed elliptical arcs whose stored end was a few millionths of a
millimetre off the ellipse; the sketch solve now retries at 1e-7 of the
sketch size when the first does not converge).

Spline kinds. The comparison pairs 404 splines of the dumps (102
control-point, 6 fitted, 296 fixed); the type agrees for all. A fitted
spline has a filled fit-point list (`F81C561A`: the sketch points between
its ends, ending with the reference back to the spline) or handle list
(`4224E565`: its tangent handle lines): 6 of 6, and no other spline has
either list filled. A control-point spline is one an offset made (a child
of the offset's chain) or one with the fixed flag; of the 96 splines that
a join by their control points pairs, 10 are offset children and 86 have
the flag, and no fixed spline has it. Every other spline is a fixed spline. The
dump IR gives the type and, for a fitted spline, `fitPoints` (the ends and
the points between); the import keeps all three as the stored NURBS curve
(a fitted spline of the file is a degree 5 curve, which the model's cubic
fitted spline does not reproduce). The handle lines are no curves of the
sketch (the API lists none), so they are left out and a constraint on one
names the line itself, as the API does.

Sketches without a link. A sketch whose feature names it through a recipe
object (`7ACC2A03`) was not found from the sketch; the feature's own
reference to the sketch now finds it (seven circles and eight constraints
of the dumps, in one design). Remaining against the dumps: the reference
plane of 913 sketches (a face the decoder does not name; not a curve
flag), 36 points that are the corners of text boxes, 31 constraints.
#### Parameters, timeline names and the small input groups against the API reference dumps

What was the comparison's own noise and what the decoder's. The comparison
now pairs parameters by their creating item, leaves out what the reference
cannot answer, and compares by family where the two name one thing twice.

| Noise in the comparison | Count | Handling |
| --- | ---: | --- |
| Parameters of the other designs a file links to (the reference lists every component design's table, whose names `d1`, `d2`, ... repeat the root's; the file holds only its own) | 1,840 | left out when no decoder parameter pairs with them (they had inflated "missing" and, paired by name order, 431 wrong expressions, values, units and roles) |
| Parameter sections the reference failed to dump | 43 designs | left out |
| Driven dimensions: the reference gives the measurement of the sketch as it is now, the file the value of the last solve | 187 | values and expressions not compared |
| Creator kind written differently (a sketch's text or pattern distance is `feature` of type `Sketch`, a plane's offset `feature` of type `ConstructionPlane`) | 221 | normalised |
| Item types named by class (`ComponentInsert`, `GeometricRelationship`, ...) and occurrence names (the component's name with a counter, or `Component Insert <name>`) | 102; 368 | compared by family and by base name |
| Object handles the reference's interface could not convert (participants, guide surfaces, offset faces) | 147 | not compared |
| A sweep's or pipe's path entities carry their spline, type and parent path; a loft's end conditions point back at their section; a move's definition repeats the item | 44 | the entities and the section's own fields are compared |

Decoder errors found and read from the file (all against every reference
dump; the learning dump's accepted answers do not carry these fields):

| Cause | Bytes and rule | Before | After |
| --- | --- | ---: | ---: |
| Sketch names given by the user | the sketch object holds `ref anchor`, `str16 name`, `u8 0`, `u32 n`, `ref[n]` in that order; the item's tail never holds the name a user gave a sketch, and a custom name in a tail is one the sketch no longer has (an empty name in the sketch means the default `Sketch<n>`); some sketch items refer to their sketch where the sketch has no reference back | 43 sketch names, 34 renamed | 2 (the default number of a sketch in a copied component) |
| Unnamed components (`Component<n>`) | a component's name is the text after its last identifier, empty while it has the default name; the text before it is its material, which was read as the name; the default number is the one in its occurrence item's tail | 5 garbled names | 0 |
| Count parameters of patterns and sketch texts without an owner | the count's holder (`A085449A`) and the text's holder (`E5460B7B`) have the layout of a parameter holder, the owner is its root reference | creator kind 265 missing, owner 386 | creator kind 10,260 of 10,260, owner 10,134 (125 differ by the item names below, 1 missing) |
| Features of a component's own list (outside the timeline) as creators | named by their item tail, sketches by their sketch object | owner names missing for about 110 | 0 |
| Favorites | the third byte from the end of the flags after the expression | 10,509 missing | 10,475 of 10,475 (6 favorites) |
| Dependents | the reference lists every parameter that depends on one, through the others (`d3 = d2 / 2`, `d2 = width + 1 mm` give `width` the dependents `d2`, `d3`) | 4 differ | 307 of 307 |
| Text parameters | a text keeps a default `1 mm` where the other parameters keep nothing: not a comment, and no number | comment 2, value 2 extra | 0 |
| Sketches whose item refers to the sketch only | `sketch.dimensions`: 22,639 of 22,639 compared values agree (before 22,949 of 22,988, 5 missing, 34 differ; the 187 driven dimensions' values are left out, they were 31 of the differences), and the sketches' circles and points followed | 8 | 0 |

The import creates the file's parameters with their names, expressions,
units, comments and favorites (`params.rs`); the count parameters now name
their pattern, and the star of a favorite is kept.

After: parameters 82,964 of 83,107 values agree (125 differ, 18 missing,
174 only in the decoder). The 125 differences are the owner names of joints,
constraint sets and rigid groups; the 174 are parameters the reference does
not list (thread offsets and lengths 90, the counts and stitch tolerance of
mirrors 30, the joints' offsets and angles of one assembly 32, a sweep's
distances and angles 13, others). Timeline: names 5,140 of 5,184 agree, types 5,184.

Still open: the default names of joints, as-built joints, constraint sets
and rigid groups (30 items): `Rigid`/`Revolute`/... from the joint's motion,
and one number shared by joints, constraint sets and rigid groups in
creation order, a single constraint numbered as the constraints so far
counting the members of sets; designs whose earlier items live in other
files cannot be numbered from the file. Canvas names (2), the names of
fasteners and pasted occurrences, which are their component's name in a file
the design links to (4), the health state and warning text (not stored in
the item: computed when the application recomputes; 5,161 and 64), the
names of the bodies a move, split, removal or copy takes (32 inputs; the
body names the user gave are in the file but not read, the default
`Body<n>` ones are numbered by the application), a sweep's guide rail (the
decoder reads one in 3 of 4 sweeps, the reference gives none), the constants
of lofts, sweeps and pipes (closed, solid, edge alignments, orientation,
profile scaling: 90 values, the same in every design, no variant model to
read them from) and the one draft.
#### Fillet and chamfer edges against the reference (mitcad#186)

The decoder's and the accepted answers' edge sets, compared with the API
reference dumps (1,158 fillet and 193 chamfer sets):

| Field (fillets) | Before | After |
| --- | ---: | ---: |
| Tangent chain, continuity (agree / not decoded) | 1,081 / 77 | 1,158 / 0 |
| Corner flag (agree / not decoded) | 1,057 / 101 | 1,158 / 0 |
| Edges (agree / differ / the reference lists none) | 1,026 / 86 / 46 | unchanged |

*Options of older records.* The 77 sets without options are in items of
class version 2 (and a few of 3). The set's options follow its root part: version
10 of the set class `u8 asymmetric | u8 0 | u8 constant | u8 flipped | u8
curvature | u8 tangent chain | u8 0 | u8 0`, version 9 `u8 asymmetric | u8
constant | u8 flipped | u8 tangent chain | u8 0 | u8 0` and version 4 the
same without the last byte, then the set input; five sets have a block of 20
zero bytes in between. The tangent chain byte agrees on all 1,158 sets (24
are off; the 12 off among the older records too); the fifth byte of the older
layouts sits where version 10 keeps the curvature flag and is 0 on all, but
no reference set has a curvature continuity, so a set with it is not read. The
corner flag (`u32 0 | u8`) was dropped by a guard against a following
health reference: the flag, a zero and a one read as a reference to the
object with id 256 in every design that has such a health object, which
removed 27 sets; the flag is read there now (the variant models'
rolling-ball and setback corners differ in this byte alone). With the
options the history-accepted answers' tangent chain agrees on all 196 settled
sets of 32 designs (183 before; the unset option was taken as on).

*The edges the replay lacks.* Of the accepted answers' edge lists that
differ from the reference (items, old learning dump, comparison folded as
above): 35 have a decoded edge, which agrees with the reference, that
`refs::dressup_edge_match` did not find in the replay, so no decoded
candidate was tried; 25 have a decoded selection that is itself another
edge than the reference's (below); 8 had the decoded candidate tried first
and the history contradicts it (the kernel fails, or the file's feature
rounds a neighbouring edge too, a tangent chain beyond what the kernel
follows), so a guess was accepted; the rest are partly unresolved decoder
edges. The decoded candidate was first wherever it existed ([*Named inputs*](f3d-history.md#named-inputs):
the ranking only reorders candidates that predict a volume change), so
no ordering changed. The matcher changed instead: an edge the replay splits
into any number of pieces, a curve found by its ends and a length up to 2 %
off, and a middle within 0.01 mm with a length up to 2 % off (*Named
inputs*). On 32 designs chosen from the mismatch list, imported one by
one, the accepted answers' edge lists agree with the reference in 175 of 207
sets (158 before, the same learning dump comparison). One item gets worse: a
later fillet whose edge the replay cannot name now builds on the file's nine
edges of its predecessor instead of the seven the history accepted, and
falls back.

*What stays open (decoder).* 86 fillet edge lists differ from the reference.
Edges the decoder lists without geometry: 21 (the names fit two, three or
eight edges equally where the recipe's tail cannot be compared with the
state's faces, one end of the edge having been renamed since; or no edge
between the faces named), 11 in items whose references the application
reports lost, 11 in items rolled back after the timeline marker, which the
file keeps no state for. Other edges: 43, of which 27 are equally long
edges elsewhere on a symmetric or patterned part (twin copies, closed
edges, copies of a mirrored component), and a tangent chain the reference
lists in full where the file names one edge. The tail picked the right edge
on 6 of 6 edges it decided outside one item. Chamfers: 5 of 193 sets differ;
the chamfers' tangent-chain and flipped options are not decoded (the
reference has no chain off and no flipped set, so no byte can be told
from it).

*Comparison.* `apiref.py` accepts a closed circle by its circle, folds the
definitions of an answer (one per tangent chain or body) into one input,
takes pieces of an edge for the edge, the edges on a selected face for the
face, a spline's middle within a tenth of its length, and counts a set the
reference lists no edge for apart.

#### Fillets and chamfers that fall back for reading reasons

Fillets and chamfers that fell back although their edges were decoded were
read item by item against the API reference dumps (edge count, positions
and lengths, sizes) and the import's trace. Where the decoded set agrees
with the reference (edges, sizes, the state before the item) and the item
still falls back, the geometry kernel is the cause (below). Three readings
were wrong or missing:

- *The body of an edge.* Bodies that touch have edges in the same place,
  and the matcher kept the first body of the coincident copies. An edge's
  input refers to its body's record, so the decoder gives it
  `_f3d.producer` and `_f3d.body_index` like a body input, and the import
  matches the edge on the bodies of that item first
  (`Importer::match_dressup_edge`). A fillet of the edge of a thin wedge
  that sits in a slot of another body was rounded on the slot's body (the
  wrong body: volume and faces never matched, and the items after it
  needed the file's bodies): it builds now, and the edge counts as read.
  On the 50 designs imported again (those that reach the import's time
  limit left out), 17 fillet and chamfer items that were `inferred` (the
  first-body choice) are `read` or `unknown` now, one fillet more is
  parametric, and no item got worse.
- *Closed ellipses.* A closed edge that is not a circle (a round hole cut
  at an angle) was found by nothing, a split replay copy of it least. Its
  axes follow from the file's start (one end of an axis), its middle
  (the other end), its direction at the middle and its length (the
  perimeter fixes the second axis); the replay's edge, or its pieces, are
  found on that ellipse (`FileCurve::Ellipse`). The edge of one design (an
  ellipse the replay splits in two at the ends of its axis) is found by its
  name now.
- *Elliptical arcs and splines the replay splits.* A circle through the
  three points the file gives can have the arc's length without the arc's
  pieces lying on it (an ellipse near its minor axis), so the pieces were
  never taken for the edge. An open edge that is not a line is found, when
  nothing else finds it, by pieces of one body that follow each other by
  their ends from its start to its end, whose lengths add up to its and
  that pass within 2 % of its length of the file's middle
  (`refs::chains_by_ends`); two such chains are ambiguous.

What the kernel does, seen on the items whose decoded sets agree with the
reference (counts are items):

- a rounding or bevel as wide as the face next to it takes the face away
  exactly. A chamfer of the width of the annulus beside it is built 0.1 %
  smaller, which moves the edges the next items name by a few micrometres,
  so they are not found and the items after it fall back (1 item). At the
  exact width the bevel of another item came out as a solid that lost 14 %
  of the volume instead of gaining 2 %: valid for the checks, so not built
  again smaller (1 item). A fillet wider than the face beside it builds a
  shape with four faces more than the file's (1 item);
- a bevel or fillet ends at a vertex where a neighbouring bevel meets it,
  or along the junction of two blend faces that become tangent to a plane
  at its end: the volume differs by 0.1 to 5 % of the body, with the same
  faces in two of the 3 items;
- fillets on both sides of a slot narrower than twice the radius (1 item),
  and a rounding of 0.1 mm that fails where 0.05 mm builds (1 item).

#### Chamfer options, moves, sweep rails and face profiles (round 2, mitcad#186)

Chamfers, moves, sweeps and extrusions of faces were read item by item
against the API reference dumps and the controlled models.

- *Chamfer sets.* The byte right after the root part of a chamfer's set
  record (class version 2) is the set's tangent-chain option. It is 1 on the
  309 sets of the API reference dumps (all `isTangentChain` true) and 0 on
  the 15 sets of the controlled models made by script (all false), so the
  decoder gives `isTangentChain` per set (and per feature where the sets
  agree) and the decoded value agrees on 324 of 324 sets. The record has no
  byte for `isFlipped` of a two-distance chamfer (every dump set has it off,
  whatever the bytes), so the sides keep coming from the edges' directions
  in the file ([*Named inputs*](f3d-history.md#named-inputs)), the other reading staying a guess. An equal
  or two-distance chamfer whose corner type, sizes, tangent-chain option and
  edges are all decoded fields is `read` when its decoded definition is
  the one the history accepts (`read_chamfer_inputs`, as for fillets); a
  distance-and-angle chamfer's angle side is not decoded, so it stays
  `unknown`. On the 11 designs imported again that have chamfers, 19 items
  went from `unknown` to `read` and no outcome changed.
- *Moves.* A move's transform object that stores no matrix (its `u32 5`
  followed by `u8 1 | u16 0 | u8 1 | u32 0 | ref` where the 128 bytes of a
  matrix go) is the identity: 2 of the 27 moves of the API reference
  dumps, both identity there; the other 25 store their matrix and all 27
  agree with the dumps. A move whose bodies and matrix are decoded is
  `read`.
- *Sweep rails.* The rail slot of a sweep (class version 6) holds curves of
  the profile's own sketch in 3 of the 4 sweeps of the API reference dumps
  (the fourth has none), where the dumps list no rail; the one controlled
  model with a rail has it in the path's sketch. Curves of the profile's own
  sketch are not taken for a rail now, so the 4 sweeps agree (3 differed).
  The two designs whose sweep fell back because the rail was in its way (the
  volume it made was a third of the file's) are parametric now.
- *Two-sided extrusions with a negative distance.* A cut through all one
  way with a negative second distance (one case) removes what a cut through
  all one way plus the same cut the other way by that distance removes,
  which is the file's state exactly, so the sign of a two-sided distance
  does not turn its side; before, the model refused it.
- *Extrusions of a face up to an object.* A profile that is only a face of
  a body (no sketch region) was made into a sketch on the face and
  extruded by a distance only; an extent up to a decoded face or plane is
  made the same way now, the side chosen by the history.

- *Tangent circles.* A region whose hole is a circle touching its outer
  boundary at a point (two circles that meet at one point)
  failed to build as a face (`intersecting
  wires`), and the prisms of such regions did not fuse with the regions beside
  them (no body at all). The hole's wire shares the outer boundary's vertex
  now and touching regions are united as faces before they are extruded; the
  design's three regions come out as the file's body (volume, area and faces
  equal), and the design is parametric.
- *Combines, shells and pipe threads.* A combine whose bodies, operation and
  keep flag are decoded, and a shell whose body or faces, thickness, type and
  tangent-chain flag are, are `read` (the dumps agree on all 33 combines'
  operation and keep flag and on all 7 shells' type and chain). A pipe
  thread's class `A` is on the internal threads of the stored designs (6 of
  6), which MitCad's table listed for the external ones only, so the class was
  left out with a note and the thread stayed `unknown`; the table lists it for
  both. On one design with 94 items, 7 items (a shell and six threads) went from `unknown` to `read`.

What the file does not give in these items (counts): the profile records of
a sweep or revolution whose sketch curves were edited after the feature
(their ids name curves that are gone: `unknown`), and profile curves that
are splines, which the region matching does not follow (`unknown`).

#### Fillets and chamfers that fall back: a second pass (mitcad#186)

98 fillet and chamfer items of 26 designs (some are versions of one another)
fell back with `its result is not in the file's history`, a volume that is
off, `edge … no longer exists after …` or an invalid shape. Each was read
against the API reference dumps (the sets' edge counts, lengths, middles and
sizes). 9 of the items have no row there (8 are of a design the reference
application could not walk); of the other 89 the decoded input agrees in 83
(82 before the change below): 4 are references the file lost (below), one
set lists the same edge twice (the same set of edges) and one input names two
edges equally (the history's guesses decide). So the kernel is the cause of
the rest, as before. Two readings were new:

- *Inputs that fit the same edges.* The recipes of the edges of one set that
  lie between the same two faces can differ in the faces at their ends only
  (the 8 edges between a plane and a free-form face), so each fits all of
  them and none was found. Where the inputs of a set that fit the same edges
  are as many as those edges, each takes one (`core/f3d/EDGE_ORDER.md`,
  *Inputs that fit the same edges*). Of the 92 inputs of the 358 reference
  designs the names fit several edges for, 36 are found now; 3 edge sets that
  differed from the reference agree, no set that agreed changed. One of the
  98 items is such a set (8 edges, a free-form patch): its decoded definition
  is the first candidate now (`read`), and it fails in the kernel (an invalid
  shape, 16 s a try). A fillet of another design that was `inferred` from the
  history's next state is `read` and verified now.
- *Edge references the file lost.* The health of an item is not in the file,
  but its inputs show it: a name whose faces are not in the history state the
  item was made from, or between whose faces no edge lies. Of the 25 fillets
  and chamfers of 16 reference designs with such an input, 23 are items the
  reference reports as having lost edge references, using the geometry they
  had cached (7 errors, none of them lists an edge; 16 warnings: the edges
  the reference lists are the edges found in 8 of the 16, in the others the
  decoder finds more edges or fewer than it lists); the other 2 are one item
  in two versions of a design. Such an item gets its result from the file,
  not from its inputs, so no definition of its inputs can rebuild it: its
  note says `N of its M edge references are lost in the file …` before the
  last kernel failure (4 of the 98 items).

What the kernel does with the rest: the decoded definition was built on the
replay and on the stored state before the item; then the size scaled by 0.5
and by 0.1 in a replay of the decoded design (tells a size too large for the
part from a case the kernel cannot do at any size); `isTangentChain` off
(tells whether the kernel's chain is longer than the selection); the
narrowest face beside the edges, the reference's face areas divided by the
edge lengths, against the radius. The classes of the 98 items (the same case
in versions of a design counts each):

| Class | Items |
| --- | ---: |
| the file's edge references are lost (above) | 4 |
| the kernel builds a result that is not the file's | 16 |
| fails at the full size, builds at a smaller one, a face beside the edges narrower than two radii | 20 |
| the same, no narrow face (or no reference row) | 16 |
| fails at every size scaled: the rounding cannot end at a vertex 17, the radius may be too large 15, an invalid shape 8 | 40 |
| the time limit (the kernel's tries of 16 to 100 s) | 1 |
| not scaled (no reference row) | 1 |

Of the 16 that build something else: in one the face beside the edge is as
wide as the rounding (a closed edge, a ring face 2.48 mm wide rounded by
2.5 mm: the result has the volume it had before and 15 faces fewer than the
file's); in two the kernel's contour follows a tangent continuation the
file's rounding did not take (the file's next state lost exactly the
selected edges; the kernel cannot round the selection alone; done in
mitcad#241, below); in three the
volume is within 1.5 % of the file's with other new faces (blend surfaces
and corners); in eight it is 4 to 77 % off; in two (chamfers, with the file's
number of faces) the volume removed is too large.
These go to the kernel work (mitcad#121); none is decoding.

*The longer tangent chain (mitcad#241).* Both items (two designs of one
project: a ring-shaped part, and a post in an assembly) round
edges whose next edges turn from them (0.74 degrees along the top of a
ring where a corner cylinder of radius 1 meets it; 4.2 degrees up the
corners of a post whose walls are drafted by 3 degrees above a vertical
part), while the faces across the vertex go on tangentially within OCCT's
0.1 rad, so OCCT's contour ran on over the turned edges. The file's chain
stops there and the face beyond the crease, extended, cuts the rounding.
With the OCCT port's patch 0180 MitCad's fillets stop their contours at
such turns unless the next edge is rounded too, the face beyond cuts the
rounding where the crease is concave (both items), and the vertex left
between two pieces of that face's edge is removed: both items are
parametric and verified against the file's next state, the ring's
rounding a torus with circles for its contacts as in the file, and the
final bodies as before (the ring's within 5e-9 of the file's volume). In
the assembly a later fillet of the post (a slot's edges up the vertical
wall, turning at the same crease) became parametric too. A chamfer after
it that the file shows no change for, whose edges now resolve on that
replayed rounding and fail, falls back where it was skipped (its stored
body is the same state).

### Sketch texts (mitcad#186)

The decoder reads a sketch text (class `E0618268`) into the dump's `texts`:
the string, the font family, the height, the weight (bold from 600), the
alignments, and for a text in a box the box's four lines and, in some files,
the placement matrix (the box corner and the baseline's direction: `position`
and `angle`; the API gives neither). The lines come from the text's own
`TextBoxConstraint` (its curves in order, a corner point among them left
out); the curve of a text on a path from its `TextPathConstraint`. The corner
points a box makes (class `00D157C2`) keep their coordinates in the sub-chunk:
before, the box's points had none. Checked against the API reference dumps:
the string, height, font, style, alignments and kind (box or path) of all 10
texts of the 6 dumps with texts (3 designs, 9 in a box and 1 on a path) agree,
and so do the reference model's; 11 more texts of a design without a dump
decode (all in a box).

The file's texts are measured as follows (derived from the API dumps'
bounding boxes of three texts: the reference model's, a rotated one and a
wrapped one, to within a font's glyph shapes; `cap_height` and `wrap` of the
sketch text):

- the height is the height of the capital letters, not the font's em;
- a line's outline, not its pen advance, is aligned: left to the box's left
  side (without the first letter's side bearing), right, or centred;
- a text in a box aligns vertically by its capitals: the top of the first
  line's capitals at the box's top, the last baseline at the bottom;
- lines are one ascender plus descender apart (no line gap);
- a line breaks at its spaces to fit the box's width (a word wider than the
  box stays whole and overflows).

Without the box, the first line's direction gives the text's angle.

The font: the glyph outlines come from the installed font of that family
(OCCT's font manager); a family that is not installed gives the bundled
Droid Sans and the sketch a warning ("the font Arial is not installed;
Droid Sans is used"). The glyphs then differ from the file's, so a text
extrusion's volume differs from the history's and the extrusion stays a
fallback on a machine without the font (nothing is substituted by shape).
A text keeps outlines a file stores for it while its font is missing
(mitcad#416, the `.ipt` import's texts), but an `.f3d` text record holds
none: its profile loops have no curve records either (below).
A text on a path: below.

### Texts on a path (mitcad#1177)

A text's path constraint is a class of its own (`9D30FCDC`, a subclass of
the constraint class) that keeps, between its root part and its sketch
reference, the text again (after a `u8 1` from class version 1), a `u32`
character count and per character `u32 16` and a row-major 4×4 matrix: the
character's placement in the sketch (cm), its x axis the baseline's
direction, its y axis the character's up, its translation the pen moved up
to the font's ascender line. A reported drill index (content id 528e5029,
the only file of the test files with the class) has 90 characters, 11 880
bytes before the sketch reference, so the constraint reader, which looked
for the sketch within 1 000 bytes, left it undecoded and the text had no
path. What the file says, checked against its stored body (the embossed
glyphs' vertices) and its preview:

- the characters stand on the circle's outside, their capitals' top on the
  circle (4.25 cm) and their baseline a text height further out (4.75 cm),
  read counter-clockwise; the pens follow the circle's arc length, each
  character's middle on the circle;
- the matrices run clockwise from the last character: a text below its
  path stores its characters against the reading order, so the order gives
  `isAbovePath` (false here);
- the text is left-aligned in the record, and its last character ends
  exactly (to 1e-6°) opposite the circle's reference direction: a whole
  circle as a path starts there (its parameter runs from -π to π about the
  reference direction), and a text below its path is the text above it
  turned round, so its ends swap. The replay gives Mitcad's text the path
  with `above: false` (hanging from its capitals with `cap_height`), the
  alignment swapped (right here), and the construction circle that start
  (`start_angle`, from `_f3d.pathStart`), from which Mitcad lays a path
  text out;
- the characters keep their own widths: no kerning (the text's pair of
  ones stands 0.52 mm, Arial's kerning of "11", farther apart than the
  font's kerning puts it), so the replay sets the text's `no_kerning`;
  with it the glyphs' vertices lie within 1.5 µm of the stored body's, the
  curves between them within 0.034 mm (the file's are cubic B-splines
  through them, the font's quadratic);
- the extrusion's 59 text profiles have no loops of sketch curves, one
  profile a letter: the dump names the texts (`_f3d_text_profiles`) and the
  replay selects their letters' regions (each glyph contour that is a
  letter's outline, not a counter), exact when the item selects as many
  profiles as there are letters and only the texts'.

The drill index's emboss then replays parametrically: 1090 faces as the
file's, the body within 5e-7 of the stored volume (0.3 mm³ of the 148 mm³
the letters add, from the curves' shapes). The two holes after it, placed
where the history removes material, compared the replayed body with the
stored one in a box grown by half its diagonal round the changed faces:
for the deep centre hole (8 by 8 by 125 mm) 63 mm on every side, over the
whole top with the text, whose faces lie micrometres off the stored ones,
so the whole bodies were cut (800 s in a debug build) and the hole fell
back. The removed material is first cut within boxes grown by half their
middle side (4 mm here; `geometry/src/removed.cpp`), then as before: about
10 s, and both holes parametric. An open path below its text and a path turning clockwise
about the sketch normal are not in the test files; the side and the swap
follow the same rule, the start of an arc is its own.


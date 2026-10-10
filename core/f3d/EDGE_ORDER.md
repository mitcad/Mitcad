# Edge recipe order investigation (mitcad#106)

The decoder retains recipe framing and can report the cyclic order of
faces around an ASM edge's endpoints. The relationship between that order
and the bytes after `edge_recipe_data` is **unverified**. Those bytes do
not participate in production edge matching. Existing ambiguous matches
remain ambiguous.

`design::recipe::parse_details(segment, object_data)` returns the same
supported recipe as `parse`, its secondary entity list, and `tail_offset`
and `tail` immediately after the type string. No new field layout is
assigned to the tail. Secondary lists on non-edge recipes remain
unsupported, as before.

Learning records retain a `recipe` block on supported recipe objects,
within the existing 16 KiB object limit: `kind`, `entities`,
`secondary_entities`, `tail_offset`, `tail_hex`, and
`tail_interpretation: "unverified"`. `tools/f3d-learn/learn.py show`
displays this framing; its reading functions accept `tail+N` addresses,
for example `u32(path, 'tail+0')`. This is an address into opaque bytes,
not a statement that the bytes contain a u32 field. Existing `gK+N` gap
addresses keep their meaning. Old records without framing return no
value at a `tail+N` address.

## ASM face cycles

`names::NamedState::edge_endpoint_face_cycles(edge, face)` takes indices
into `state.edges` and `state.faces`, with `face` one of the edge's two
distinct incident faces. It returns two vectors of face indices:

- Endpoints follow the selected face's coedge direction.
- Each vector starts with the selected face, then the other incident face,
  then the remaining faces around that vertex. The anchor is not repeated
  at the end.
- At the start vertex, each step crosses to the coedge's partner and then
  to that partner's `next`. At the end vertex, it crosses to the partner
  and then its `previous`. Opposite partner senses and connected endpoint
  identities keep the walk at the same vertex.

Traversal uses the existing ASM field layouts documented in
[ASM_FORMAT.md](ASM_FORMAT.md), including history-state entity remapping.
It requires reciprocal loop and partner links, exactly two opposite
coedge uses on every traversed edge, closure on the original anchor, and
coverage of all coedge occurrences and named faces incident to the vertex.
Loop walks are bounded by the file's record count; vertex walks by the
retained coedge count. Face order comes from these links, with no spatial
angle sort or recipe-derived interpretation.

Boundary edges, seams, repeated faces in a fan, nonmanifold or disconnected
fans, closed or degenerate edges, missing partners, malformed links and
incomplete ASM files return an error. Point loops and malformed loops
anywhere in the selected body also prevent cycle reporting. Refusal is
diagnostic evidence of unsupported topology, not proof that the recipe is
invalid.

For a future learning-only collector, use the same ASM history view as the
selection resolver, enumerate existing name candidates, and retain both
possible adjacent-face anchors:

```rust
let state = NamedState::new(&asm, Some(&history_view));
for edge in state.edges_fitting(&recipe.entities)? {
    for &anchor in &state.edges[edge].faces {
        let cycles = state.edge_endpoint_face_cycles(edge, anchor)?;
        let face_names = cycles.map(|cycle| {
            cycle.into_iter()
                .map(|face| &state.faces[face].names)
                .collect::<Vec<_>>()
        });
        // Record edge entity, anchor entity, face_names and any errors.
    }
}
```

The snippet illustrates the API; a collector must retain each error and
continue other candidates. Cycles are not emitted automatically by the
import's learning context. A call rebuilds the selected body's coedge
topology, so future collectors should run only when learning is enabled,
bound candidate/output counts, and consider reusing the topology when
collecting many edges from the same state.

## Evidence still required

The added synthetic tests describe topology direction, rotation of an
anchor, history remapping and conservative refusal. They do not establish
binary recipe semantics and were not executed on the owner's machine.

Before enabling order-based matching, obtain independent selected-edge
references at the correct history state, retain every existing candidate,
and explain the entire tail framing, including counts and separators.
Check both endpoints and adjacent-face anchors, operation-generated edge
name prefixes, secondary lists and absent/truncated tails. Include vertices
with at least four incident faces to distinguish cyclic order from simple
membership. Test the proposed interpretation on designs excluded from its
derivation and report counterexamples and unsupported topologies. A match
must require one supported candidate; ambiguous or unverified cases must
retain fallback.

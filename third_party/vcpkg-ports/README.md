# vcpkg overlay ports

`vcpkg.json` (`vcpkg-configuration.overlay-ports`) makes vcpkg build these
ports instead of its own, on every platform and for every use of the
manifest (the CMake presets, `tools/dev-env/build-cycles.sh`). vcpkg builds
an overlay port from source once per machine and triplet and keeps the
result in its binary cache; a change to the port rebuilds it.

## opencascade

vcpkg's `opencascade` port for OCCT 8.0.1 as of the baseline in
`vcpkg.json`, with four patches of Mitcad's.

| Files | Origin | Licence |
|---|---|---|
| `portfile.cmake`, `vcpkg.json`, patches 0001 to 0005 | microsoft/vcpkg, `ports/opencascade` at the baseline; unchanged but for `port-version`, the list of patches and the SPDX line | MIT |
| patches 0006 to 0009 | Mitcad | LGPL-2.1-only WITH OCCT-exception-1.0, as OCCT |

The patches change OCCT, so they are under OCCT's licence (the GNU LGPL
version 2.1 with the Open CASCADE exception), published here with Mitcad's
source, and OCCT stays a set of shared libraries (vcpkg's dynamic
triplets), as the [licence policy](../../docs/development.md#licence-policy)
requires; the third-party notices of the packages name them. Each patch
says at its top what it changes and why. None changes a result: each
removes repeated work that made an algorithm quadratic in the edges or
wires of a face, which bodies with thousands of holes reach (mitcad#39,
mitcad#72, mitcad#77, mitcad#79).

- `0006-shapeupgrade-unifysamedomain-seam-search-by-vertex.patch`: the face
  merge after booleans (`ShapeUpgrade_UnifySameDomain`) looks up the edges
  it removed by vertex instead of going through all of them at each step of
  a wire across a seam.
- `0007-brepcheck-analyzer-no-search-in-face.patch`: the checker
  (`BRepCheck_Analyzer`) no longer searches a face for each of its own
  vertices and edges (`InContextOfAncestor`).
- `0008-brepcheck-wire-seam-edges-once-per-face.patch`: the checker finds
  the seam edges of a face once instead of once per wire for the 2d closure
  check, and no longer searches the face for each of its wires
  (`BRepCheck_Wire::InContextOfFace`).
- `0009-bopalgo-face-face-adaptors-once-per-face.patch`: the face/face
  intersections of a boolean build the surface adaptor of each face (its UV
  bounds go through all its edges) once instead of once per pair of faces
  (`IntTools_Context::SetSurfaceAdaptor`).

Each patch has a report for OCCT, with a reproduction that uses OCCT alone
and measurements: mitcad#84 (0006), mitcad#90 (0007), mitcad#91 (0008) and
mitcad#92 (0009); sending them upstream is the maintainers' step. When the
baseline moves to a newer OCCT, check which of the changes OCCT has made;
update the port from vcpkg's and apply the remaining patches again
(`git apply` in OCCT's source tree, in order), and compare the corpus
results as for any change to the geometry. Building OCCT from the port is
described in [development.md](../../docs/development.md#faster-builds).

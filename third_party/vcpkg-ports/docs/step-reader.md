# The STEP reader: 1820

mitcad#2028 (GitHub mitcad/Mitcad#455): opening a STEP file crashed the
application in OCCT's pass that names a part's sub-shapes. Back to the
[port's README](../README.md).

## Overview

- **1820** makes the sub-shape name pass of `STEPCAFControl_Reader` walk
  past a face bound, a loop, a shell or an edge list the file reader left
  null, as the shape transfer already does: a file with such a reference
  opens with the names and colours of everything else.

## The case

An automatic crash report of 0.66.0 on Windows: an access violation in
`Standard_Transient::IsInstance`, called from
`STEPCAFControl_Reader::ExpandShell` <- `ExpandSubShapes` <- `Transfer`,
while a STEP file was opened. No file came with it.

MitCad reads STEP with `DESTEP_Parameters::ReadSubshapeNames` on
(`geometry/io/src/step.cpp`): the names and colours of a part's solids
come from the sub-shape labels (`geometry/io/src/xcaf.cpp`). After the
shapes are transferred, `ExpandSubShapes` walks each part's
MANIFOLD_SOLID_BREP and SHELL_BASED_SURFACE_MODEL down to its faces,
bounds, edges and vertices and labels each.

The file reader leaves a reference it cannot resolve null: an entity
number not in the file ("Unresolved reference") or an entity of another
type ("Entity has illegal type"), reported in the entity's check. The
shape transfer treats a null face bound and a null loop as absent
(`StepToTopoDS_TranslateFace`), so the face and the solid are made of the
rest. The name pass skipped a null face, bound array, oriented edge and
edge, but dereferenced a null face bound and a null loop (the virtual
`IsInstance` on a null loop is the reported fault), a null edge list, a
null outer shell or face array and a null shell array of a surface model.

Turning `ReadSubshapeNames` off for files with such references would avoid
the patch, but would drop every name of the file and cover only one of
these nulls.

## 1820: the name pass past what the reader left unresolved

### 1820-stepcafcontrol-sub-shape-names-skip-what-the-reader-left-unresolved

- `ExpandManifoldSolidBrep` and `ExpandSBSM` return on an item of another
  type and on a null boundary array.
- `ExpandShell` returns on a null shell or face array, goes on to the next
  bound on a null face bound or loop, and to the next loop on a null edge
  list.
- What is null has no sub-shape in the transferred shape to put a name on;
  everything else is named as before. No catch is added.

## Reports and tests

- `geometry.port.1820` (`geometry/tests/port/p1820-step-sub-shape-names-unresolved-bound.cpp`):
  a 10 mm box as the MANIFOLD_SOLID_BREP of a product named "Null bound
  box", one face with a second FACE_BOUND whose bound is an entity not in
  the file (`geometry/tests/step_null_face_bound.hpp`), read from memory
  through `STEPCAFControl_Reader` with sub-shape names, OCCT alone. Stock
  OCCT crashes in `ExpandShell`; with the patch it is one valid solid of
  1000 mm3 with six faces, under the product's name.
- `geometry.io` reads the same text with `read_step_structured`: one body
  named after the product, 1000 mm3.
- The STEP read is also one of the geometry's operations
  (`geometry::run_operation("step", ...)` in `read_step_document`), so that
  another crash inside OCCT's reader is an error that says the file could
  not be read instead of a crash report: `geometry.step_read_crash` and
  `cli.occt_crash_step` with `MITCAD_TEST_OCCT_CRASH=step`.

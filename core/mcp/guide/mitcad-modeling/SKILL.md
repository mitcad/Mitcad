---
name: mitcad-modeling
description: How to model well in MitCad through its MCP tools, so that a design stays editable - design intent and named parameters, placement on references, fully constrained sketches. Read it before building or changing a design in MitCad, then the topic that fits the task.
---

# Modeling in MitCad

<!-- SPDX-License-Identifier: MIT -->

These guides say how to model well in MitCad. What exists (the commands,
queries and feature types, and their fields) is in the reference:
`model_reference` with section, query, command, feature or search.

Read the topic whose line below fits the task, one at a time. A topic is
`model_reference {"guide": "<id>"}`, the resource `mitcad://guide/<id>`,
or, as a skill, the file `references/<id>.md`.

## Topics

- [principles](references/principles.md): before any modeling task; intent, parameters, feature order, names and groups.
- [choosing-features](references/choosing-features.md): when several features could make the shape; choose the one that preserves its purpose.
- [sketches](references/sketches.md): before the first sketch or a sketch edit; joined geometry, constraints, dimensions and projections.
- [references](references/references.md): before placing a sketch, component or feature; stable parents and geometry that follows edits.
- [verification](references/verification.md): after every modeling change; status, dimensions, bodies, pictures and recovery.
- [3d-printing](references/3d-printing.md): when a component is to be FDM/FFF printed; orientation, fits, printable details and manufacturing checks.

# Principles

Read this before any modeling task. Three rules keep a design editable:
when the user changes a size later, the rest of the model follows.

## 1. Plan the intent, then name the sizes that matter

Before the first feature, decide what stays fixed and what may change:
the key sizes, the symmetries and alignments, and which features drive
others. Say it in a line or two, and ask the user when the request is
ambiguous. Make a size a named parameter (`add_parameter`) when the user
will want to change it or it is used more than once, and write the sizes
that follow from it as formulas of it, not as copied numbers.

## 2. Place everything on references

Put a sketch's `plane` and a primitive's `at` on an origin plane, a
construction plane, an axis, a point or an existing face, so that it
follows the model. A fixed plane (`{"origin", "normal"}`) stays where it
is written when the model changes. Do not use fillet or chamfer faces,
or the faces of a pattern's instances, as parents: they come and go as
the model changes.

## 3. Fully constrain every sketch

Unless the user asks otherwise, dimension the sizes that matter and
constrain the rest (coincident, horizontal, equal, symmetric, tangent),
tied to the origin or to existing geometry, until the `sketch` query's
`dof` is 0. Do not fix points just to remove degrees of freedom: a fixed
point does not follow the model.

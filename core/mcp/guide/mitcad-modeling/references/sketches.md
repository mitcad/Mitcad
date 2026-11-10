# Sketches

<!-- SPDX-License-Identifier: MIT -->

Read this before the first sketch and before changing a profile or
route. A sketch describes relationships and dimensions; coordinates
alone are initial positions, not design intent.

## Choose a purpose and support

Give each sketch one clear job: a main section, a hole layout, a path or
a shared layout. A small layout sketch can carry mounting centres and
axes that several later sketches project, instead of repeating their
coordinates independently. Distinguish construction geometry from the
curves intended to form a profile.

Use `sketch.create` on an origin plane, construction plane or stable
planar face. Read [references](references.md) for placement choices.
Inspect the plane's frame: XY, XZ and YZ do not all map their second
coordinate to the same world axis. Query `sketch` before assuming an
orientation.

## Ids and joined points

Points, curves and texts share one counter in each sketch: p3, c4 and t5
are different entities, and p4 is not another name for c4. Constraints,
dimensions, patterns and offsets share another counter with names such
as k6. Obtain ids from command results or the `sketch` query; never
calculate the next id from an earlier number.

A drawing command's point input `[x, y]` creates a new point, even when
its coordinates equal an existing endpoint. Where the command accepts a
point input, give the endpoint's returned id to join the next curve to
it. Otherwise add a coincident relation afterwards; coincident curve
endpoints are merged by the sketch command. Coincident coordinates
without a relation remain separate points that can open when edited.

Create geometry in a call, read the returned ids, then constrain or use
them in the next call. Do not predict ids produced earlier in the same
`model_command` array. Read the reference for fields that take a position
only, such as a pick place, rather than an existing point id.

## Constraints

Unless the user requests otherwise, finish with the `sketch` query's
`dof` equal to 0, a solved sketch and no unexplained warnings. Connect
the sketch to its origin or intentional projected references so that the
whole outline cannot translate or rotate freely.

Apply geometric relations before driving dimensions: coincident ends,
horizontal or vertical directions, parallel and perpendicular edges,
tangent transitions, equal repeated sizes and symmetry about an axis.
Then dimension the independent sizes and offsets that express the
purpose. Use `sketch.add_constraint` and `sketch.add_dimension`, looking
up each relation's fields with `model_reference` first.

Do not use `sketch.set_fixed` on free geometry merely to remove degrees
of freedom. It freezes today's positions instead of explaining how they
follow edits. Intentionally static imported/reference geometry can be
appropriate; distinguish it from new geometry intended to be driven by
dimensions. Do not add redundant driving dimensions: use a driven
dimension to measure a size already implied by other relations.

Re-query after each meaningful set of relations. A rejected solve names
conflicting constraints; inspect them and existing dimensions rather
than adding another constraint or moving points to force a result.
Correct the relation that misstates the intent.

## Dimensions and parameters

Use named user parameters for shared or adjustable dimensions such as
wall thickness and mounting spacing. Give dimensions expressions based
on them rather than copied literals. A driven dimension reports a size;
it does not drive geometry. Read `parameters` for user, model and
reference parameter roles.

Coordinates are sketch-plane millimeters and angles are radians when
given as JSON numbers. Use unit-bearing text for other units or degrees.
Dimension to origin axes, construction lines or shared references when
those describe the design better than arbitrary endpoint distances.

## Projections and shared layouts

Use `sketch.project` with a linked projection when a later sketch should
follow an earlier sketch's point/curve or a body's edge. The source must
precede the consumer. Project only what is needed and respect reference
policies across components.

Projected geometry is reference geometry. Do not drive it to a second
set of positions or freeze a copied outline and call it associative. If
a link stops following, inspect projection records and warnings.
`sketch.project_again` repairs a linked projection that no longer follows
when that is the intended repair; `sketch.convert_projected` deliberately
makes ordinary free geometry and removes the association. These choices
change intent and are not interchangeable.

## Profiles and the Sketch Palette

Read `profiles` after creating or editing a closed section. Use its
returned region names in the consuming feature rather than inventing a
region from presumed curve order. Check that construction lines,
duplicate curves, separate endpoints and tiny gaps have not changed the
regions. The `sketch` query also reports collapsed geometry and redundant
relations.

In the application, the Sketch Palette exposes constraints, dimensions
and warnings. Read query data to understand its state; appearance alone
does not prove full constraint. With a sketch open, respect the live
server's edit-session rules and finish it before incompatible feature
commands.

Before leaving, verify purpose, support, solved state, degree count and
profiles. Follow [verification](verification.md) for the consuming solid
feature.

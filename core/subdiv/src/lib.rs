// SPDX-License-Identifier: MIT
//! Subdivision surfaces of the Form feature (mitcad#1757): the control
//! cage a Form stores, its edits, Catmull-Clark subdivision with creases
//! and boundaries, and the limit surface as bicubic patches that the
//! geometry kernel sews into a solid or a sheet.
//!
//! - [`Cage`] ([`cage`]): control vertices with stable ids, faces as
//!   vertex loops (counter-clockwise seen from outside), creased edges
//!   with a sharpness, corner vertices and an optional symmetry plane.
//!   With a symmetry plane the cage stores one half and the other is its
//!   mirror image: the mirrored half's vertices and faces have the
//!   negative ids of those they mirror ([`Full`]).
//! - [`edit`]: moves, rotations and scalings of vertices, edges and faces,
//!   extrusion of faces, insertion and removal of edge loops, creases,
//!   corners, removal of faces and the symmetry plane; each one keeps a
//!   symmetric cage symmetric.
//! - [`primitive`]: the cages a Form starts from (a box, a cylinder, a
//!   sphere, a plane, a polygon such as a face of the model).
//! - [`mesh`]: Catmull-Clark subdivision (semi-sharp creases after
//!   DeRose, Kass and Truong 1998; boundaries as sharp creases) and the
//!   limit positions of the vertices.
//! - [`patch`]: the limit surface as patches, one for each quad of the
//!   once-subdivided cage (a cage face of `n` corners gives `n`): where the
//!   quad is regular the patch is the bicubic B-spline that is the limit
//!   surface there exactly (as a Bézier patch); elsewhere (beside an
//!   extraordinary vertex, a crease, a corner or a boundary) it is the
//!   bicubic B-spline that interpolates exact limit points on a regular grid
//!   of the quad. Neighbouring patches share their boundary curves (see
//!   `patch`), so the kernel sews them without gaps.
//! - [`preview`]: a triangle mesh of the limit surface for drawing while
//!   the cage is edited, without the kernel.
//!
//! Lengths are millimetres in the coordinates of the Form's component.

pub mod cage;
pub mod edit;
pub mod interp;
pub mod mesh;
// Faces of a net from a file turned to run one way (the imports).
pub mod orient;
pub mod patch;
pub mod preview;
pub mod primitive;

pub use cage::{Cage, CageFace, CageVertex, Crease, Full, ON_PLANE, SymmetryPlane};
pub use patch::{LimitSurface, Patch, limit_surface};
pub use preview::{LimitMesh, limit_mesh};

// SPDX-License-Identifier: MIT
//! DXF reader and writer for Mitcad's 2D sketches.
//!
//! Open source OCCT has no DXF support, so sketch import and export go
//! through this crate. It reads ASCII DXF files from R12 to R2018 and writes
//! R12 or R2000 files. Geometry is converted to neutral 2D entity types
//! ([`Geometry`]) that the sketch model maps to its own entities:
//!
//! - lines, circular arcs, circles, elliptical arcs and ellipses, NURBS
//!   splines (control points, knots, weights or fit points), points and text;
//! - polylines (`LWPOLYLINE`, `POLYLINE`) are split into lines and arcs, a
//!   bulge giving an arc;
//! - block references (`INSERT`, also arrays) are expanded with their
//!   transforms, so a non-uniformly scaled circle becomes an ellipse;
//! - entities whose object coordinate system is not the XY plane are
//!   projected onto it.
//!
//! Coordinates stay in drawing units ([`Drawing::units`], from `$INSUNITS`);
//! [`Drawing::to_millimeters`] converts them. Angles are radians,
//! counter-clockwise. Layers are kept as metadata.
//!
//! ```
//! let text = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n5\n0\nENDSEC\n0\nEOF\n";
//! let drawing = mitcad_dxf::read_str(text).unwrap();
//! assert_eq!(drawing.entities.len(), 1);
//! let out = mitcad_dxf::write_string(&drawing, &mitcad_dxf::WriteOptions::default()).unwrap();
//! assert!(out.contains("AC1015"));
//! ```

mod error;
mod math;
mod reader;
mod text;
mod types;
mod units;
mod writer;

pub use error::Error;
pub use math::{Affine, bulge_arc};
pub use reader::{read, read_file, read_str};
pub use types::{Drawing, Entity, Geometry, Layer, Point2, Spline, Text};
pub use units::Units;
pub use writer::{DxfVersion, WriteOptions, write_file, write_string};

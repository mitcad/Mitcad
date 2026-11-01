// SPDX-License-Identifier: MIT
//! A body's print bed face (mitcad#731): the planar face that lies on the
//! print bed, kept with the body's other attributes (`bodies.rs`) by its
//! topological name as it was assigned. It is found at the marker as a face
//! appearance's face is, so it follows the body's edits, and it applies to
//! every occurrence of the body. Setting and clearing it is an undo step
//! that recomputes nothing.
//!
//! The 3D print export turns the bodies onto it (`exchange/bed.rs`), and
//! Print Analysis starts from it. A face that is gone or no longer planar
//! stays kept (an undo or another edit may bring it back): the `bodies`
//! query and the export say why it cannot be used. So does a surface
//! body's (one that became a surface body, or stored so in a file): it is
//! not printed, and the export leaves it out (mitcad#737).

use std::str::FromStr;

use serde_json::{Value, json};

use super::{Document, ModelError, invalid};
use crate::datum::DatumPlane;
use crate::exchange::BodyKind;
use crate::features::geom_ref::GeomRef;
use crate::features::renamed::faces_found;
use crate::ids::BodyUid;
use crate::kernel::{FaceInfo, Kernel, KernelError};
use crate::topo::FaceName;

/// Why a print bed face whose face the body no longer has cannot be used.
pub(crate) const GONE: &str = "the print bed face is gone";
/// Why a print bed face that is no longer planar cannot be used.
pub(crate) const NOT_PLANAR: &str = "the print bed face is not planar";
/// Why the print bed face of a body that is not printed (a surface body,
/// or a body that became one, or one stored so in a file) is not used.
pub(crate) const NOT_PRINTED: &str = "a surface body is not printed";

/// A body's print bed face at the timeline marker.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintBedFace {
    /// The face's name as it was assigned.
    pub face: String,
    /// A name of each face of the body the assigned name finds now (its
    /// own, or that of the face renamed since); empty when it is gone.
    pub faces: Vec<String>,
    /// Its plane in the body's own coordinates (the normal the face's
    /// outward one), or why it cannot lie on the bed.
    pub plane: Result<DatumPlane, String>,
}

/// A face's names that are face names.
fn face_names(face: &FaceInfo) -> Vec<FaceName> {
    face.names
        .iter()
        .filter_map(|n| FaceName::from_str(n).ok())
        .collect()
}

impl<K: Kernel> Document<K> {
    /// A body's print bed face at the marker, or None when it has none.
    pub fn print_bed_face(&self, uid: BodyUid) -> Option<PrintBedFace> {
        let attributes = self.state.body_attributes.get(&uid)?;
        let face = attributes.print_bed_face.clone()?;
        let (faces, mut found) = self.find_print_bed_face(uid, &face);
        // A body that is not printed keeps its face (it may become a solid
        // again), which is not used meanwhile (mitcad#737).
        if found.is_ok() && !self.printed_body(uid) {
            found = Err(NOT_PRINTED.to_owned());
        }
        Some(PrintBedFace {
            face,
            faces,
            plane: found,
        })
    }

    /// The faces of the body a print bed face's name finds at the marker,
    /// and its plane, or why it cannot lie on the bed.
    fn find_print_bed_face(
        &self,
        uid: BodyUid,
        face: &str,
    ) -> (Vec<String>, Result<DatumPlane, String>) {
        let Some(shape) = self.body_shape(uid) else {
            let missing = format!("body {uid} does not exist at the timeline marker");
            return (Vec::new(), Err(missing));
        };
        let Ok(name) = FaceName::from_str(face) else {
            return (Vec::new(), Err(GONE.to_owned()));
        };
        let reference = GeomRef::Face {
            body: uid,
            face: name.clone(),
        };
        let Ok(listed) = self.kernel().faces(shape) else {
            // A kernel that cannot list the faces: the plane alone.
            return (Vec::new(), self.resolve_plane(&reference));
        };
        let names: Vec<Vec<FaceName>> = listed.iter().map(face_names).collect();
        let found = faces_found(&names, &name);
        if found.is_empty() {
            return (Vec::new(), Err(GONE.to_owned()));
        }
        let faces: Vec<String> = found.iter().map(|(_, n)| n.to_string()).collect();
        // The face is there: a surface that is no plane is not planar; a
        // plane that cannot be resolved says why itself (a renamed face
        // that two faces fit alike).
        let planar = found.iter().all(|(i, _)| listed[*i].surface == "plane");
        let resolved = self.resolve_plane(&reference);
        let plane = if planar {
            resolved
        } else {
            resolved.map_err(|_| NOT_PLANAR.to_owned())
        };
        (faces, plane)
    }

    /// Sets the face of a body that lies on the print bed, a planar face of
    /// the body at the marker by its topological name (a body has one: it
    /// replaces another), or with None takes it away: one undo step, which
    /// recomputes nothing, unless nothing changes.
    pub fn set_print_bed_face(
        &mut self,
        uid: BodyUid,
        face: Option<&str>,
    ) -> Result<(), ModelError> {
        let assigned = face
            .map(|face| self.check_print_bed_face(uid, face))
            .transpose()?;
        let verb = if assigned.is_some() {
            "Set Print Bed Face of"
        } else {
            "Clear Print Bed Face of"
        };
        let mut attributes = self.body_attributes(uid);
        attributes.print_bed_face = assigned;
        self.set_attributes(uid, verb, attributes)
    }

    /// The canonical name of a face that can lie on the bed, or why not: it
    /// must be a planar face of a body at the marker that is printed (a
    /// solid or a mesh body).
    fn check_print_bed_face(&self, uid: BodyUid, face: &str) -> Result<String, ModelError> {
        let shape = self
            .body_shape(uid)
            .ok_or_else(|| invalid(format!("body {uid} does not exist at the timeline marker")))?;
        if !self.printed_body(uid) {
            return Err(invalid(format!(
                "{} is no solid or mesh body: it is not printed",
                self.body_name(uid)
            )));
        }
        let name = FaceName::from_str(face.trim())
            .map_err(|e| invalid(format!("{face} is no face name: {e}")))?;
        match self.kernel().count_faces(shape, &name) {
            Ok(0) => {
                return Err(invalid(format!(
                    "{} has no face {name} at the timeline marker",
                    self.body_name(uid)
                )));
            }
            Ok(_) | Err(KernelError::Unsupported(_)) => {}
            Err(e) => return Err(invalid(e.to_string())),
        }
        let canonical = name.to_string();
        let reference = GeomRef::Face {
            body: uid,
            face: name,
        };
        let plane = self
            .resolve_plane(&reference)
            .map_err(|e| invalid(format!("the print bed face must be a planar face: {e}")));
        plane.map(|_| canonical)
    }

    /// Whether a body at the marker is printed: a solid or a mesh body (a
    /// kernel that cannot tell takes every body).
    pub(crate) fn printed_body(&self, uid: BodyUid) -> bool {
        let Some(shape) = self.body_shape(uid) else {
            return false;
        };
        match self.kernel().body_kind(shape) {
            Ok(kind) => matches!(kind, BodyKind::Solid | BodyKind::Mesh),
            Err(_) => true,
        }
    }

    /// A body's print bed face in the `bodies` query: `face` (as assigned),
    /// `faces` (those it finds at the marker) and `error` (why it cannot
    /// lie on the bed, when it cannot); None without one.
    pub fn print_bed_face_json(&self, uid: BodyUid) -> Option<Value> {
        let bed = self.print_bed_face(uid)?;
        let mut value = json!({"face": bed.face, "faces": bed.faces});
        if let Err(error) = &bed.plane {
            value["error"] = json!(error);
        }
        Some(value)
    }
}

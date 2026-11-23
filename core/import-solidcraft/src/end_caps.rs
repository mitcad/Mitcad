// SPDX-License-Identifier: MIT
//! Frame members' end caps (`frame_end_cap`, formats 52 and later;
//! mitcad#2243): one MitCad `end_cap` feature per end, on the planar end
//! face of the member's body that lies on the end's plane. The plate is
//! the member's outer section as the end face's boundary gives it (the
//! rectangle round it with its corner rounds), `thickness` thick, set in
//! by `offset` (or `ratio` times the thickness), its corners chamfered by
//! `chamfer`, outward past the end or inward with the member cut back.
//! Each value is the file's input (its parameter); the plate is a body of
//! its own named `<member> End Cap`, as SolidCraft names it.

use mitcad_model::{ComponentUid, Kernel};
use mitcad_solidcraft::Feature;
use mitcad_solidcraft::kinds::PlaneRef;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Importer, MapResult, Mapped, NotMapped};
use mitcad_import_base::geom::{cross, dot, sub, unit};

/// A cap's end in the file: the end's plane and the member line it closes.
#[derive(Debug, Deserialize)]
struct CapEnd {
    plane: PlaneRef,
    member: CapMember,
}

#[derive(Debug, Deserialize)]
struct CapMember {
    feature: u64,
    line: String,
}

/// The fields of a `frame_end_cap` this import reads (its values are read
/// as inputs).
#[derive(Debug, Deserialize)]
struct EndCap {
    #[serde(default)]
    ends: Vec<CapEnd>,
    #[serde(default)]
    offset: Option<Value>,
    #[serde(default)]
    ratio: Option<Value>,
    #[serde(default)]
    chamfer: Option<Value>,
    #[serde(default)]
    direction: Option<String>,
}

/// How far a face may lie off the end's plane and still be on it, mm.
const ON_PLANE: f64 = 1e-3;

impl<K: Kernel> Importer<'_, '_, K> {
    /// A frame member's end caps (mitcad#2243): an `end_cap` per end.
    pub(super) fn frame_end_cap(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let cap: EndCap = f.typed()?;
        if cap.ends.is_empty() {
            return Err(NotMapped::Failed(
                "the end cap closes no member's end".to_owned(),
            ));
        }
        let direction = match cap.direction.as_deref() {
            None | Some("outward") => "outward",
            Some("inward") => "inward",
            Some(other) => {
                return Err(NotMapped::Failed(format!(
                    "its direction `{other}` is neither outward nor inward"
                )));
            }
        };
        let thickness = self.slot(f, "/thickness")?;
        let inset = match (&cap.offset, &cap.ratio) {
            (Some(_), _) => Some(("inset", self.slot(f, "/offset")?)),
            (None, Some(_)) => Some(("ratio", self.slot(f, "/ratio")?)),
            (None, None) => None,
        };
        let chamfer = match &cap.chamfer {
            Some(_) => Some(self.slot(f, "/chamfer")?),
            None => None,
        };
        let mut notes = Vec::new();
        let mut failed = Vec::new();
        let mut made = 0;
        for (k, end) in cap.ends.iter().enumerate() {
            let pointer = format!("/ends/{k}/plane");
            let result = self.end_cap_face(f, end, &pointer, component, &mut notes);
            let (body, face, member) = match result {
                Ok(found) => found,
                Err(e) => {
                    failed.push(format!(
                        "the cap on line {} of the member did not come in: {e}",
                        end.member.line
                    ));
                    continue;
                }
            };
            let mut def = json!({
                "type": "end_cap",
                "face": {"body": body, "face": face},
                "thickness": thickness,
            });
            if let Some((key, value)) = &inset {
                def[*key] = json!(value);
            }
            if let Some(chamfer) = &chamfer {
                def["chamfer"] = json!(chamfer);
            }
            if direction == "inward" {
                def["direction"] = json!("inward");
            }
            let name = if k == 0 {
                f.name.clone()
            } else {
                format!("{} ({})", f.name, k + 1)
            };
            self.member_body = Some(self.unique_body_name(&format!("{member} End Cap")));
            let added = self.add(&def, Some(&name), component);
            self.member_body = None;
            match added {
                Ok(_) => made += 1,
                Err(e) => failed.push(format!(
                    "the cap on line {} of the member did not come in: {e}",
                    end.member.line
                )),
            }
        }
        if made == 0 {
            return Err(NotMapped::Failed(failed.join("; ")));
        }
        notes.extend(failed);
        Ok(Mapped::partial(notes))
    }

    /// The member body's planar face on an end's plane, facing out along
    /// it: its body and face names, and the member body's file name.
    fn end_cap_face(
        &mut self,
        f: &Feature,
        end: &CapEnd,
        pointer: &str,
        component: ComponentUid,
        notes: &mut Vec<String>,
    ) -> Result<(String, String, String), String> {
        let member = self
            .member_lines
            .get(&(end.member.feature, end.member.line.clone()))
            .cloned()
            .ok_or("its member did not come in")?;
        let uid = self.body(&member)?;
        let geom = self.feature_plane(f, &end.plane, pointer, component, notes)?;
        let plane = self.mitcad_frame(&geom)?;
        let normal = unit(cross(plane.x, plane.y)).ok_or("the end's plane has no normal")?;
        let shape = self
            .doc
            .body_shape(uid)
            .ok_or("its member has no body")?
            .clone();
        let kernel = self.doc.kernel();
        let faces = kernel.faces(&shape).map_err(|e| e.to_string())?;
        for info in faces {
            for text in &info.names {
                let Ok(name) = text.parse::<mitcad_model::FaceName>() else {
                    continue;
                };
                let Ok(mitcad_model::datum::SurfaceGeometry::Plane { origin, normal: n }) =
                    kernel.face_geometry(&shape, &name)
                else {
                    continue;
                };
                let Some(n) = unit(n) else { continue };
                if dot(n, normal).abs() > 1.0 - 1e-6
                    && dot(sub(origin, plane.o), normal).abs() <= ON_PLANE
                {
                    return Ok((uid.to_string(), name.to_string(), member));
                }
            }
        }
        Err("its member has no planar end face on the end's plane".to_owned())
    }
}

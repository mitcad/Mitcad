// SPDX-License-Identifier: MIT
//! Shell: hollows one body. The removed faces become openings; without any
//! the body gets a closed void. The wall runs `inside` into the body and
//! `outside` out of it from its surface (one may be left out). With
//! `tangent_chain` (the default) the faces tangent to a removed face go
//! too; `rounded` rounds the outer corners (a rounded offset), else they
//! stay sharp.
//!
//! The outside of the wall keeps the body's face names; the inside is
//! `<shell>:offset(<face>)` and the end of the wall where a removed face was
//! `<shell>:offset_cap(<face>)`.
//!
//! Parts of the body no thicker than the wall stay solid with their own
//! faces, and the feature warns, naming the faces the wall does not fit
//! under (and the roundings it runs past); where nothing can be hollowed it
//! fails with that message (mitcad#1233).
//!
//! `face_thicknesses` give faces a wall thickness of their own
//! (mitcad#2443): each entry's faces get its `inside` and `outside` in
//! place of the shell's, a side it leaves out keeps the shell's. The kernel
//! offsets each face by its own thickness; faces that meet smoothly (and in
//! a rounded shell, faces a rounding joins) take one thickness: with the
//! tangent chain the faces tangent to one take its thickness, and a face
//! joined so to one of another thickness is refused, both painted. Where
//! the walls around a face cannot meet its own thickness, the shell fails
//! painting that face.

use serde::{Deserialize, Serialize};

use super::face_refs::{add_faces, check_faces, kernel_error, require_faces};
use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::BodyUid;
use crate::kernel::{Kernel, ShellFace, ShellSpec};
use crate::parameters::ParamId;
use crate::topo::FaceName;

/// Faces whose wall has a thickness of its own (mitcad#2443): `inside`
/// and `outside` in place of the shell's; a side left out keeps the
/// shell's. Only the sides the shell's wall has may be given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct ShellFaceThickness<P = ParamId> {
    pub faces: Vec<FaceName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inside: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outside: Option<P>,
}

/// The entries' values, each its own slots: `face_thicknesses[i].own_inside`
/// and `.own_outside`, so that their parameters read "Shell1 own_inside"
/// beside the shell's "Shell1 inside" (the label is the last part).
fn map_face_thicknesses<P, Q, E>(
    entries: &[ShellFaceThickness<P>],
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<Vec<ShellFaceThickness<Q>>, E> {
    entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            Ok(ShellFaceThickness {
                faces: entry.faces.clone(),
                inside: entry
                    .inside
                    .as_ref()
                    .map(|value| f(&format!("face_thicknesses[{i}].own_inside"), value))
                    .transpose()?,
                outside: entry
                    .outside
                    .as_ref()
                    .map(|value| f(&format!("face_thicknesses[{i}].own_outside"), value))
                    .transpose()?,
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct ShellDef<P = ParamId> {
    pub body: BodyUid,
    /// The faces to remove; none for a closed void.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<FaceName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inside: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outside: Option<P>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub tangent_chain: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub rounded: bool,
    /// Faces with a wall thickness of their own (mitcad#2443).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub face_thicknesses: Vec<ShellFaceThickness<P>>,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl<P> ShellDef<P> {
    pub const TYPE: &'static str = "shell";
    pub const BASE_NAME: &'static str = "Shell";

    map_params! {
        ShellDef {
            body: copy,
            faces: clone,
            inside: option,
            outside: option,
            tangent_chain: copy,
            rounded: copy,
            face_thicknesses: with(map_face_thicknesses),
        }
    }
}

impl FeatureInfo for ShellDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        add_faces(&mut references, &self.faces);
        for entry in &self.face_thicknesses {
            add_faces(&mut references, &entry.faces);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        if !self.faces.is_empty() {
            check_faces(ctx, &self.faces, "faces to remove")?;
        }
        if self.inside.is_none() && self.outside.is_none() {
            return Err("the shell needs an inside or an outside thickness".to_owned());
        }
        let mut listed: Vec<&FaceName> = Vec::new();
        for entry in &self.face_thicknesses {
            check_faces(ctx, &entry.faces, "faces with their own thickness")?;
            if entry.inside.is_none() && entry.outside.is_none() {
                return Err(face_error(
                    &entry.faces[0],
                    "faces with their own thickness need an inside or an outside thickness",
                ));
            }
            if (entry.inside.is_some() && self.inside.is_none())
                || (entry.outside.is_some() && self.outside.is_none())
            {
                return Err(face_error(&entry.faces[0], OWN_SIDES));
            }
            for face in &entry.faces {
                if self.faces.contains(face) {
                    return Err(face_error(
                        face,
                        "a face the shell removes cannot have its own thickness",
                    ));
                }
                if listed.contains(&face) {
                    return Err(face_error(
                        face,
                        "a face is given its own thickness more than once",
                    ));
                }
                listed.push(face);
            }
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let inside = self.inside.map_or(0.0, value);
        let outside = self.outside.map_or(0.0, value);
        thicknesses(inside, outside)?;
        for entry in &self.face_thicknesses {
            for own in [entry.inside, entry.outside].into_iter().flatten() {
                let own = value(own);
                if !(own > 0.0 && own.is_finite()) {
                    return Err(face_error(
                        &entry.faces[0],
                        &format!("a face's own thickness must be greater than zero, got {own}"),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// What a face's own thickness may be given for.
const OWN_SIDES: &str = "a face's own thickness can be given only on the sides of the shell's wall: \
     inside, outside or both, as the shell's thickness";

/// An error about a face of an entry, the face painted.
fn face_error(face: &FaceName, said: &str) -> String {
    crate::notice::placed(
        said,
        crate::notice::NoticePlace::face(face.to_string()),
        format!("face {face}: {said}"),
    )
}

/// An entry's thickness on one side of the wall: its own `given`, or the
/// shell's `shell` when it gives none. Refused on a side the shell's wall
/// has not (its thickness 0).
fn own_side<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    entry: &ShellFaceThickness,
    given: Option<ParamId>,
    shell: f64,
) -> Result<f64, String> {
    match given {
        Some(_) if shell == 0.0 => Err(face_error(&entry.faces[0], OWN_SIDES)),
        Some(id) => ctx.param(id),
        None => Ok(shell),
    }
}

fn thicknesses(inside: f64, outside: f64) -> Result<(), String> {
    if !(inside >= 0.0 && outside >= 0.0) {
        Err(format!(
            "shell thicknesses must not be negative, got {inside} inside and {outside} outside"
        ))
    } else if inside == 0.0 && outside == 0.0 {
        Err("the shell needs an inside or an outside thickness greater than zero".to_owned())
    } else {
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for ShellDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let inside = match self.inside {
            Some(id) => ctx.param(id)?,
            None => 0.0,
        };
        let outside = match self.outside {
            Some(id) => ctx.param(id)?,
            None => 0.0,
        };
        thicknesses(inside, outside)?;
        let shape = ctx.surface_body(self.body)?;
        let faces = require_faces(ctx, self.body, &shape, &self.faces)?;
        // Each face with its own thickness, a side left out at the shell's
        // (mitcad#2443).
        let mut own = Vec::new();
        for entry in &self.face_thicknesses {
            let own_inside = own_side(ctx, entry, entry.inside, inside)?;
            let own_outside = own_side(ctx, entry, entry.outside, outside)?;
            for face in require_faces(ctx, self.body, &shape, &entry.faces)? {
                own.push(ShellFace {
                    face,
                    inside: own_inside,
                    outside: own_outside,
                });
            }
        }
        let spec = ShellSpec {
            faces: &faces,
            inside,
            outside,
            tangent_chain: self.tangent_chain,
            rounded: self.rounded,
            own: &own,
        };
        let hollow = ctx
            .kernel
            .shell(ctx.uid, &shape, &spec)
            .map_err(kernel_error)?;
        // Where the wall does not fit (parts no thicker than it, left solid)
        // or runs past faces too small for it to follow (mitcad#1233).
        ctx.warn_notes(&hollow);
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, hollow)],
            ..FeatureOutput::default()
        })
    }
}

#[cfg(test)]
mod tests {
    //! Faces with their own thickness (mitcad#2443) with the mock kernel:
    //! what reaches the kernel, their parameters, and the refusals.

    use serde_json::{Value, json};

    use crate::FeatureStatus;
    use crate::test_support::{Block, block, def, side};
    use crate::topo::FaceName;

    fn top(b: &Block) -> FaceName {
        FaceName::end(b.extrude, b.region.clone())
    }

    fn bottom(b: &Block) -> FaceName {
        FaceName::start(b.extrude, b.region.clone())
    }

    /// Adds a feature; the definition is built before the document is
    /// borrowed.
    macro_rules! add {
        ($b:ident, $value:expr $(,)?) => {{
            let value: Value = $value;
            $b.doc
                .add_feature(&def(value), None)
                .map(|added| added.uid)
                .map_err(|e| e.to_string())
        }};
    }

    fn history(b: &Block) -> String {
        b.doc
            .body_shape(b.body)
            .map_or_else(|| "none".to_owned(), |s| s.history.clone())
    }

    #[test]
    fn faces_with_their_own_thickness_reach_the_kernel() {
        let mut b = block();
        let shell = add!(
            b,
            json!({"type": "shell", "body": b.body, "faces": [top(&b)], "inside": 2.0,
                   "face_thicknesses": [{"faces": [bottom(&b)], "inside": 5.0}]}),
        )
        .unwrap();
        assert_eq!(b.doc.status(shell), Some(&FeatureStatus::Ok));
        let history = history(&b);
        assert!(
            history.ends_with(&format!(",2,0,own={}:5/0)", bottom(&b))),
            "{history}"
        );
        // Its own parameter, after the shell's.
        let names: Vec<String> = b
            .doc
            .parameters()
            .iter()
            .map(|p| p.comment().to_owned())
            .collect();
        assert!(names.contains(&"Shell1 inside".to_owned()), "{names:?}");
        assert!(names.contains(&"Shell1 own_inside".to_owned()), "{names:?}");
    }

    #[test]
    fn a_side_an_entry_leaves_out_keeps_the_shells() {
        let mut b = block();
        let front = side(b.extrude, 1, 0);
        add!(
            b,
            json!({"type": "shell", "body": b.body, "faces": [top(&b)],
                   "inside": 1.0, "outside": 1.5,
                   "face_thicknesses": [
                       {"faces": [bottom(&b)], "inside": 3.0},
                       {"faces": [front], "outside": 2.0}]}),
        )
        .unwrap();
        let history = history(&b);
        assert!(
            history.ends_with(&format!(
                ",1,1.5,own={}:3/1.5;{}:1/2)",
                bottom(&b),
                side(b.extrude, 1, 0)
            )),
            "{history}"
        );
    }

    #[test]
    fn wrong_entries_are_refused() {
        let mut b = block();
        let front = side(b.extrude, 1, 0);
        for (entries, said) in [
            (
                json!([{"faces": [bottom(&b)]}]),
                "need an inside or an outside thickness",
            ),
            (
                json!([{"faces": [bottom(&b)], "outside": 3.0}]),
                "only on the sides of the shell's wall",
            ),
            (
                json!([{"faces": [top(&b)], "inside": 3.0}]),
                "a face the shell removes cannot have its own thickness",
            ),
            (
                json!([{"faces": [front.clone()], "inside": 3.0},
                       {"faces": [front.clone()], "inside": 4.0}]),
                "more than once",
            ),
            (
                json!([{"faces": [bottom(&b)], "inside": 0.0}]),
                "must be greater than zero",
            ),
            (
                json!([{"faces": [], "inside": 3.0}]),
                "no faces with their own thickness selected",
            ),
        ] {
            let error = add!(
                b,
                json!({"type": "shell", "body": b.body, "faces": [top(&b)], "inside": 2.0,
                       "face_thicknesses": entries}),
            )
            .unwrap_err();
            assert!(error.contains(said), "{said}: {error}");
        }
    }

    /// An own inside thickness on a shell whose inside is 0 fails when it
    /// is computed, the face painted.
    #[test]
    fn a_side_the_wall_has_not_fails() {
        let mut b = block();
        let shell = add!(
            b,
            json!({"type": "shell", "body": b.body, "faces": [top(&b)],
                   "inside": 0.0, "outside": 2.0,
                   "face_thicknesses": [{"faces": [bottom(&b)], "inside": 3.0}]}),
        )
        .unwrap();
        let Some(FeatureStatus::Failed(message)) = b.doc.status(shell).cloned() else {
            panic!("expected a failure");
        };
        let said = message.agent_text();
        assert!(said.contains("sides of the shell's wall"), "{said}");
    }
}

// SPDX-License-Identifier: MIT
//! Drafts (mitcad#1363): a `DraftFeature` as MitCad's `draft`, one per
//! body of its drafted faces, about the neutral plane the decoder read.
//!
//! The pull direction: the file pulls a face draft along the neutral
//! face's outward normal and a plane draft along the plane's normal, a
//! positive angle removing material on the pull side as MitCad's does (the
//! controlled model, a side face drafted 5 degrees about the bottom face,
//! grows by the wedge: the material is on the other side). MitCad pulls
//! from a face into its material and along a plane's normal, so a face
//! draft is `flip` and a plane draft is not; the other way is tried after
//! it where the history checks the item. What the file does not settle comes as
//! guesses only the history can confirm: a second angle (whether the
//! draft is two-sided or symmetric is not decoded) and a parting-line
//! draft, whose faces turn about the line where they meet the parting
//! tool: MitCad's draft turns them about one plane, which is the same only
//! where the tool is one planar face square to the pull direction.

use mitcad_dump::{DraftDetail, Reference};
use mitcad_model::BodyUid;
use serde_json::{Value, json};

use crate::{Candidate, Importer, InputProvenance};

/// What the replay draws from a draft's neutral plane.
struct Neutral {
    /// The plane as a MitCad reference: a face of a body or a plane.
    plane: Value,
    /// A face (the pull direction from MitCad's side is `flip`).
    face: bool,
    /// A parting-line draft's single planar parting face.
    parting: bool,
}

impl<K: crate::ImportKernel> Importer<'_, K> {
    pub(crate) fn draft(&self, d: &DraftDetail) -> Result<Vec<Candidate>, String> {
        let faces = d.input_faces.as_deref().unwrap_or_default();
        if faces.is_empty() {
            return Err("a draft: its drafted faces were not decoded".to_owned());
        }
        let angle_ref = d.draft_definition.as_ref().and_then(|x| x.angle.clone());
        let angle_value = angle_ref
            .as_ref()
            .and_then(Reference::parameter)
            .and_then(|p| p.value)
            .ok_or("a draft: its angle was not decoded")?;
        if angle_value == 0.0 {
            return Err(
                "a draft of angle zero leaves its faces as they are: MitCad's draft needs an angle"
                    .to_owned(),
            );
        }
        let angle = self
            .value(&angle_ref)
            .ok_or("a draft: its angle was not decoded")?;
        let angle_two = d
            .angle_two
            .as_ref()
            .filter(|r| {
                r.parameter()
                    .and_then(|p| p.value)
                    .is_some_and(|v| v != 0.0)
            })
            .and_then(|r| self.value(&Some(r.clone())));
        let mut by_body: Vec<(BodyUid, Vec<String>)> = Vec::new();
        for r in faces {
            let Reference::Face(fp) = r else {
                return Err("a draft: a drafted face is not a face".to_owned());
            };
            let (b, f) = self
                .face_input(fp)
                .ok_or("a draft: a drafted face was not found")?;
            match by_body.iter_mut().find(|(x, _)| *x == b) {
                Some((_, list)) => list.push(f.to_string()),
                None => by_body.push((b, vec![f.to_string()])),
            }
        }
        let neutral = self.draft_neutral(d)?;
        let chain = d.is_tangent_chain.unwrap_or(true);
        let defs = |flip: bool, chain: bool, two: Option<&Value>, symmetric: bool| {
            by_body
                .iter()
                .map(|(b, f)| {
                    let mut v = json!({"type": "draft", "body": b.to_string(), "faces": f,
                        "plane": neutral.plane, "angle": angle, "flip": flip,
                        "tangent_chain": chain});
                    if let Some(a) = two {
                        v["angle2"] = a.clone();
                    }
                    if symmetric {
                        v["symmetric"] = json!(true);
                    }
                    v
                })
                .collect::<Vec<Value>>()
        };
        let candidate = |defs: Vec<Value>, guess: bool, note: Option<&str>| {
            let mut c = Candidate::new(Value::Null);
            c.defs = defs;
            c.guess = guess;
            c.note = note.map(str::to_owned);
            c.input_provenance = InputProvenance::Inferred;
            c
        };
        let flip = neutral.face;
        // A parting line, or a second angle whose sides option is not
        // decoded: guesses only.
        let unsettled = neutral.parting || angle_two.is_some();
        let parting_note = neutral.parting.then_some(
            "a parting-line draft about its parting face (the same where the face is planar and \
             square to the pull direction)",
        );
        let mut out = Vec::new();
        let mut first = candidate(
            defs(flip, chain, angle_two.as_ref(), false),
            unsettled,
            parting_note.or(angle_two
                .as_ref()
                .map(|_| "two-sided: its second angle on the other side of the plane")),
        );
        if !unsettled && d.is_tangent_chain.is_some() {
            // The faces, the plane, the angle and the tangent chain are
            // decoded fields; the pull direction follows from the plane
            // (the controlled model).
            first.input_provenance = InputProvenance::Read;
        }
        out.push(first);
        if angle_two.is_some() {
            out.push(candidate(
                defs(flip, chain, None, true),
                true,
                Some("symmetric about the plane"),
            ));
            out.push(candidate(
                defs(flip, chain, None, false),
                true,
                Some("one-sided: its second angle not used"),
            ));
        }
        out.push(candidate(
            defs(!flip, chain, angle_two.as_ref(), false),
            true,
            Some("pulled the other way"),
        ));
        if !chain {
            out.push(candidate(
                defs(flip, true, angle_two.as_ref(), false),
                true,
                Some("with the faces tangent to the drafted ones"),
            ));
        }
        Ok(out)
    }

    /// The plane a draft's faces turn about: its neutral face or plane, or
    /// a parting-line draft's single parting face.
    fn draft_neutral(&self, d: &DraftDetail) -> Result<Neutral, String> {
        if let Some(p) = &d.plane {
            let plane = self
                .plane_ref(p)
                .ok_or("a draft: its neutral plane was not found")?;
            return Ok(Neutral {
                plane,
                face: matches!(p, Reference::Face(_)),
                parting: false,
            });
        }
        if let Some(faces) = &d.neutral_faces {
            return Err(format!(
                "a draft about {} neutral faces: MitCad's draft turns faces about one plane",
                faces.len()
            ));
        }
        if let Some(tool) = &d.parting_tool {
            return match tool.as_slice() {
                [face @ Reference::Face(_)] => Ok(Neutral {
                    plane: self
                        .plane_ref(face)
                        .ok_or("a parting-line draft: its parting face was not found")?,
                    face: true,
                    parting: true,
                }),
                _ => Err(format!(
                    "a parting-line draft with a parting tool of {} faces: MitCad's draft turns \
                     faces about one plane (parting-line drafts are not modelled)",
                    tool.len()
                )),
            };
        }
        Err("a draft: its neutral plane was not decoded".to_owned())
    }
}

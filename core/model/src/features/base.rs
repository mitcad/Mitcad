// SPDX-License-Identifier: MIT
//! Base feature: bodies without history, stored as
//! B-rep data in the project. Imported files (STEP, IGES, BRep, STL, OBJ)
//! and the bodies of .f3d files become base features, and the .f3d
//! importer replays a feature it cannot rebuild as one (the fallback).
//!
//! The bodies become new bodies (`<feature>.b<i>` for body i), or join, cut
//! or intersect participant bodies like an extrusion's tool. Face j of the
//! feature is named `<feature>:import(<j>)`, counting the faces of the
//! bodies in order (body 0 first, each body's faces in the kernel's face
//! order), so the names stay the same as long as the data does.
//!
//! The data is OCCT's binary B-rep format, kept in memory as it is and
//! written to single project files and commands zlib-compressed and
//! base64-encoded, or to the store of a project (version 3, P12a) and
//! referred to by its SHA-256 (see [`Brep`]).

use std::fmt;
use std::marker::PhantomData;
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, Operation,
    References, is_false,
};
use crate::ids::BodyUid;
use crate::kernel::{BooleanOp, Kernel};
use crate::parameters::ParamId;
use crate::sha256::Sha256;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaseDef<P = ParamId> {
    pub bodies: Vec<BaseBody>,
    /// New bodies by default; join, cut and intersect work on the
    /// participants, as an extrusion's operation does.
    #[serde(default = "new_body", skip_serializing_if = "is_new_body")]
    pub operation: Operation,
    /// The bodies a join, cut or intersect works on; empty for all bodies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
    /// Where the bodies come from, e.g. the imported file's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// New bodies only: bodies these replace (the .f3d importer's
    /// fallback, T1). Body i takes the id and name of `replaces[i]`, so
    /// later features that use that body work on the new one; replaced
    /// bodies without a new one are removed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaces: Vec<BodyUid>,
    /// A base feature has no dimensions; the value form is unused.
    #[serde(skip)]
    pub values: PhantomData<P>,
}

fn new_body() -> Operation {
    Operation::NewBody
}

fn is_new_body(operation: &Operation) -> bool {
    *operation == Operation::NewBody
}

/// A body of a base feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaseBody {
    /// Its name in the source (a STEP part, an .f3d body); the new body
    /// is named after it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Display colour, sRGB components in [0, 1].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f64; 3]>,
    /// A mesh body (triangles without surfaces, from STL or OBJ): not a
    /// solid, so it can only be a new body.
    #[serde(default, skip_serializing_if = "is_false")]
    pub mesh: bool,
    pub brep: Brep,
}

impl<P> BaseDef<P> {
    pub const TYPE: &'static str = "base";
    pub const BASE_NAME: &'static str = "Base";

    pub fn new(bodies: Vec<BaseBody>) -> Self {
        Self {
            bodies,
            operation: Operation::NewBody,
            participants: Vec::new(),
            source: None,
            replaces: Vec::new(),
            values: PhantomData,
        }
    }

    pub fn map_params<Q, E>(
        &self,
        _f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<BaseDef<Q>, E> {
        Ok(BaseDef {
            bodies: self.bodies.clone(),
            operation: self.operation,
            participants: self.participants.clone(),
            source: self.source.clone(),
            replaces: self.replaces.clone(),
            values: PhantomData,
        })
    }

    /// Writes the definition to a fingerprint (P7d): what its serde form
    /// holds, with each body's B-rep data by the data's own fingerprint.
    pub(crate) fn fingerprint(&self, fingerprint: &mut crate::fingerprint::Fingerprint) {
        fingerprint.u64(self.bodies.len() as u64);
        for body in &self.bodies {
            fingerprint
                .option(body.name.as_deref(), |f, name| {
                    f.str(name);
                })
                .option(body.color, |f, color| {
                    for c in color {
                        f.f64(c);
                    }
                })
                .bool(body.mesh)
                .u128(body.brep.fingerprint());
        }
        let rest = (
            &self.operation,
            &self.participants,
            &self.source,
            &self.replaces,
        );
        serde_json::to_writer(&mut *fingerprint, &rest).expect("definitions serialize");
    }

    /// The id of new body `i`: the body it replaces, else `<feature>.b<i>`.
    fn body_uid(&self, feature: crate::ids::FeatureUid, i: u32) -> BodyUid {
        self.replaces
            .get(i as usize)
            .copied()
            .unwrap_or_else(|| BodyUid::new(feature, i))
    }
}

impl FeatureInfo for BaseDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for body in self.participants.iter().chain(&self.replaces) {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        // Without bodies it only removes the bodies it replaces.
        if self.bodies.is_empty() && self.replaces.is_empty() {
            return Err("a base feature needs at least one body".to_owned());
        }
        for (i, body) in self.bodies.iter().enumerate() {
            if body.name.as_ref().is_some_and(|n| n.trim().is_empty()) {
                return Err(format!("bodies[{i}]: the name is empty"));
            }
            if let Some(color) = body.color
                && !color.iter().all(|c| (0.0..=1.0).contains(c))
            {
                return Err(format!(
                    "bodies[{i}]: colour components must be between 0 and 1, got {color:?}"
                ));
            }
            if body.mesh && self.operation != Operation::NewBody {
                return Err(format!(
                    "bodies[{i}] is a mesh body, which can only be a new body"
                ));
            }
        }
        if self.operation == Operation::NewBody && !self.participants.is_empty() {
            return Err("a new body base feature has no participant bodies".to_owned());
        }
        for (i, body) in self.participants.iter().enumerate() {
            if self.participants[..i].contains(body) {
                return Err(format!("body {body} is listed more than once"));
            }
            ctx.body(*body)?;
        }
        if !self.replaces.is_empty() && self.operation != Operation::NewBody {
            return Err("only new bodies can replace bodies".to_owned());
        }
        for (i, body) in self.replaces.iter().enumerate() {
            if self.replaces[..i].contains(body) {
                return Err(format!("body {body} is replaced more than once"));
            }
            ctx.body(*body)?;
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        true
    }

    fn new_component(&self) -> bool {
        self.operation == Operation::NewComponent
    }
}

impl<K: Kernel> Evaluate<K> for BaseDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let mut shapes = Vec::with_capacity(self.bodies.len());
        let mut first_face = 0;
        for (i, body) in self.bodies.iter().enumerate() {
            let data = body.brep.data().ok_or_else(|| {
                format!(
                    "body {i}: its B-rep data {} is missing from the project store ({})",
                    body.brep.sha256(),
                    crate::file::BREP_DIR
                )
            })?;
            let (shape, faces) = ctx
                .kernel
                .import_brep(ctx.uid, data, first_face)
                .map_err(|e| format!("body {i}: {e}"))?;
            first_face += faces;
            shapes.push(shape);
        }
        let mut output = FeatureOutput::default();
        let op = match self.operation {
            Operation::NewBody | Operation::NewComponent => {
                let count = shapes.len();
                for (i, shape) in (0..).zip(shapes) {
                    output
                        .changes
                        .push(BodyChange::Set(self.body_uid(ctx.uid, i), shape));
                }
                for replaced in self.replaces.iter().skip(count) {
                    // Read it, so the cache key holds the body it removes.
                    ctx.body(*replaced)?;
                    output.changes.push(BodyChange::Remove(*replaced));
                }
                return Ok(output);
            }
            Operation::Join => BooleanOp::Join,
            Operation::Cut => BooleanOp::Cut,
            Operation::Intersect => BooleanOp::Intersect,
        };
        let tool = if shapes.len() == 1 {
            shapes.remove(0)
        } else {
            ctx.kernel.compound(&shapes).map_err(|e| e.to_string())?
        };
        output.tool = Some(tool.clone());
        let participants = if self.participants.is_empty() {
            ctx.bodies()
        } else {
            self.participants
                .iter()
                .map(|uid| ctx.body(*uid).map(|shape| (*uid, shape)))
                .collect::<Result<_, _>>()?
        };
        output.changes = apply_tool(ctx, op, &participants, &tool, "base feature")?;
        Ok(output)
    }
}

/// The body changes of a join, cut or intersect of a tool with participant
/// bodies, as an extrusion makes them: joined bodies merge into the first
/// participant they touch and tool solids that touch nothing become new
/// bodies; a cut or intersection keeps each body for its first piece, makes
/// new bodies of the other pieces and removes a body it leaves nothing of.
/// `what` names the tool in errors ("the base feature does not touch ...").
pub(crate) fn apply_tool<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    op: BooleanOp,
    participants: &[(BodyUid, K::Shape)],
    tool: &K::Shape,
    what: &str,
) -> Result<Vec<BodyChange<K::Shape>>, String> {
    let verb = match op {
        BooleanOp::Join => "join",
        BooleanOp::Cut => "cut",
        BooleanOp::Intersect => "intersect",
    };
    if participants.is_empty() {
        return Err(format!("there is no body to {verb}"));
    }
    let targets: Vec<&K::Shape> = participants.iter().map(|(_, s)| s).collect();
    let result = ctx
        .kernel
        .boolean(op, &targets, tool)
        .map_err(|e| e.to_string())?;
    if !result.touched.iter().any(|t| *t) {
        return Err(match op {
            BooleanOp::Join => format!("the {what} does not touch any participant body"),
            BooleanOp::Cut => format!("the {what} does not cut into any participant body"),
            BooleanOp::Intersect => format!("the {what} does not intersect any participant body"),
        });
    }
    let mut changes = Vec::new();
    match op {
        BooleanOp::Join => {
            for piece in result.pieces {
                match piece.sources.split_first() {
                    None => changes.push(BodyChange::Set(ctx.new_body(), piece.shape)),
                    Some((first, merged)) => {
                        changes.push(BodyChange::Set(participants[*first].0, piece.shape));
                        for i in merged {
                            changes.push(BodyChange::Remove(participants[*i].0));
                        }
                    }
                }
            }
        }
        BooleanOp::Cut | BooleanOp::Intersect => {
            let mut pieces: Vec<Vec<K::Shape>> = vec![Vec::new(); participants.len()];
            for piece in result.pieces {
                if let Some(source) = piece.sources.first() {
                    pieces[*source].push(piece.shape);
                }
            }
            for (i, (uid, _)) in participants.iter().enumerate() {
                if !result.touched[i] {
                    continue;
                }
                let mut own = pieces[i].drain(..);
                match own.next() {
                    None => changes.push(BodyChange::Remove(*uid)),
                    Some(first) => {
                        changes.push(BodyChange::Set(*uid, first));
                        for piece in own {
                            changes.push(BodyChange::Set(ctx.new_body(), piece));
                        }
                    }
                }
            }
        }
    }
    Ok(changes)
}

/// B-rep data of a body: OCCT's binary format (or its text format), as the
/// kernel writes and reads it ([`Kernel::brep_data`],
/// [`Kernel::import_brep`]). Cloning shares the data.
///
/// In single project files (version 2) and commands it is an object with
/// the zlib-compressed data in base64 and its size:
///
/// ```json
/// {"format": "occt", "compression": "zlib", "size": 6146, "data": "eJy1V..."}
/// ```
///
/// `compression` may also be `"none"`. The encoded form is computed once
/// and kept, so saving a project again does not compress again.
///
/// A project file of version 3 (P12a, [`crate::file::project`]) refers to
/// the data in its project's store by the SHA-256 of the uncompressed data;
/// the store's file holds it zlib-compressed:
///
/// ```json
/// {"format": "occt", "compression": "zlib", "size": 6146, "sha256": "3fa9...e1"}
/// ```
///
/// Read, such a B-rep is a *reference* without its data until the store
/// gives it ([`Brep::resolve`]); one whose data the store does not have
/// stays a reference, and its base feature fails to evaluate.
#[derive(Clone)]
pub struct Brep(Arc<BrepData>);

struct BrepData {
    /// None for a reference.
    bytes: Option<Vec<u8>>,
    size: u64,
    /// Of the data; known from the start for a reference.
    sha256: OnceLock<Sha256>,
    encoded: OnceLock<String>,
    /// Of `bytes`, for the definition's fingerprint (P7d).
    fingerprint: OnceLock<u128>,
}

/// Largest B-rep accepted from a file, so a corrupt size cannot exhaust
/// memory: 4 GiB.
const MAX_SIZE: u64 = 1 << 32;

/// The compression level of the stored forms.
const LEVEL: u8 = 6;

impl Brep {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Arc::new(BrepData {
            size: bytes.len() as u64,
            bytes: Some(bytes),
            sha256: OnceLock::new(),
            encoded: OnceLock::new(),
            fingerprint: OnceLock::new(),
        }))
    }

    /// A reference to `size` bytes of data whose SHA-256 is `sha256`, kept
    /// elsewhere (a project's store).
    pub fn reference(sha256: Sha256, size: u64) -> Self {
        Self(Arc::new(BrepData {
            bytes: None,
            size,
            sha256: OnceLock::from(sha256),
            encoded: OnceLock::new(),
            fingerprint: OnceLock::new(),
        }))
    }

    /// The data; None for a reference.
    pub fn data(&self) -> Option<&[u8]> {
        self.0.bytes.as_deref()
    }

    /// Whether this is a reference without its data.
    pub fn is_reference(&self) -> bool {
        self.0.bytes.is_none()
    }

    /// The size of the data in bytes.
    pub fn size(&self) -> u64 {
        self.0.size
    }

    /// The SHA-256 of the data, worked out once.
    pub fn sha256(&self) -> Sha256 {
        *self
            .0
            .sha256
            .get_or_init(|| Sha256::of(self.data().expect("a reference is made with its SHA-256")))
    }

    /// A reference to this data, as a version 3 project file writes it.
    pub fn to_reference(&self) -> Brep {
        if self.is_reference() {
            self.clone()
        } else {
            Brep::reference(self.sha256(), self.size())
        }
    }

    /// The data zlib-compressed, the content of its file in a project's
    /// store; None for a reference.
    pub fn stored(&self) -> Option<Vec<u8>> {
        let data = self.data()?;
        // Read from a single file: its compressed data is at hand.
        if let Some(encoded) = self.0.encoded.get()
            && let Ok(compressed) = crate::base64::decode(encoded)
        {
            return Some(compressed);
        }
        Some(miniz_oxide::deflate::compress_to_vec_zlib(data, LEVEL))
    }

    /// The data of this reference from the content of its store file,
    /// checked against the reference's size and SHA-256.
    pub fn resolve(&self, stored: &[u8]) -> Result<Brep, String> {
        let size = usize::try_from(self.size()).map_err(|_| "too large".to_owned())?;
        let bytes = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(stored, size)
            .map_err(|e| format!("it does not decompress: {:?}", e.status))?;
        if bytes.len() != size {
            return Err(format!("it has {} bytes, expected {size}", bytes.len()));
        }
        let sha256 = Sha256::of(&bytes);
        if sha256 != self.sha256() {
            return Err(format!("its data has the SHA-256 {sha256}"));
        }
        let brep = Brep::new(bytes);
        let _ = brep.0.sha256.set(sha256);
        Ok(brep)
    }

    /// The fingerprint of the data, worked out once.
    pub(crate) fn fingerprint(&self) -> u128 {
        *self.0.fingerprint.get_or_init(|| match self.data() {
            Some(bytes) => crate::fingerprint::of_bytes("brep", bytes),
            None => crate::fingerprint::of_bytes("brep reference", &self.sha256().0),
        })
    }

    fn encoded(&self, data: &[u8]) -> &str {
        self.0.encoded.get_or_init(|| {
            crate::base64::encode(&miniz_oxide::deflate::compress_to_vec_zlib(data, LEVEL))
        })
    }
}

impl PartialEq for Brep {
    fn eq(&self, other: &Self) -> bool {
        if Arc::ptr_eq(&self.0, &other.0) {
            return true;
        }
        match (self.data(), other.data()) {
            (Some(a), Some(b)) => a == b,
            // The same data, here or referred to.
            _ => self.size() == other.size() && self.sha256() == other.sha256(),
        }
    }
}

impl fmt::Debug for Brep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_reference() {
            write!(
                f,
                "Brep({} bytes, reference {})",
                self.size(),
                self.sha256()
            )
        } else {
            write!(f, "Brep({} bytes)", self.size())
        }
    }
}

impl Serialize for Brep {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct BrepFile<'a> {
            format: &'a str,
            compression: &'a str,
            size: u64,
            #[serde(skip_serializing_if = "Option::is_none")]
            data: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            sha256: Option<String>,
        }
        let (data, sha256) = match self.data() {
            Some(data) => (Some(self.encoded(data)), None),
            None => (None, Some(self.sha256().to_string())),
        };
        BrepFile {
            format: "occt",
            compression: "zlib",
            size: self.size(),
            data,
            sha256,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Brep {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Owned {
            format: String,
            compression: String,
            size: u64,
            #[serde(default)]
            data: Option<String>,
            #[serde(default)]
            sha256: Option<String>,
        }
        let file = Owned::deserialize(deserializer)?;
        if file.format != "occt" {
            return Err(D::Error::custom(format!(
                "unknown B-rep format '{}' (expected \"occt\")",
                file.format
            )));
        }
        if file.size > MAX_SIZE {
            return Err(D::Error::custom(format!(
                "B-rep size {} is too large",
                file.size
            )));
        }
        let data = match (file.data, file.sha256) {
            (Some(data), None) => data,
            (None, Some(sha256)) => {
                // In a project's store (version 3).
                if file.compression != "zlib" {
                    return Err(D::Error::custom(format!(
                        "B-rep data in a project's store is zlib-compressed, not '{}'",
                        file.compression
                    )));
                }
                let sha256 = sha256
                    .parse()
                    .map_err(|e| D::Error::custom(format!("B-rep sha256: {e}")))?;
                return Ok(Brep::reference(sha256, file.size));
            }
            (Some(_), Some(_)) => {
                return Err(D::Error::custom(
                    "a B-rep has its \"data\" or a \"sha256\", not both",
                ));
            }
            (None, None) => return Err(D::Error::missing_field("data")),
        };
        let decoded = crate::base64::decode(&data)
            .map_err(|e| D::Error::custom(format!("B-rep data: {e}")))?;
        let size = file.size as usize;
        let (bytes, encoded) = match file.compression.as_str() {
            "zlib" => {
                let bytes = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&decoded, size)
                    .map_err(|e| {
                        D::Error::custom(format!("B-rep data does not decompress: {:?}", e.status))
                    })?;
                (bytes, Some(data))
            }
            "none" => (decoded, None),
            other => {
                return Err(D::Error::custom(format!(
                    "unknown B-rep compression '{other}' (expected \"zlib\" or \"none\")"
                )));
            }
        };
        if bytes.len() != size {
            return Err(D::Error::custom(format!(
                "B-rep data has {} bytes, expected {size}",
                bytes.len()
            )));
        }
        let brep = Brep::new(bytes);
        if let Some(encoded) = encoded {
            let _ = brep.0.encoded.set(encoded);
        }
        Ok(brep)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::features::FeatureDef;

    #[test]
    fn breps_round_trip_compressed() {
        let data: Vec<u8> = (0..2000u32).map(|i| (i % 7) as u8).collect();
        let brep = Brep::new(data.clone());
        let value = serde_json::to_value(&brep).unwrap();
        assert_eq!(value["format"], "occt");
        assert_eq!(value["compression"], "zlib");
        assert_eq!(value["size"], 2000);
        assert!(value["data"].as_str().unwrap().len() < 200, "{value}");
        let back: Brep = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(back.data(), Some(&data[..]));
        assert_eq!(back, brep);
        // Saving again writes the text it was read from.
        assert_eq!(serde_json::to_value(&back).unwrap(), value);
        assert_eq!(format!("{back:?}"), "Brep(2000 bytes)");

        let plain: Brep = serde_json::from_value(
            json!({"format": "occt", "compression": "none", "size": 3, "data": "Zm9v"}),
        )
        .unwrap();
        assert_eq!(plain.data(), Some(&b"foo"[..]));
    }

    #[test]
    fn references_name_the_data_by_its_sha256() {
        let data: Vec<u8> = (0..3000u32).map(|i| (i % 11) as u8).collect();
        let brep = Brep::new(data.clone());
        assert_eq!(brep.sha256(), Sha256::of(&data));
        let reference = brep.to_reference();
        assert!(reference.is_reference() && !brep.is_reference());
        assert_eq!(reference.data(), None);
        assert_eq!(reference, brep, "the same data, referred to");
        assert_eq!(
            format!("{reference:?}"),
            format!("Brep(3000 bytes, reference {})", brep.sha256())
        );
        let value = serde_json::to_value(&reference).unwrap();
        assert_eq!(
            value,
            json!({"format": "occt", "compression": "zlib", "size": 3000,
                   "sha256": brep.sha256().to_string()})
        );
        let back: Brep = serde_json::from_value(value).unwrap();
        assert!(back.is_reference());
        assert_eq!(back, brep);

        // The store's file: the data zlib-compressed, checked when read.
        let stored = brep.stored().unwrap();
        assert!(stored.len() < data.len());
        let resolved = back.resolve(&stored).unwrap();
        assert_eq!(resolved.data(), Some(&data[..]));
        let other = Brep::new(b"other data".to_vec()).stored().unwrap();
        assert!(
            back.resolve(&other)
                .unwrap_err()
                .contains("bytes, expected 3000")
        );
        let mut changed = data.clone();
        changed[7] ^= 1;
        let same_size = Brep::new(changed).stored().unwrap();
        assert!(back.resolve(&same_size).unwrap_err().contains("SHA-256"));
        assert!(
            back.resolve(b"not zlib")
                .unwrap_err()
                .contains("decompress")
        );

        // A file read from a single file keeps its compressed data at hand.
        let read: Brep = serde_json::from_value(serde_json::to_value(&brep).unwrap()).unwrap();
        assert_eq!(read.stored().unwrap(), stored);
    }

    #[test]
    fn malformed_references_are_rejected() {
        let sha = Sha256::of(b"x").to_string();
        let error = |value: serde_json::Value| {
            serde_json::from_value::<Brep>(value)
                .unwrap_err()
                .to_string()
        };
        assert!(
            error(json!({"format": "occt", "compression": "none", "size": 1, "sha256": sha}))
                .contains("zlib-compressed, not 'none'")
        );
        assert!(
            error(json!({"format": "occt", "compression": "zlib", "size": 1, "sha256": "abc"}))
                .contains("not a SHA-256")
        );
        assert!(
            error(
                json!({"format": "occt", "compression": "zlib", "size": 1, "sha256": sha,
                         "data": "eA=="})
            )
            .contains("not both")
        );
        assert!(
            error(json!({"format": "occt", "compression": "zlib", "size": 1}))
                .contains("missing field `data`")
        );
    }

    #[test]
    fn malformed_breps_are_rejected() {
        let good = serde_json::to_value(Brep::new(b"some brep".to_vec())).unwrap();
        let with = |field: &str, value: serde_json::Value| {
            let mut v = good.clone();
            v[field] = value;
            serde_json::from_value::<Brep>(v).unwrap_err().to_string()
        };
        assert!(with("format", json!("step")).contains("unknown B-rep format 'step'"));
        assert!(with("compression", json!("lzma")).contains("unknown B-rep compression"));
        assert!(with("size", json!(4)).contains("does not decompress"));
        assert!(with("size", json!(400)).contains("has 9 bytes, expected 400"));
        assert!(with("data", json!("Zm9v!")).contains("B-rep data:"));
        assert!(with("size", json!(1u64 << 40)).contains("too large"));
        let mut extra = good.clone();
        extra["file"] = json!("body.brep");
        assert!(
            serde_json::from_value::<Brep>(extra)
                .unwrap_err()
                .to_string()
                .contains("unknown field `file`")
        );
    }

    #[test]
    fn base_features_serialize_with_defaults_left_out() {
        let brep = serde_json::to_value(Brep::new(b"x".to_vec())).unwrap();
        let def: FeatureDef<String> = serde_json::from_value(json!({
            "type": "base", "bodies": [{"name": "Bracket", "brep": brep}]
        }))
        .unwrap();
        let FeatureDef::Base(base) = &def else {
            panic!("a base feature");
        };
        assert_eq!(base.operation, Operation::NewBody);
        assert!(base.participants.is_empty());
        assert_eq!(
            serde_json::to_value(&def).unwrap(),
            json!({"type": "base", "bodies": [{"name": "Bracket", "brep": brep}]})
        );
        let joined: FeatureDef<String> = serde_json::from_value(json!({
            "type": "base", "bodies": [{"brep": brep, "color": [1.0, 0.5, 0.0]}],
            "operation": "cut", "participants": ["F2.b0"], "source": "part.step"
        }))
        .unwrap();
        let value = serde_json::to_value(&joined).unwrap();
        assert_eq!(value["operation"], "cut");
        assert_eq!(value["participants"], json!(["F2.b0"]));
        assert_eq!(value["source"], "part.step");
        assert_eq!(value["bodies"][0]["color"], json!([1.0, 0.5, 0.0]));
    }
}

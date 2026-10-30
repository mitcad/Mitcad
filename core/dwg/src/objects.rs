// SPDX-License-Identifier: MIT
//! The objects (the specification's chapter 20): the common frame of
//! every object (its size, type, handle, extended data, and from R2007 on
//! its string stream and its handle stream), the common entity data, and
//! the objects the import uses: block headers, layers, inserts, the
//! entities with B-rep data, 3D faces, polyface and polygon meshes with
//! their vertices, and subdivision meshes.

use crate::bits::{BitReader, HandleRef, modular_char, modular_short};
use crate::classes::Class;
use crate::{DwgError, Version};

/// Fixed object types (20.3).
pub mod types {
    pub const TEXT: u16 = 1;
    pub const ATTRIB: u16 = 2;
    pub const ATTDEF: u16 = 3;
    pub const BLOCK: u16 = 4;
    pub const ENDBLK: u16 = 5;
    pub const SEQEND: u16 = 6;
    pub const INSERT: u16 = 7;
    pub const MINSERT: u16 = 8;
    pub const VERTEX_2D: u16 = 0x0A;
    pub const VERTEX_3D: u16 = 0x0B;
    pub const VERTEX_MESH: u16 = 0x0C;
    pub const VERTEX_PFACE: u16 = 0x0D;
    pub const VERTEX_PFACE_FACE: u16 = 0x0E;
    pub const POLYLINE_2D: u16 = 0x0F;
    pub const POLYLINE_3D: u16 = 0x10;
    pub const ARC: u16 = 0x11;
    pub const CIRCLE: u16 = 0x12;
    pub const LINE: u16 = 0x13;
    pub const POINT: u16 = 0x1B;
    pub const FACE3D: u16 = 0x1C;
    pub const POLYLINE_PFACE: u16 = 0x1D;
    pub const POLYLINE_MESH: u16 = 0x1E;
    pub const SOLID: u16 = 0x1F;
    pub const TRACE: u16 = 0x20;
    pub const VIEWPORT: u16 = 0x22;
    pub const ELLIPSE: u16 = 0x23;
    pub const SPLINE: u16 = 0x24;
    pub const REGION: u16 = 0x25;
    pub const SOLID3D: u16 = 0x26;
    pub const BODY: u16 = 0x27;
    pub const BLOCK_CONTROL: u16 = 0x30;
    pub const BLOCK_HEADER: u16 = 0x31;
    pub const LAYER_CONTROL: u16 = 0x32;
    pub const LAYER: u16 = 0x33;
    pub const LWPOLYLINE: u16 = 0x4D;
    pub const HATCH: u16 = 0x4E;
}

/// The name of a fixed object type, as DXF names it.
pub fn fixed_type_name(code: u16) -> Option<&'static str> {
    Some(match code {
        1 => "TEXT",
        2 => "ATTRIB",
        3 => "ATTDEF",
        4 => "BLOCK",
        5 => "ENDBLK",
        6 => "SEQEND",
        7 => "INSERT",
        8 => "MINSERT",
        0x0A..=0x0E => "VERTEX",
        0x0F | 0x10 | 0x1D | 0x1E => "POLYLINE",
        0x11 => "ARC",
        0x12 => "CIRCLE",
        0x13 => "LINE",
        0x14..=0x1A => "DIMENSION",
        0x1B => "POINT",
        0x1C => "3DFACE",
        0x1F => "SOLID",
        0x20 => "TRACE",
        0x21 => "SHAPE",
        0x22 => "VIEWPORT",
        0x23 => "ELLIPSE",
        0x24 => "SPLINE",
        0x25 => "REGION",
        0x26 => "3DSOLID",
        0x27 => "BODY",
        0x28 => "RAY",
        0x29 => "XLINE",
        0x2A => "DICTIONARY",
        0x2B => "OLEFRAME",
        0x2C => "MTEXT",
        0x2D => "LEADER",
        0x2E => "TOLERANCE",
        0x2F => "MLINE",
        0x30 => "BLOCK_CONTROL",
        0x31 => "BLOCK_RECORD",
        0x32 => "LAYER_CONTROL",
        0x33 => "LAYER",
        0x34 => "STYLE_CONTROL",
        0x35 => "STYLE",
        0x38 => "LTYPE_CONTROL",
        0x39 => "LTYPE",
        0x3C => "VIEW_CONTROL",
        0x3D => "VIEW",
        0x3E => "UCS_CONTROL",
        0x3F => "UCS",
        0x40 => "VPORT_CONTROL",
        0x41 => "VPORT",
        0x42 => "APPID_CONTROL",
        0x43 => "APPID",
        0x44 => "DIMSTYLE_CONTROL",
        0x45 => "DIMSTYLE",
        0x46 => "VP_ENT_HDR_CONTROL",
        0x47 => "VP_ENT_HDR",
        0x48 => "GROUP",
        0x49 => "MLINESTYLE",
        0x4A => "OLE2FRAME",
        0x4C => "LONG_TRANSACTION",
        0x4D => "LWPOLYLINE",
        0x4E => "HATCH",
        0x4F => "XRECORD",
        0x50 => "ACDBPLACEHOLDER",
        0x51 => "VBA_PROJECT",
        0x52 => "LAYOUT",
        0x1f2 => "ACAD_PROXY_ENTITY",
        0x1f3 => "ACAD_PROXY_OBJECT",
        _ => return None,
    })
}

/// Fixed types that are entities (OLE2FRAME too: read as an object, its
/// data ended early, mitcad#589).
fn fixed_is_entity(code: u16) -> bool {
    matches!(code, 1..=0x29 | 0x2B..=0x2F | 0x4A | 0x4D | 0x4E | 0x1f2)
}

/// The DXF names of class entities whose data starts with B-rep data as
/// a 3D solid's does.
pub const SURFACE_CLASSES: &[&str] = &[
    "PLANESURFACE",
    "EXTRUDEDSURFACE",
    "LOFTEDSURFACE",
    "REVOLVEDSURFACE",
    "SWEPTSURFACE",
    "NURBSURFACE",
    // The plain surface class (AcDbSurface), as a public sample drawing
    // stores its procedural surfaces' B-rep (mitcad#589).
    "SURFACE",
];

/// An entity's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    ByLayer,
    ByBlock,
    /// A colour index (1 to 255).
    Index(u8),
    /// A true colour.
    Rgb([u8; 3]),
}

impl Color {
    fn from_index(i: i16) -> Color {
        match i {
            0 => Color::ByBlock,
            256 => Color::ByLayer,
            1..=255 => Color::Index(i as u8),
            // A negative index: the colour of a layer that is off.
            -255..=-1 => Color::Index((-i) as u8),
            _ => Color::ByLayer,
        }
    }

    /// The colour as sRGB, None when it comes from the layer or block.
    pub fn rgb(self) -> Option<[u8; 3]> {
        match self {
            Color::Index(i) => Some(aci_rgb(i)),
            Color::Rgb(c) => Some(c),
            _ => None,
        }
    }
}

/// The RGB value of a colour index: the seven named colours, then
/// 24 hues in steps of 15 degrees, each in five shades of a full and a
/// half saturated colour, then six greys.
pub fn aci_rgb(i: u8) -> [u8; 3] {
    match i {
        1 => [255, 0, 0],
        2 => [255, 255, 0],
        3 => [0, 255, 0],
        4 => [0, 255, 255],
        5 => [0, 0, 255],
        6 => [255, 0, 255],
        7 | 0 => [255, 255, 255],
        8 => [128, 128, 128],
        9 => [192, 192, 192],
        250..=255 => {
            let g = [51u8, 91, 132, 173, 214, 255][usize::from(i - 250)];
            [g, g, g]
        }
        _ => {
            let hue = f64::from((i - 10) / 10) * 15.0;
            let shade = (i - 10) % 10;
            let value = [1.0, 1.0, 0.8, 0.8, 0.6, 0.6, 0.5, 0.5, 0.3, 0.3][usize::from(shade)];
            let saturation = if shade.is_multiple_of(2) { 1.0 } else { 0.5 };
            hsv(hue, saturation, value)
        }
    }
}

fn hsv(h: f64, s: f64, v: f64) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let q = |f: f64| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [q(r), q(g), q(b)]
}

/// The common data of an entity that the import uses.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityCommon {
    /// 0: owned by the `owner` handle; 1: paper space; 2: model space.
    pub entmode: u8,
    pub owner: Option<u64>,
    pub layer: Option<u64>,
    pub color: Color,
    pub invisible: bool,
    /// The next entity of the block's chain (R13 to R2000), when stored
    /// or implied.
    pub next: Option<u64>,
}

/// The B-rep data an entity carries.
#[derive(Clone, Debug, PartialEq)]
pub enum BrepData {
    /// None in the entity (from R2013 on it is in the data storage).
    Empty,
    /// The text form, still masked as stored.
    Text(Vec<u8>),
    /// The binary form.
    Binary(Vec<u8>),
    /// The entity's data could not be read: why.
    Unreadable(String),
}

/// The entity types with B-rep data.
pub fn is_brep(code: u16, name: &str) -> bool {
    matches!(code, types::REGION | types::SOLID3D | types::BODY) || SURFACE_CLASSES.contains(&name)
}

/// An entity's own data.
#[derive(Clone, Debug, PartialEq)]
pub enum EntityData {
    /// `3DSOLID`, `BODY`, `REGION` and the surfaces with B-rep data.
    Brep(BrepData),
    Face3d {
        corners: [[f64; 3]; 4],
        invisible_edges: i16,
    },
    /// A polyface mesh (`POLYLINE` with flag 64).
    PolyfaceMesh {
        vertex_count: i16,
        face_count: i16,
        /// The vertices and faces it owns (R2004 on), else the first and
        /// last.
        owned: Vec<u64>,
        first_last: Option<(u64, u64)>,
    },
    /// A polygon mesh (`POLYLINE` with flag 16).
    PolygonMesh {
        flags: i16,
        m: i16,
        n: i16,
        owned: Vec<u64>,
        first_last: Option<(u64, u64)>,
    },
    /// A vertex of a polygon or polyface mesh.
    MeshVertex {
        point: [f64; 3],
        flags: u8,
    },
    /// A face record of a polyface mesh: 1-based vertex indices, negative
    /// for an invisible edge, 0 for none.
    FaceRecord {
        indices: [i16; 4],
    },
    Insert(Insert),
    /// A line (`LINE`).
    Line {
        start: [f64; 3],
        end: [f64; 3],
    },
    /// A circle (`CIRCLE`), or an arc (`ARC`) with its start and end
    /// angles in radians.
    Circle {
        center: [f64; 3],
        radius: f64,
        angles: Option<(f64, f64)>,
    },
    /// A point (`POINT`).
    Point([f64; 3]),
    /// An ellipse or elliptical arc (`ELLIPSE`): its centre, the end of its
    /// major axis from the centre, the minor to major ratio, the start and
    /// end parameters.
    Ellipse {
        center: [f64; 3],
        major: [f64; 3],
        ratio: f64,
        start: f64,
        end: f64,
    },
    /// A single-line text (`TEXT`) at its insertion point, as stored (in
    /// the code page before R2007, see [`crate::write::decode`]).
    Text {
        position: [f64; 3],
        height: f64,
        rotation: f64,
        value: String,
    },
    /// A subdivision mesh: its control cage, the mesh at level 0.
    Mesh {
        vertices: Vec<[f64; 3]>,
        /// Each face's vertex indices.
        faces: Vec<Vec<u32>>,
        /// The subdivision level it is shown at (smoothed above 0).
        level: i32,
    },
    /// Anything else.
    Other,
}

/// A block reference, also an array of them (`MINSERT`).
#[derive(Clone, Debug, PartialEq)]
pub struct Insert {
    pub block: u64,
    pub point: [f64; 3],
    pub scale: [f64; 3],
    pub rotation: f64,
    pub extrusion: [f64; 3],
    pub columns: i16,
    pub rows: i16,
    pub column_spacing: f64,
    pub row_spacing: f64,
}

/// A block header (a block table record).
#[derive(Clone, Debug, PartialEq)]
pub struct BlockHeader {
    pub name: String,
    pub base: [f64; 3],
    pub anonymous: bool,
    pub xref: bool,
}

/// A layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub color: Color,
    pub frozen: bool,
    /// Off: a negative colour index.
    pub off: bool,
}

/// What an object is.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Entity(EntityCommon, EntityData),
    BlockHeader(BlockHeader),
    Layer(Layer),
    Other,
}

/// An object read from the file.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub handle: u64,
    pub type_code: u16,
    /// The DXF name of its type (`3DSOLID`, `MESH`, `LAYER`, ...).
    pub type_name: String,
    /// It has binary data in the data storage (R2013 on).
    pub has_ds_data: bool,
    pub kind: Kind,
}

/// The reader of one object's streams.
struct Reader<'a> {
    v: Version,
    handle: u64,
    data: BitReader<'a>,
    strings: Option<BitReader<'a>>,
    handles: BitReader<'a>,
}

impl Reader<'_> {
    fn text(&mut self) -> Result<String, DwgError> {
        match &mut self.strings {
            Some(s) if self.v >= Version::R2007 => s.text16(),
            None if self.v >= Version::R2007 => Ok(String::new()),
            _ => self.data.text(),
        }
    }

    fn h(&mut self) -> Result<HandleRef, DwgError> {
        self.handles.handle(self.handle)
    }

    /// The colour of an entity (ENC from R2004 on); whether a colour
    /// book handle follows in the handle stream.
    fn entity_color(&mut self) -> Result<(Color, bool), DwgError> {
        let value = self.data.bs()?;
        if self.v < Version::R2004 {
            return Ok((Color::from_index(value), false));
        }
        let flags = value as u16 & 0xff00;
        let mut color = Color::from_index((value as u16 & 0x1ff) as i16);
        if flags & 0x8000 != 0 {
            let rgb = self.data.bl()? as u32;
            color = Color::Rgb([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]);
        }
        if flags & 0x2000 != 0 {
            self.data.bl()?;
        }
        Ok((color, flags & 0x4000 != 0))
    }

    /// CMC of a table entry: an index, and from R2004 on RGB, flags and
    /// names.
    fn cmc(&mut self) -> Result<Color, DwgError> {
        let index = self.data.bs()?;
        let mut color = Color::from_index(index);
        if self.v >= Version::R2004 {
            let rgb = self.data.bl()? as u32;
            let flags = self.data.rc()?;
            if rgb >> 24 == 0xc2 {
                color = Color::Rgb([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]);
            }
            if flags & 1 != 0 {
                self.text()?;
            }
            if flags & 2 != 0 {
                self.text()?;
            }
        }
        Ok(color)
    }
}

/// The kind of an object type: its DXF name and whether it is an entity.
pub fn type_info(code: u16, classes: &[Class]) -> (String, bool) {
    if code >= 500
        && let Some(c) = classes.iter().find(|c| c.number == code)
    {
        return (c.dxf_name.clone(), c.entity);
    }
    match fixed_type_name(code) {
        Some(name) => (name.to_owned(), fixed_is_entity(code)),
        None => (format!("type {code}"), false),
    }
}

/// Reads the object at `offset` of the objects' bytes.
pub fn read(
    objects: &[u8],
    offset: usize,
    v: Version,
    classes: &[Class],
) -> Result<Object, DwgError> {
    let mut p = offset;
    let size = modular_short(objects, &mut p)? as usize;
    let handle_bits = if v >= Version::R2010 {
        Some(modular_char(objects, &mut p, false)? as usize)
    } else {
        None
    };
    let start = p * 8;
    let end = (p + size) * 8;
    if p + size > objects.len() {
        return Err(DwgError::Corrupt(format!(
            "the object at {offset} ends past its section"
        )));
    }
    let mut data = BitReader::at(objects, v, start, end);
    let type_code = data.ot()?;
    let (type_name, entity) = type_info(type_code, classes);
    let mut endbit = match handle_bits {
        Some(h) => Some(end.checked_sub(h).ok_or_else(|| {
            DwgError::Corrupt(format!(
                "the object at {offset} has a handle stream larger than itself"
            ))
        })?),
        None if v >= Version::R2000 => Some(start + data.rl()? as usize),
        None => None,
    };
    let own = data.handle(0)?;
    if own.code != 0 {
        return Err(DwgError::Corrupt(format!(
            "the object at {offset} does not start with its handle"
        )));
    }
    let handle = own.handle;
    // Extended data: runs of a size, an application handle and bytes.
    loop {
        let n = data.bs()? as u16 as usize;
        if n == 0 {
            break;
        }
        data.handle(0)?;
        data.bytes(n)?;
    }
    if entity && data.bit()? {
        let n = if v >= Version::R2010 {
            data.bll()? as usize
        } else {
            data.rl()? as usize
        };
        data.bytes(n)?;
    }
    if v <= Version::R14 {
        endbit = Some(start + data.rl()? as usize);
    }
    let endbit = endbit.unwrap_or(end).min(end);
    let strings = if v >= Version::R2007 {
        data.string_stream(endbit)?
    } else {
        None
    };
    let handles = BitReader::at(objects, v, endbit, end);
    let mut r = Reader {
        v,
        handle,
        data,
        strings,
        handles,
    };
    let (kind, has_ds_data) = if entity {
        let (common, ds) = entity_common(&mut r)?;
        let parsed = entity_data(&mut r, type_code, &type_name);
        let data = match parsed {
            Ok(d) => d,
            // The entity stands with what is known: its B-rep record may
            // be in the data storage (R2013 on).
            Err(e) if is_brep(type_code, &type_name) => {
                EntityData::Brep(BrepData::Unreadable(e.to_string()))
            }
            // The 2D entities the drawing export writes are read back
            // (mitcad#728); the import leaves them out either way, so one
            // that does not read is only not decoded.
            Err(_) if is_flat(type_code) => EntityData::Other,
            Err(e) => {
                return Err(DwgError::Corrupt(format!("{type_name} {handle:X}: {e}")));
            }
        };
        (Kind::Entity(common, data), ds)
    } else {
        let ds = object_common(&mut r)?;
        let kind = match type_code {
            types::BLOCK_HEADER => Kind::BlockHeader(block_header(&mut r)?),
            types::LAYER => Kind::Layer(layer(&mut r)?),
            _ => Kind::Other,
        };
        (kind, ds)
    };
    Ok(Object {
        handle,
        type_code,
        type_name,
        has_ds_data,
        kind,
    })
}

/// The common data of an entity and its handles before the entity's own;
/// whether it has data in the data storage.
fn entity_common(r: &mut Reader) -> Result<(EntityCommon, bool), DwgError> {
    use Version::*;
    let v = r.v;
    let d = &mut r.data;
    let entmode = d.bb()?;
    let reactors = d.bl()?;
    let xdic_missing = v >= R2004 && d.bit()?;
    let ds = v >= R2013 && d.bit()?;
    let bylayer_lt = v <= R14 && d.bit()?;
    // The links' bit: up to R2000 only. From R2004 on the links are not
    // stored and neither is the bit (the specification lists it as always
    // 1; drawings of R2004 have no such bit: their common data ends with
    // the line weight just before the entity's own data).
    let nolinks = v >= R2004 || d.bit()?;
    let (color, book) = r.entity_color()?;
    let d = &mut r.data;
    d.bd()?;
    let (mut lt_flags, mut ps_flags, mut mat_flags) = (0, 0, 0);
    let (mut full_vs, mut face_vs, mut edge_vs) = (false, false, false);
    if v >= R2000 {
        lt_flags = d.bb()?;
        ps_flags = d.bb()?;
    }
    if v >= R2007 {
        mat_flags = d.bb()?;
        d.rc()?;
    }
    if v >= R2010 {
        full_vs = d.bit()?;
        face_vs = d.bit()?;
        edge_vs = d.bit()?;
    }
    let invisible = d.bs()? & 1 != 0;
    if v >= R2000 {
        d.rc()?;
    }
    let mut common = EntityCommon {
        entmode,
        color,
        invisible,
        ..Default::default()
    };
    // The handles.
    if entmode == 0 {
        common.owner = Some(r.h()?.handle);
    }
    for _ in 0..reactors.max(0) {
        r.h()?;
    }
    if !xdic_missing {
        r.h()?;
    }
    if v <= R14 {
        common.layer = Some(r.h()?.handle);
        if !bylayer_lt {
            r.h()?;
        }
    }
    if v <= R2000 {
        if !nolinks {
            r.h()?;
            common.next = Some(r.h()?.handle);
        } else {
            common.next = Some(r.handle + 1);
        }
    }
    if v >= R2004 && book {
        r.h()?;
    }
    if v >= R2000 {
        common.layer = Some(r.h()?.handle);
        if lt_flags == 3 {
            r.h()?;
        }
    }
    if v >= R2007 && mat_flags == 3 {
        r.h()?;
    }
    if v >= R2000 && ps_flags == 3 {
        r.h()?;
    }
    if v >= R2010 {
        for present in [full_vs, face_vs, edge_vs] {
            if present {
                r.h()?;
            }
        }
    }
    Ok((common, ds))
}

/// The common data of a non-entity object and its handles before its
/// own; whether it has data in the data storage.
fn object_common(r: &mut Reader) -> Result<bool, DwgError> {
    let v = r.v;
    let reactors = r.data.bl()?;
    let xdic_missing = v >= Version::R2004 && r.data.bit()?;
    let ds = v >= Version::R2013 && r.data.bit()?;
    r.h()?;
    for _ in 0..reactors.max(0) {
        r.h()?;
    }
    if !xdic_missing {
        r.h()?;
    }
    Ok(ds)
}

/// The flags every table entry starts with: the 64-flag, before R2007 the
/// xref index plus one (drawings of R2013 and R2018 have no such field:
/// their layers' and block headers' data ends where it should only
/// without it), and the xref dependency.
fn table_entry_flags(r: &mut Reader) -> Result<(), DwgError> {
    let d = &mut r.data;
    d.bit()?;
    if r.v < Version::R2007 {
        d.bs()?;
    }
    d.bit()?;
    Ok(())
}

fn block_header(r: &mut Reader) -> Result<BlockHeader, DwgError> {
    let name = r.text()?;
    table_entry_flags(r)?;
    let d = &mut r.data;
    let anonymous = d.bit()?;
    d.bit()?;
    let xref = d.bit()?;
    let overlaid = d.bit()?;
    if r.v >= Version::R2000 {
        d.bit()?;
    }
    if r.v >= Version::R2004 {
        d.bl()?;
    }
    let base = d.bd3()?;
    Ok(BlockHeader {
        name,
        base,
        anonymous,
        xref: xref || overlaid,
    })
}

fn layer(r: &mut Reader) -> Result<Layer, DwgError> {
    let name = r.text()?;
    table_entry_flags(r)?;
    let d = &mut r.data;
    let frozen = if r.v <= Version::R14 {
        let frozen = d.bit()?;
        d.bit()?;
        d.bit()?;
        d.bit()?;
        frozen
    } else {
        d.bs()? & 1 != 0
    };
    let index_pos = r.data.position();
    let color = r.cmc()?;
    // A layer that is off has its colour index negated.
    let mut probe = BitReader::at(r.data.data(), r.v, index_pos, index_pos + 18);
    let off = probe.bs().is_ok_and(|i| i < 0);
    Ok(Layer {
        name,
        color,
        frozen,
        off,
    })
}

fn insert(r: &mut Reader, array: bool) -> Result<Insert, DwgError> {
    let v = r.v;
    let d = &mut r.data;
    let point = d.bd3()?;
    let scale = if v < Version::R2000 {
        d.bd3()?
    } else {
        match d.bb()? {
            3 => [1.0; 3],
            1 => [1.0, d.dd(1.0)?, d.dd(1.0)?],
            2 => {
                let x = d.rd()?;
                [x; 3]
            }
            _ => {
                let x = d.rd()?;
                [x, d.dd(x)?, d.dd(x)?]
            }
        }
    };
    let rotation = d.bd()?;
    let extrusion = d.bd3()?;
    let attribs = d.bit()?;
    let owned = if v >= Version::R2004 && attribs {
        d.bl()?
    } else {
        0
    };
    let (mut columns, mut rows, mut column_spacing, mut row_spacing) = (1, 1, 0.0, 0.0);
    if array {
        columns = d.bs()?;
        rows = d.bs()?;
        column_spacing = d.bd()?;
        row_spacing = d.bd()?;
    }
    let _ = owned;
    let block = r.h()?.handle;
    Ok(Insert {
        block,
        point,
        scale,
        rotation,
        extrusion,
        columns,
        rows,
        column_spacing,
        row_spacing,
    })
}

/// The vertices a polyline owns: its list (R2004 on), or its first and
/// last (before).
/// A polyline's vertex list, or its first and last vertex.
type Vertices = (Vec<u64>, Option<(u64, u64)>);

fn owned_vertices(r: &mut Reader, count: i32) -> Result<Vertices, DwgError> {
    if r.v >= Version::R2004 {
        // Not sized from the count (a field of the file): the list grows
        // with the handles the data really holds.
        let mut owned = Vec::with_capacity((count.max(0) as usize).min(1024));
        for _ in 0..count.max(0) {
            owned.push(r.h()?.handle);
        }
        Ok((owned, None))
    } else {
        let first = r.h()?.handle;
        let last = r.h()?.handle;
        Ok((Vec::new(), Some((first, last))))
    }
}

/// B-rep data: an empty bit, then blocks of the masked text (version 1)
/// or the binary form (version 2).
fn brep_data(r: &mut Reader) -> Result<BrepData, DwgError> {
    let d = &mut r.data;
    if d.bit()? {
        return Ok(BrepData::Empty);
    }
    d.bit()?;
    let version = d.bs()?;
    match version {
        1 => {
            let mut text = Vec::new();
            loop {
                let n = d.bl()?;
                if n <= 0 {
                    break;
                }
                text.extend(d.bytes(n as usize)?);
            }
            Ok(BrepData::Text(text))
        }
        2 => {
            // No length: up to the end marker, in the ASM or the older
            // naming.
            let mut bytes = Vec::new();
            let ends: [&[u8]; 2] = [
                b"\x0e\x03End\x0e\x02of\x0e\x03ASM\x0d\x04data",
                b"\x0e\x03End\x0e\x02of\x0e\x04\x41\x43\x49\x53\x0d\x04data",
            ];
            loop {
                bytes.push(d.rc()?);
                if ends.iter().any(|e| bytes.ends_with(e)) {
                    break;
                }
            }
            Ok(BrepData::Binary(bytes))
        }
        _ => Err(DwgError::Corrupt(format!(
            "B-rep data of version {version}"
        ))),
    }
}

/// A subdivision mesh (the class `MESH`): version, blend crease,
/// subdivision level, the vertices, the face list (each face its vertex
/// count, then the indices), the edges and the creases.
fn mesh(r: &mut Reader) -> Result<EntityData, DwgError> {
    let d = &mut r.data;
    d.bs()?;
    d.bit()?;
    let level = d.bl()?;
    let n = d.bl()?;
    if !(0..=10_000_000).contains(&n) {
        return Err(DwgError::Corrupt(format!("a mesh of {n} vertices")));
    }
    let mut vertices = Vec::with_capacity((n as usize).min(1024));
    for _ in 0..n {
        vertices.push(d.bd3()?);
    }
    let list = d.bl()?;
    if !(0..=50_000_000).contains(&list) {
        return Err(DwgError::Corrupt(format!("a mesh face list of {list}")));
    }
    let mut items = Vec::with_capacity((list as usize).min(1024));
    for _ in 0..list {
        items.push(d.bl()?);
    }
    let mut faces = Vec::new();
    let mut i = 0;
    while i < items.len() {
        let count = items[i] as usize;
        let face = items
            .get(i + 1..i + 1 + count)
            .ok_or_else(|| DwgError::Corrupt("a mesh face runs past its list".into()))?;
        if count < 3 || face.iter().any(|&k| k < 0 || k >= n) {
            return Err(DwgError::Corrupt("a mesh face with a bad vertex".into()));
        }
        faces.push(face.iter().map(|&k| k as u32).collect());
        i += 1 + count;
    }
    Ok(EntityData::Mesh {
        vertices,
        faces,
        level,
    })
}

fn entity_data(r: &mut Reader, code: u16, name: &str) -> Result<EntityData, DwgError> {
    use types::*;
    let v = r.v;
    Ok(match code {
        REGION | SOLID3D | BODY => EntityData::Brep(brep_data(r)?),
        FACE3D => {
            let d = &mut r.data;
            let mut corners = [[0.0; 3]; 4];
            let mut invisible_edges = 0;
            if v < Version::R2000 {
                for c in &mut corners {
                    *c = d.bd3()?;
                }
                invisible_edges = d.bs()?;
            } else {
                let no_flags = d.bit()?;
                let z_zero = d.bit()?;
                let x = d.rd()?;
                let y = d.rd()?;
                let z = if z_zero { 0.0 } else { d.rd()? };
                corners[0] = [x, y, z];
                for i in 1..4 {
                    corners[i] = d.dd3(corners[i - 1])?;
                }
                if !no_flags {
                    invisible_edges = d.bs()?;
                }
            }
            EntityData::Face3d {
                corners,
                invisible_edges,
            }
        }
        POLYLINE_PFACE => {
            let vertex_count = r.data.bs()?;
            let face_count = r.data.bs()?;
            let count = if v >= Version::R2004 { r.data.bl()? } else { 0 };
            let (owned, first_last) = owned_vertices(r, count)?;
            EntityData::PolyfaceMesh {
                vertex_count,
                face_count,
                owned,
                first_last,
            }
        }
        POLYLINE_MESH => {
            let d = &mut r.data;
            let flags = d.bs()?;
            d.bs()?;
            let m = d.bs()?;
            let n = d.bs()?;
            d.bs()?;
            d.bs()?;
            let count = if v >= Version::R2004 { d.bl()? } else { 0 };
            let (owned, first_last) = owned_vertices(r, count)?;
            EntityData::PolygonMesh {
                flags,
                m,
                n,
                owned,
                first_last,
            }
        }
        VERTEX_MESH | VERTEX_PFACE => {
            let flags = r.data.rc()?;
            let point = r.data.bd3()?;
            EntityData::MeshVertex { point, flags }
        }
        VERTEX_PFACE_FACE => {
            let d = &mut r.data;
            EntityData::FaceRecord {
                indices: [d.bs()?, d.bs()?, d.bs()?, d.bs()?],
            }
        }
        INSERT => EntityData::Insert(insert(r, false)?),
        MINSERT => EntityData::Insert(insert(r, true)?),
        _ if is_flat(code) => flat(r, code)?,
        _ if SURFACE_CLASSES.contains(&name) => EntityData::Brep(brep_data(r)?),
        _ if name == "MESH" => mesh(r)?,
        _ => EntityData::Other,
    })
}

/// The 2D entities [`flat`] reads.
fn is_flat(code: u16) -> bool {
    use types::*;
    matches!(code, LINE | ARC | CIRCLE | POINT | ELLIPSE | TEXT)
}

/// A line, arc, circle, point, ellipse or text (the specification's
/// 20.4.3, 18, 20, 21, 31 and 39).
fn flat(r: &mut Reader, code: u16) -> Result<EntityData, DwgError> {
    use types::*;
    let v = r.v;
    let d = &mut r.data;
    Ok(match code {
        LINE => {
            let (start, end) = if v < Version::R2000 {
                (d.bd3()?, d.bd3()?)
            } else {
                let z_zero = d.bit()?;
                let x1 = d.rd()?;
                let x2 = d.dd(x1)?;
                let y1 = d.rd()?;
                let y2 = d.dd(y1)?;
                let (z1, z2) = if z_zero {
                    (0.0, 0.0)
                } else {
                    let z1 = d.rd()?;
                    (z1, d.dd(z1)?)
                };
                ([x1, y1, z1], [x2, y2, z2])
            };
            EntityData::Line { start, end }
        }
        ARC | CIRCLE => {
            let center = d.bd3()?;
            let radius = d.bd()?;
            d.bt()?;
            d.be()?;
            let angles = if code == ARC {
                Some((d.bd()?, d.bd()?))
            } else {
                None
            };
            EntityData::Circle {
                center,
                radius,
                angles,
            }
        }
        POINT => EntityData::Point(d.bd3()?),
        ELLIPSE => EntityData::Ellipse {
            center: d.bd3()?,
            major: d.bd3()?,
            ratio: {
                d.bd3()?;
                d.bd()?
            },
            start: d.bd()?,
            end: d.bd()?,
        },
        _ => {
            let (position, height, rotation) = if v < Version::R2000 {
                let elevation = d.bd()?;
                let [x, y] = d.rd2()?;
                d.rd2()?;
                d.bd3()?;
                d.bd()?;
                d.bd()?;
                let rotation = d.bd()?;
                let height = d.bd()?;
                d.bd()?;
                ([x, y, elevation], height, rotation)
            } else {
                let flags = d.rc()?;
                let elevation = if flags & 0x01 == 0 { d.rd()? } else { 0.0 };
                let [x, y] = d.rd2()?;
                if flags & 0x02 == 0 {
                    d.dd(x)?;
                    d.dd(y)?;
                }
                d.be()?;
                d.bt()?;
                if flags & 0x04 == 0 {
                    d.rd()?;
                }
                let rotation = if flags & 0x08 == 0 { d.rd()? } else { 0.0 };
                let height = d.rd()?;
                if flags & 0x10 == 0 {
                    d.rd()?;
                }
                ([x, y, elevation], height, rotation)
            };
            let value = r.text()?;
            EntityData::Text {
                position,
                height,
                rotation,
                value,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_surfaces_have_brep_data() {
        // A class entity SURFACE (AcDbSurface) carries B-rep data as the
        // other surfaces do (a public sample drawing: 88 of them).
        assert!(is_brep(500, "SURFACE"));
        assert!(is_brep(501, "PLANESURFACE"));
        assert!(!is_brep(502, "SURFACEVAR"));
    }

    #[test]
    fn ole_frames_are_entities() {
        // Both OLE frame types are entities: their common data is an
        // entity's (a public sample drawing's OLE2FRAME read as an object
        // ended early).
        assert_eq!(type_info(0x2B, &[]), ("OLEFRAME".to_owned(), true));
        assert_eq!(type_info(0x4A, &[]), ("OLE2FRAME".to_owned(), true));
        assert_eq!(type_info(0x4F, &[]), ("XRECORD".to_owned(), false));
    }
}

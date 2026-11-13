// SPDX-License-Identifier: MIT
//! The model of a .3dm file: its header, the version of the library and
//! the application that wrote it, its unit system and tolerance, its layers,
//! block definitions and objects with their attributes.

use std::collections::HashMap;

use crate::archive::{self, Reader, Result, Uuid, err};
use crate::geom::{self, Geometry, class};

/// The length unit of a model.
#[derive(Clone, Debug, PartialEq)]
pub struct Units {
    /// The format's unit system number (2 millimetres, 8 inches, …).
    pub system: u32,
    /// Millimetres per unit (1 when the file has no unit).
    pub mm: f64,
    /// The unit's name for the report (`millimeters`, `inches`, …).
    pub name: String,
    /// No unit in the file: millimetres assumed.
    pub assumed: bool,
}

impl Units {
    fn of(system: u32, meters_per_unit: f64, custom: &str) -> Units {
        let (mm, name) = match system {
            1 => (1e-3, "microns"),
            2 => (1.0, "millimeters"),
            3 => (10.0, "centimeters"),
            4 => (1e3, "meters"),
            5 => (1e6, "kilometers"),
            6 => (25.4e-6, "microinches"),
            7 => (25.4e-3, "mils"),
            8 => (25.4, "inches"),
            9 => (304.8, "feet"),
            10 => (1_609_344.0, "miles"),
            12 => (1e-7, "angstroms"),
            13 => (1e-6, "nanometers"),
            14 => (100.0, "decimeters"),
            15 => (1e4, "dekameters"),
            16 => (1e5, "hectometers"),
            19 => (914.4, "yards"),
            20 => (25.4 / 72.0, "printer points"),
            21 => (25.4 / 6.0, "printer picas"),
            11 if meters_per_unit > 0.0 && meters_per_unit.is_finite() => {
                (meters_per_unit * 1e3, custom)
            }
            _ => {
                return Units {
                    system,
                    mm: 1.0,
                    name: "none".to_owned(),
                    assumed: true,
                };
            }
        };
        Units {
            system,
            mm,
            name: if name.is_empty() {
                "custom".to_owned()
            } else {
                name.to_owned()
            },
            assumed: false,
        }
    }

    /// The unit's symbol where MitCad has the unit (mm, cm, m, in, ft).
    pub fn symbol(&self) -> Option<&'static str> {
        match self.system {
            2 => Some("mm"),
            3 => Some("cm"),
            4 => Some("m"),
            8 => Some("in"),
            9 => Some("ft"),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub index: i32,
    pub id: Uuid,
    pub parent: Uuid,
    pub name: String,
    /// `0xAABBGGRR`.
    pub color: u32,
    pub visible: bool,
}

/// How an object's colour is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSource {
    Layer,
    Object,
    Material,
    Parent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Attributes {
    pub id: Uuid,
    pub layer: i32,
    pub name: String,
    pub color: u32,
    pub color_source: ColorSource,
    pub visible: bool,
    /// 0 normal, 1 hidden, 2 locked, 3 a block definition's geometry.
    pub mode: u8,
}

impl Default for Attributes {
    fn default() -> Self {
        Attributes {
            id: Uuid::NIL,
            layer: 0,
            name: String::new(),
            color: 0,
            color_source: ColorSource::Layer,
            visible: true,
            mode: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub geometry: Geometry,
    pub attributes: Attributes,
}

/// A block definition: its name and the ids of its objects.
#[derive(Clone, Debug, PartialEq)]
pub struct InstanceDefinition {
    pub id: Uuid,
    pub name: String,
    pub objects: Vec<Uuid>,
}

/// An object the reader could not read, and why: what it is and its
/// attributes where they were read (its data may be damaged while its
/// attributes, which follow it, are whole), so that a person reads which
/// object it was in words; its index in the object table is for agents
/// and the log.
#[derive(Clone, Debug, PartialEq)]
pub struct Failed {
    pub index: usize,
    /// Its kind by its class (`B-rep`, `mesh`, …), when its class was read.
    pub kind: Option<String>,
    pub attributes: Option<Attributes>,
    pub error: archive::Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct File3dm {
    /// The archive's version: 2, 3, 4, 50, 60, 70, 80.
    pub version: u32,
    /// The version of the library that wrote it (0 when the file does not
    /// say).
    pub writer: u64,
    /// The application that wrote it (its name and version), from the
    /// properties, or the file's comment.
    pub application: String,
    pub units: Units,
    /// The model's absolute tolerance (its units).
    pub tolerance: f64,
    pub layers: Vec<Layer>,
    pub definitions: Vec<InstanceDefinition>,
    pub objects: Vec<Object>,
    pub failed: Vec<Failed>,
    /// Parts of the file that could not be read (a table cut short): what
    /// is wrong in words (`.0`), and where for agents and the log (`.1`).
    pub warnings: Vec<archive::Error>,
    /// `layers` by their index and by their id.
    layer_at: HashMap<i32, usize>,
    layer_of_id: HashMap<Uuid, usize>,
}

pub const HEADER: &[u8] = b"3D Geometry File Format ";

impl File3dm {
    pub fn open(path: &std::path::Path) -> Result<File3dm> {
        let data = std::fs::read(path).map_err(|e| archive::Error::new(e.to_string()))?;
        File3dm::read(&data)
    }

    pub fn read(data: &[u8]) -> Result<File3dm> {
        if data.len() < 32 || !data.starts_with(HEADER) {
            return err("not a .3dm file (its header is missing)");
        }
        let version: u32 = std::str::from_utf8(&data[24..32])
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .ok_or_else(|| archive::Error::new("not a .3dm file (its version is missing)"))?;
        if version < 2 {
            return err(format!(
                "a .3dm file of format version {version}, which MitCad does not read"
            ));
        }
        let mut file = File3dm {
            version,
            writer: 0,
            application: String::new(),
            units: Units::of(0, 0.0, ""),
            tolerance: 0.001,
            layers: Vec::new(),
            definitions: Vec::new(),
            objects: Vec::new(),
            failed: Vec::new(),
            warnings: Vec::new(),
            layer_at: HashMap::new(),
            layer_of_id: HashMap::new(),
        };
        let mut r = Reader::new(&data[32..], version);
        // Files whose properties do not say are taken as new ones.
        r.writer = u64::MAX;
        while !r.at_end() {
            let mut c = match r.chunk_or_rest() {
                Ok((c, false)) => c,
                Ok((c, true)) => {
                    file.warnings.push(archive::Error::at(
                        "the file ends early",
                        format!("in the chunk {:08X}", c.typecode),
                    ));
                    c
                }
                Err(e) => {
                    file.warnings
                        .push(within(e, "the file ends early", String::new()));
                    break;
                }
            };
            match c.typecode {
                archive::TCODE_ENDOFFILE => break,
                0x0000_0001 => {
                    // The comment block: the application's name and more.
                    let n = c.data.remaining();
                    if let Ok(b) = c.data.bytes(n) {
                        let text = String::from_utf8_lossy(b);
                        if let Some(line) = text.lines().map(str::trim).find(|l| !l.is_empty()) {
                            file.application = line.trim_end_matches('\0').to_owned();
                        }
                    }
                }
                archive::TCODE_PROPERTIES_TABLE => {
                    file.properties(&mut c.data);
                    if file.writer > 0 {
                        r.writer = file.writer;
                    }
                }
                archive::TCODE_SETTINGS_TABLE => file.settings(&mut c.data),
                archive::TCODE_LAYER_TABLE => file.layer_table(&mut c.data),
                archive::TCODE_INSTANCE_DEFINITION_TABLE => file.definition_table(&mut c.data),
                archive::TCODE_OBJECT_TABLE => file.object_table(&mut c.data),
                _ => {}
            }
        }
        file.index_layers();
        Ok(file)
    }

    /// The layers by index and by id (the first of each), so that looking
    /// up an object's layer does not walk the table.
    fn index_layers(&mut self) {
        self.layer_at.clear();
        self.layer_of_id.clear();
        for (k, l) in self.layers.iter().enumerate() {
            self.layer_at.entry(l.index).or_insert(k);
            if !l.id.is_nil() {
                self.layer_of_id.entry(l.id).or_insert(k);
            }
        }
    }

    fn properties(&mut self, r: &mut Reader) {
        while let Ok(c) = r.chunk() {
            match c.typecode {
                archive::TCODE_ENDOFTABLE => break,
                archive::TCODE_PROPERTIES_WRITER_VERSION => {
                    self.writer = u64::try_from(c.value).unwrap_or(0);
                }
                archive::TCODE_PROPERTIES_APPLICATION => {
                    // A version byte, the application's name, URL and
                    // details.
                    let mut d = c.data;
                    if d.chunk_version().is_ok()
                        && let Ok(name) = d.wstring()
                        && !name.trim().is_empty()
                    {
                        self.application = name.trim().to_owned();
                    }
                }
                _ => {}
            }
        }
    }

    fn settings(&mut self, r: &mut Reader) {
        while let Ok(c) = r.chunk() {
            if c.typecode == archive::TCODE_ENDOFTABLE {
                break;
            }
            if c.typecode != archive::TCODE_SETTINGS_UNITSANDTOLS {
                continue;
            }
            let mut d = c.data;
            let read = (|| -> Result<()> {
                let v = d.i32()?;
                if !(100..200).contains(&v) {
                    return archive::err_at(
                        "units of a version MitCad does not read",
                        format!("version {v}"),
                    );
                }
                let system = d.u32()?;
                let tolerance = d.f64()?;
                d.f64()?; // angle tolerance
                d.f64()?; // relative tolerance
                let (mut meters, mut custom) = (0.0, String::new());
                if v >= 101 {
                    d.i32()?;
                    d.i32()?;
                    if v >= 102 {
                        meters = d.f64()?;
                        custom = d.wstring()?;
                    }
                }
                self.units = Units::of(system, meters, &custom);
                if tolerance > 0.0 && tolerance.is_finite() {
                    self.tolerance = tolerance;
                }
                Ok(())
            })();
            if let Err(e) = read {
                self.warnings
                    .push(within(e, "the model's units cannot be read", String::new()));
            }
        }
    }

    fn layer_table(&mut self, r: &mut Reader) {
        let mut index = 0;
        while let Ok(c) = r.chunk() {
            if c.typecode == archive::TCODE_ENDOFTABLE {
                break;
            }
            if c.typecode != archive::TCODE_LAYER_RECORD {
                continue;
            }
            let mut d = c.data;
            match read_layer(&mut d, index) {
                Ok(Some(layer)) => self.layers.push(layer),
                Ok(None) => {}
                Err(e) => self.warnings.push(within(
                    e,
                    "a layer cannot be read",
                    format!("layer {index}"),
                )),
            }
            index += 1;
        }
    }

    fn definition_table(&mut self, r: &mut Reader) {
        let mut index = 0;
        while let Ok(c) = r.chunk() {
            if c.typecode == archive::TCODE_ENDOFTABLE {
                break;
            }
            if c.typecode != archive::TCODE_INSTANCE_DEFINITION_RECORD {
                continue;
            }
            let mut d = c.data;
            match read_definition(&mut d) {
                Ok(Some(def)) => self.definitions.push(def),
                Ok(None) => self.warnings.push(archive::Error::at(
                    "a block definition of format version 5 or earlier is not read; its instances are left out",
                    format!("block definition {index}"),
                )),
                Err(e) => self.warnings.push(within(
                    e,
                    "a block definition cannot be read",
                    format!("block definition {index}"),
                )),
            }
            index += 1;
        }
    }

    fn object_table(&mut self, r: &mut Reader) {
        let mut index = 0;
        loop {
            let c = match r.chunk() {
                Ok(c) => c,
                Err(e) => {
                    self.warnings.push(within(
                        e,
                        "the object table ends early",
                        format!("after {index} objects"),
                    ));
                    break;
                }
            };
            if c.typecode == archive::TCODE_ENDOFTABLE {
                break;
            }
            if c.typecode != archive::TCODE_OBJECT_RECORD {
                continue;
            }
            let mut d = c.data;
            match read_object(&mut d, self.version, self.writer) {
                Ok(Some(object)) => self.objects.push(object),
                Ok(None) => {}
                Err(mut f) => {
                    f.index = index;
                    self.failed.push(*f);
                }
            }
            index += 1;
        }
    }

    /// The layer's full name (`Parent::Child`) by its index.
    /// The layer's full name (`Parent::Child`) by its index; parents are
    /// followed 32 deep at most (a file's parents may form a loop).
    pub fn layer_path(&self, index: i32) -> String {
        let Some(layer) = self.layer(index) else {
            return format!("layer {index}");
        };
        let mut names = vec![layer.name.clone()];
        let mut parent = layer.parent;
        for _ in 0..32 {
            match self.parent_layer(parent) {
                Some(p) => {
                    names.push(p.name.clone());
                    parent = p.parent;
                }
                None => break,
            }
        }
        names.reverse();
        names.join("::")
    }

    pub fn layer(&self, index: i32) -> Option<&Layer> {
        self.layer_at.get(&index).map(|&k| &self.layers[k])
    }

    /// The layer of a parent id (None for the nil id).
    fn parent_layer(&self, id: Uuid) -> Option<&Layer> {
        if id.is_nil() {
            return None;
        }
        self.layer_of_id.get(&id).map(|&k| &self.layers[k])
    }

    /// Whether the layer and all its parents are visible.
    pub fn layer_visible(&self, index: i32) -> bool {
        let Some(mut layer) = self.layer(index) else {
            return true;
        };
        for _ in 0..32 {
            if !layer.visible {
                return false;
            }
            match self.parent_layer(layer.parent) {
                Some(p) => layer = p,
                None => break,
            }
        }
        true
    }

    /// The colour an object shows in (`0xAABBGGRR`): its own, its layer's
    /// or, for the geometry of a block, the instance's (`parent`).
    pub fn object_color(&self, a: &Attributes, parent: Option<u32>) -> Option<u32> {
        match a.color_source {
            ColorSource::Object => Some(a.color),
            ColorSource::Parent => parent.or_else(|| self.layer(a.layer).map(|l| l.color)),
            ColorSource::Layer | ColorSource::Material => self.layer(a.layer).map(|l| l.color),
        }
    }
}

fn read_layer(r: &mut Reader, index: i32) -> Result<Option<Layer>> {
    let Some((uuid, mut d)) = geom::class_chunk(r)? else {
        return Ok(None);
    };
    if uuid != class::LAYER {
        return archive::err_at("a layer record of another class", format!("class {uuid}"));
    }
    let (major, minor) = d.chunk_version()?;
    if major != 1 {
        return archive::err_at(
            "a layer of a version MitCad does not read",
            format!("version {major}"),
        );
    }
    let mode = d.i32()?;
    let file_index = d.i32()?;
    d.i32()?; // IGES level
    d.i32()?; // material
    d.i32()?; // obsolete
    let color = d.color()?;
    d.skip(20)?; // obsolete line style
    let name = d.wstring()?;
    let mut layer = Layer {
        index: if file_index >= 0 { file_index } else { index },
        id: Uuid::NIL,
        parent: Uuid::NIL,
        name,
        color,
        visible: mode != 1,
    };
    if minor >= 1 {
        layer.visible = d.bool()? && mode != 1;
    }
    if minor >= 2 {
        d.i32()?; // line pattern
    }
    if minor >= 3 {
        d.color()?; // plot colour
        d.f64()?; // plot weight
    }
    if minor >= 4 {
        d.bool()?; // locked
    }
    if minor >= 5 {
        layer.id = d.uuid()?;
    }
    if minor >= 6 && d.writer > 200_505_110 {
        layer.parent = d.uuid()?;
    }
    Ok(Some(layer))
}

/// A block definition in the form of format version 6 and later (None for the
/// older format).
fn read_definition(r: &mut Reader) -> Result<Option<InstanceDefinition>> {
    let Some((uuid, mut d)) = geom::class_chunk(r)? else {
        return Ok(None);
    };
    if uuid != class::INSTANCE_DEFINITION {
        return archive::err_at(
            "a block definition record of another class",
            format!("class {uuid}"),
        );
    }
    if d.version < 60 || d.peek_typecode() != Some(archive::TCODE_ANONYMOUS_CHUNK) {
        return Ok(None);
    }
    let (major, _, mut c) = d.versioned_chunk("a block definition")?;
    if major != 1 {
        return archive::err_at(
            "a block definition of a version MitCad does not read",
            format!("version {major}"),
        );
    }
    let mut def = InstanceDefinition {
        id: Uuid::NIL,
        name: String::new(),
        objects: Vec::new(),
    };
    // The definition's id and name, in a chunk of their own with a
    // version (two ints). Files of format version 6 and 7 keep them as a
    // set of bits saying which fields follow (`TCODE_ANONYMOUS_CHUNK`):
    // 1 the id, 2 a second id, 4 an index, 8 the name. Files of version 8
    // keep them in another chunk (`TCODE_ANONYMOUS_CHUNK` + 2) as five
    // fields in order, each after a byte that says whether it is there: a
    // field not read here (refused when there), the id, a second id, an
    // index, the name.
    let names = c.chunk()?;
    let flagged = match names.typecode {
        archive::TCODE_ANONYMOUS_CHUNK => false,
        t if t == archive::TCODE_ANONYMOUS_CHUNK + 2 => true,
        t => {
            return archive::err_at(
                "a block definition's name is missing",
                format!("a chunk {t:08X} where its name was expected"),
            );
        }
    };
    let mut a = names.data;
    a.i32()?; // the chunk's version
    a.i32()?;
    if flagged {
        if a.bool()? {
            return err("a block definition in a form MitCad does not read");
        }
        if a.bool()? {
            def.id = a.uuid()?;
        }
        if a.bool()? {
            a.uuid()?;
        }
        if a.bool()? {
            a.i32()?;
        }
        if a.bool()? {
            def.name = a.wstring()?;
        }
    } else {
        let bits = a.u32()?;
        if bits & 1 != 0 {
            def.id = a.uuid()?;
        }
        if bits & 2 != 0 {
            a.uuid()?;
        }
        if bits & 4 != 0 {
            a.i32()?;
        }
        if bits & 8 != 0 {
            def.name = a.wstring()?;
        }
    }
    c.i32()?; // update type
    c.expect_chunk(archive::TCODE_ANONYMOUS_CHUNK, "a block definition's units")?;
    c.wstring()?; // description
    c.wstring()?; // URL
    c.wstring()?; // URL tag
    c.skip(48)?; // bounding box
    if c.bool()? {
        let n = c.count(16)?;
        def.objects = (0..n).map(|_| c.uuid()).collect::<Result<_>>()?;
    }
    Ok(Some(def))
}

fn read_attributes(r: &mut Reader, version: u32, writer: u64) -> Result<Attributes> {
    let mut a = Attributes::default();
    if version < 5 || writer < 200_712_190 {
        // The older format: its fields in order.
        let (major, minor) = r.chunk_version()?;
        if major != 1 {
            return archive::err_at(
                "object attributes of a version MitCad does not read",
                format!("version {major}"),
            );
        }
        a.id = r.uuid()?;
        a.layer = r.i32()?;
        r.i32()?; // material
        a.color = r.color()?;
        r.skip(20)?; // obsolete line style
        r.i32()?; // wire density
        a.mode = r.u8()?;
        a.color_source = color_source(r.u8()?);
        r.u8()?;
        r.u8()?;
        a.name = r.wstring()?;
        r.wstring()?; // URL
        a.visible = a.mode != 1;
        if minor >= 2 {
            r.i32_array()?; // groups
            a.visible = r.bool()?;
        }
        return Ok(a);
    }
    let (major, _) = r.chunk_version()?;
    if major != 2 {
        return archive::err_at(
            "object attributes of a version MitCad does not read",
            format!("version {major}"),
        );
    }
    a.id = r.uuid()?;
    a.layer = r.i32()?;
    // The values that differ from the defaults, each after its item id,
    // in increasing order; 0 ends them. Those after the visibility, mode
    // and colour source are not needed.
    let mut item = r.u8()?;
    while item != 0 && item <= 13 {
        match item {
            1 => a.name = r.wstring()?,
            2 => {
                r.wstring()?;
            }
            3 | 4 => {
                r.i32()?;
            }
            5 => {
                r.expect_chunk(archive::TCODE_ANONYMOUS_CHUNK, "rendering attributes")?;
            }
            6 => a.color = r.color()?,
            7 => {
                r.color()?;
            }
            8 => {
                r.f64()?;
            }
            9 => {
                r.u8()?;
            }
            10 => {
                r.i32()?;
            }
            11 => a.visible = r.bool()?,
            12 => a.mode = r.u8()?,
            13 => a.color_source = color_source(r.u8()?),
            _ => unreachable!(),
        }
        item = r.u8()?;
    }
    if a.mode == 1 {
        a.visible = false;
    }
    Ok(a)
}

fn color_source(c: u8) -> ColorSource {
    match c {
        1 => ColorSource::Object,
        2 => ColorSource::Material,
        3 => ColorSource::Parent,
        _ => ColorSource::Layer,
    }
}

/// An error with the words of what could not be read before its own,
/// and its place after `context` (an index, for agents and the log).
fn within(e: archive::Error, words: &str, context: String) -> archive::Error {
    let place = match (context.is_empty(), e.1.is_empty()) {
        (true, _) => e.1,
        (false, true) => context,
        (false, false) => format!("{context}: {}", e.1),
    };
    archive::Error::at(format!("{words}: {}", e.0), place)
}

/// An object record: None for a null object; a [`Failed`] (boxed, its index set
/// by the caller) with what is known of the object when it cannot be
/// read. Its data comes first, its attributes after it: when the data is
/// damaged, the attributes are still read, so that the object can be
/// named by its kind and layer.
fn read_object(
    r: &mut Reader,
    version: u32,
    writer: u64,
) -> std::result::Result<Option<Object>, Box<Failed>> {
    let failed = |kind: Option<String>, attributes: Option<Attributes>, error| {
        Box::new(Failed {
            index: 0,
            kind,
            attributes,
            error,
        })
    };
    r.expect_chunk(archive::TCODE_OBJECT_RECORD_TYPE, "an object's type")
        .map_err(|e| failed(None, None, e))?;
    let Some((uuid, mut d)) = geom::class_chunk(r).map_err(|e| failed(None, None, e))? else {
        return Ok(None);
    };
    let kind = geom::class::kind(uuid);
    let geometry = geom::read_geometry(uuid, &mut d);
    let mut attributes = Ok(Attributes::default());
    while !r.at_end() {
        let c = match r.chunk() {
            Ok(c) => c,
            Err(e) => {
                attributes = Err(e);
                break;
            }
        };
        match c.typecode {
            archive::TCODE_OBJECT_RECORD_ATTRIBUTES => {
                let mut a = c.data;
                attributes = read_attributes(&mut a, version, writer);
            }
            archive::TCODE_OBJECT_RECORD_END => break,
            _ => {}
        }
    }
    match (geometry, attributes) {
        (Ok(geometry), Ok(attributes)) => Ok(Some(Object {
            geometry,
            attributes,
        })),
        (Err(e), attributes) => Err(failed(Some(kind), attributes.ok(), e)),
        (Ok(_), Err(e)) => Err(failed(
            Some(kind),
            None,
            within(e, "its attributes cannot be read", String::new()),
        )),
    }
}

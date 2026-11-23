// SPDX-License-Identifier: MIT
//! Point cloud files (mitcad#2240): the points of a scan read for a point
//! cloud item of the design ([`crate::PointCloud`]).
//!
//! - PLY, ASCII and binary (little- and big-endian): the `vertex`
//!   element's `x`, `y`, `z` of any numeric type and its colour when it has
//!   one (`red`, `green`, `blue`, or `r`, `g`, `b`, or `diffuse_red` …;
//!   bytes, 16-bit values or 0 to 1 floats). Other properties and elements
//!   (faces, edges) are passed over.
//! - XYZ and PTS text: a point per line, `x y z`, then optionally `r g b`
//!   (six numbers) or an intensity and `r g b` (seven or more, as PTS files
//!   write it); spaces, tabs, commas or semicolons between the numbers;
//!   lines starting with `#` or `//` and a PTS file's count line are
//!   skipped.
//! - CSV: comma (or semicolon) separated, with an optional header naming
//!   the columns (`x`, `y`, `z`, and `r`, `g`, `b` or `red`, `green`,
//!   `blue`); without a header as XYZ text.
//!
//! A file with more points than asked for is thinned evenly: every n-th
//! point is kept, with a warning. The file is untrusted: every count is
//! checked against its size, and a point that is not three finite numbers
//! is left out and counted.

/// Points kept from a file unless another number is asked for.
pub const DEFAULT_MAX_POINTS: usize = 2_000_000;
/// The most points a cloud keeps.
pub const MAX_POINTS: usize = 20_000_000;
/// The largest point cloud file read, bytes.
pub const MAX_FILE_BYTES: usize = 1 << 30;

/// A point cloud as read from a file, in millimetres.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CloudRead {
    /// The points kept, in the file's order.
    pub points: Vec<[f64; 3]>,
    /// The colour of each point kept, when the file has colours.
    pub colors: Option<Vec<[u8; 3]>>,
    /// The points the file holds (more than kept when it was thinned).
    pub file_points: usize,
    /// Every n-th point was kept (1: all).
    pub step: usize,
    /// Points left out as not three finite numbers.
    pub unreadable: usize,
    /// In words, for the user (thinning, unreadable points).
    pub warnings: Vec<String>,
}

/// Whether a path names a point cloud file the reader takes, by its
/// extension (any case): `.ply`, `.xyz`, `.pts`, `.csv`, `.txt`.
pub fn is_point_cloud_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| {
            ["ply", "xyz", "pts", "csv", "txt"]
                .iter()
                .any(|x| e.eq_ignore_ascii_case(x))
        })
}

/// Reads a point cloud file: PLY by its first line whatever its name, CSV
/// by a `.csv` name, else XYZ text. Lengths are `scale` millimetres each;
/// at most `max_points` points are kept (thinned evenly beyond that).
pub fn read_point_cloud(
    bytes: &[u8],
    file_name: &str,
    scale: f64,
    max_points: usize,
) -> Result<CloudRead, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!(
            "the file is {} MB, more than the {} MB a point cloud is read from",
            bytes.len() >> 20,
            MAX_FILE_BYTES >> 20
        ));
    }
    if !(scale > 0.0 && scale.is_finite()) {
        return Err(format!(
            "the length unit's size must be greater than zero, got {scale}"
        ));
    }
    if max_points == 0 || max_points > MAX_POINTS {
        return Err(format!(
            "the most points to keep must be from 1 to {MAX_POINTS}, got {max_points}"
        ));
    }
    let mut read = if bytes.starts_with(b"ply") {
        ply::read(bytes, max_points)?
    } else {
        let csv = std::path::Path::new(file_name)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
        text::read(bytes, csv, max_points)?
    };
    if read.points.is_empty() {
        return Err("the file holds no points".to_owned());
    }
    if scale != 1.0 {
        for p in &mut read.points {
            *p = p.map(|c| c * scale);
        }
    }
    if read.step > 1 {
        read.warnings.push(format!(
            "the file holds {} points: every {} kept, {} in all",
            read.file_points,
            ordinal(read.step),
            read.points.len()
        ));
    }
    if read.unreadable > 0 {
        read.warnings.push(if read.unreadable == 1 {
            "a point that is not three finite numbers was left out".to_owned()
        } else {
            format!(
                "{} points that are not three finite numbers were left out",
                read.unreadable
            )
        });
    }
    Ok(read)
}

/// The step that keeps at most `max` of `count` points evenly.
pub fn thinning_step(count: usize, max: usize) -> usize {
    count.div_ceil(max.max(1)).max(1)
}

/// "2nd", "3rd", "10th".
fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, r) if r != 11 => "st",
        (2, r) if r != 12 => "nd",
        (3, r) if r != 13 => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// A colour channel as a byte: a 0 to 1 float scaled, else a byte value.
fn channel(value: f64, fraction: bool) -> u8 {
    let v = if fraction { value * 255.0 } else { value };
    // Clamped to 0..=255 first: the cast is exact.
    v.round().clamp(0.0, 255.0) as u8
}

/// Collects the points kept and counts the rest.
struct Keeper {
    out: CloudRead,
    /// Index of the next readable point.
    index: usize,
}

impl Keeper {
    fn new(count: usize, max: usize, colored: bool) -> Self {
        let step = thinning_step(count, max);
        Self {
            out: CloudRead {
                points: Vec::with_capacity(count.div_ceil(step).min(max)),
                colors: colored.then(Vec::new),
                file_points: count,
                step,
                unreadable: 0,
                warnings: Vec::new(),
            },
            index: 0,
        }
    }

    fn push(&mut self, point: [f64; 3], color: Option<[u8; 3]>) {
        if !point.iter().all(|c| c.is_finite()) {
            self.out.unreadable += 1;
            return;
        }
        let keep = self.index.is_multiple_of(self.out.step);
        self.index += 1;
        if !keep {
            return;
        }
        self.out.points.push(point);
        if let Some(colors) = &mut self.out.colors {
            colors.push(color.unwrap_or([255; 3]));
        }
    }

    fn unreadable(&mut self) {
        self.out.unreadable += 1;
    }

    fn finish(mut self) -> CloudRead {
        // The unreadable ones are no points of the file.
        self.out.file_points = self.index;
        self.out
    }
}

mod ply {
    //! PLY: the header, then the vertex element's rows.

    use super::{CloudRead, Keeper, MAX_POINTS, channel};

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Format {
        Ascii,
        Little,
        Big,
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Scalar {
        I8,
        U8,
        I16,
        U16,
        I32,
        U32,
        F32,
        F64,
    }

    impl Scalar {
        fn parse(name: &str) -> Option<Self> {
            Some(match name {
                "char" | "int8" => Self::I8,
                "uchar" | "uint8" => Self::U8,
                "short" | "int16" => Self::I16,
                "ushort" | "uint16" => Self::U16,
                "int" | "int32" => Self::I32,
                "uint" | "uint32" => Self::U32,
                "float" | "float32" => Self::F32,
                "double" | "float64" => Self::F64,
                _ => return None,
            })
        }

        fn size(self) -> usize {
            match self {
                Self::I8 | Self::U8 => 1,
                Self::I16 | Self::U16 => 2,
                Self::I32 | Self::U32 | Self::F32 => 4,
                Self::F64 => 8,
            }
        }

        fn is_float(self) -> bool {
            matches!(self, Self::F32 | Self::F64)
        }

        /// The value at the start of `b` (at least `size` bytes).
        fn value(self, b: &[u8], little: bool) -> f64 {
            macro_rules! num {
                ($t:ty, $n:expr) => {{
                    let mut a = [0u8; $n];
                    a.copy_from_slice(&b[..$n]);
                    if little {
                        <$t>::from_le_bytes(a)
                    } else {
                        <$t>::from_be_bytes(a)
                    }
                }};
            }
            match self {
                Self::I8 => f64::from(num!(i8, 1)),
                Self::U8 => f64::from(num!(u8, 1)),
                Self::I16 => f64::from(num!(i16, 2)),
                Self::U16 => f64::from(num!(u16, 2)),
                Self::I32 => f64::from(num!(i32, 4)),
                Self::U32 => f64::from(num!(u32, 4)),
                Self::F32 => f64::from(num!(f32, 4)),
                Self::F64 => num!(f64, 8),
            }
        }
    }

    #[derive(Debug, Clone)]
    enum Property {
        Scalar(String, Scalar),
        /// A list: its count's type and its items' type.
        List(Scalar, Scalar),
    }

    #[derive(Debug, Clone)]
    struct Element {
        name: String,
        count: usize,
        properties: Vec<Property>,
    }

    struct Header {
        format: Format,
        elements: Vec<Element>,
        /// Where the data starts.
        body: usize,
    }

    fn header(bytes: &[u8]) -> Result<Header, String> {
        let end = find(bytes, b"end_header")
            .ok_or_else(|| "the PLY file has no end_header line".to_owned())?;
        // The data starts after the line's end (\n or \r\n).
        let mut body = end + b"end_header".len();
        while body < bytes.len() && bytes[body] != b'\n' {
            body += 1;
        }
        body = (body + 1).min(bytes.len());
        let text = std::str::from_utf8(&bytes[..end])
            .map_err(|_| "the PLY file's header is not text".to_owned())?;
        let mut format = None;
        let mut elements: Vec<Element> = Vec::new();
        for line in text.lines().skip(1) {
            let words: Vec<&str> = line.split_whitespace().collect();
            match words.as_slice() {
                ["format", kind, ..] => {
                    format = Some(match *kind {
                        "ascii" => Format::Ascii,
                        "binary_little_endian" => Format::Little,
                        "binary_big_endian" => Format::Big,
                        other => return Err(format!("the PLY format {other} is not known")),
                    });
                }
                ["element", name, count] => {
                    let count = count
                        .parse::<usize>()
                        .map_err(|_| format!("the PLY element {name} has no count"))?;
                    elements.push(Element {
                        name: (*name).to_owned(),
                        count,
                        properties: Vec::new(),
                    });
                }
                ["property", "list", count, item, name] => {
                    let element = elements
                        .last_mut()
                        .ok_or_else(|| "a PLY property comes before its element".to_owned())?;
                    let (Some(count), Some(item)) = (Scalar::parse(count), Scalar::parse(item))
                    else {
                        return Err(format!("the PLY list {name} has a type that is not known"));
                    };
                    element.properties.push(Property::List(count, item));
                }
                ["property", kind, name] => {
                    let element = elements
                        .last_mut()
                        .ok_or_else(|| "a PLY property comes before its element".to_owned())?;
                    let kind = Scalar::parse(kind).ok_or_else(|| {
                        format!("the PLY property {name} has a type that is not known")
                    })?;
                    element
                        .properties
                        .push(Property::Scalar((*name).to_owned(), kind));
                }
                _ => {} // comments, obj_info, blank lines
            }
        }
        let format = format.ok_or_else(|| "the PLY file names no format".to_owned())?;
        Ok(Header {
            format,
            elements,
            body,
        })
    }

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        // The header is short: within the first 64 kB.
        let limit = haystack.len().min(1 << 16);
        haystack[..limit]
            .windows(needle.len())
            .position(|w| w == needle)
    }

    /// Which of the vertex's properties are the coordinates and colours,
    /// by their index among its properties.
    struct Columns {
        xyz: [usize; 3],
        rgb: Option<[usize; 3]>,
    }

    fn columns(vertex: &Element) -> Result<Columns, String> {
        let index = |names: &[&str]| {
            vertex.properties.iter().position(|p| match p {
                Property::Scalar(n, _) => names.iter().any(|m| n.eq_ignore_ascii_case(m)),
                Property::List(..) => false,
            })
        };
        let (Some(x), Some(y), Some(z)) = (index(&["x"]), index(&["y"]), index(&["z"])) else {
            return Err("the PLY file's vertices have no x, y and z".to_owned());
        };
        let rgb = match (
            index(&["red", "r", "diffuse_red"]),
            index(&["green", "g", "diffuse_green"]),
            index(&["blue", "b", "diffuse_blue"]),
        ) {
            (Some(r), Some(g), Some(b)) => Some([r, g, b]),
            _ => None,
        };
        Ok(Columns {
            xyz: [x, y, z],
            rgb,
        })
    }

    /// A colour channel of a property's value by its type.
    fn color_of(value: f64, kind: Scalar) -> u8 {
        match kind {
            Scalar::U16 | Scalar::I16 => channel(value / 257.0, false),
            k if k.is_float() => channel(value, true),
            _ => channel(value, false),
        }
    }

    pub(super) fn read(bytes: &[u8], max: usize) -> Result<CloudRead, String> {
        let header = header(bytes)?;
        let at = header
            .elements
            .iter()
            .position(|e| e.name == "vertex")
            .ok_or_else(|| "the PLY file has no vertex element".to_owned())?;
        let vertex = &header.elements[at];
        if vertex.count > MAX_POINTS.saturating_mul(16) {
            return Err(format!(
                "the PLY file says it has {} points, too many to read",
                vertex.count
            ));
        }
        let columns = columns(vertex)?;
        let kinds: Vec<Scalar> = vertex
            .properties
            .iter()
            .map(|p| match p {
                Property::Scalar(_, k) => *k,
                Property::List(_, k) => *k,
            })
            .collect();
        let mut keeper = Keeper::new(vertex.count, max, columns.rgb.is_some());
        let data = &bytes[header.body..];
        match header.format {
            Format::Ascii => ascii(
                data,
                &header.elements[..at],
                vertex,
                &columns,
                &kinds,
                &mut keeper,
            )?,
            Format::Little | Format::Big => {
                let little = header.format == Format::Little;
                binary(
                    data,
                    &header.elements[..at],
                    vertex,
                    &columns,
                    &kinds,
                    little,
                    &mut keeper,
                )?;
            }
        }
        Ok(keeper.finish())
    }

    fn ascii(
        data: &[u8],
        before: &[Element],
        vertex: &Element,
        columns: &Columns,
        kinds: &[Scalar],
        keeper: &mut Keeper,
    ) -> Result<(), String> {
        let text = String::from_utf8_lossy(data);
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        // Elements before the vertices: a line per item.
        let skip: usize = before.iter().map(|e| e.count).sum();
        for _ in 0..skip {
            lines
                .next()
                .ok_or_else(|| "the PLY file ends before its vertices".to_owned())?;
        }
        let has_lists = vertex
            .properties
            .iter()
            .any(|p| matches!(p, Property::List(..)));
        for _ in 0..vertex.count {
            let Some(line) = lines.next() else {
                return Err("the PLY file ends before its last vertex".to_owned());
            };
            let values: Vec<f64> = line
                .split_whitespace()
                .map(|w| w.parse::<f64>().unwrap_or(f64::NAN))
                .collect();
            // A vertex with a list property: each value's place moves by
            // the list's length; read it in order.
            let row: Vec<f64> = if has_lists {
                let mut row = Vec::with_capacity(vertex.properties.len());
                let mut k = 0;
                for p in &vertex.properties {
                    match p {
                        Property::Scalar(..) => {
                            row.push(values.get(k).copied().unwrap_or(f64::NAN));
                            k += 1;
                        }
                        Property::List(..) => {
                            let n = values.get(k).copied().unwrap_or(0.0);
                            row.push(f64::NAN);
                            // A count is a small whole number.
                            k += 1 + if n.is_finite() && n >= 0.0 {
                                n as usize
                            } else {
                                0
                            };
                        }
                    }
                }
                row
            } else {
                values
            };
            if row.len() < vertex.properties.len() {
                keeper.unreadable();
                continue;
            }
            let point = columns.xyz.map(|i| row[i]);
            let color = columns
                .rgb
                .map(|rgb| rgb.map(|i| color_of(row[i], kinds[i])));
            keeper.push(point, color);
        }
        Ok(())
    }

    /// The size of an element's row starting at `data`, reading its lists'
    /// counts; None when the data ends.
    fn row_size(data: &[u8], element: &Element, little: bool) -> Option<usize> {
        let mut at = 0usize;
        for p in &element.properties {
            match p {
                Property::Scalar(_, k) => at = at.checked_add(k.size())?,
                Property::List(count, item) => {
                    let n = count.value(data.get(at..at + count.size())?, little);
                    if !(n.is_finite() && n >= 0.0) {
                        return None;
                    }
                    // A count is a whole number within the file.
                    let n = n as usize;
                    at = at
                        .checked_add(count.size())?
                        .checked_add(n.checked_mul(item.size())?)?;
                }
            }
        }
        (at <= data.len()).then_some(at)
    }

    fn binary(
        data: &[u8],
        before: &[Element],
        vertex: &Element,
        columns: &Columns,
        kinds: &[Scalar],
        little: bool,
        keeper: &mut Keeper,
    ) -> Result<(), String> {
        let ended = || "the PLY file ends before its last vertex".to_owned();
        let mut at = 0usize;
        for element in before {
            for _ in 0..element.count {
                at += row_size(&data[at..], element, little).ok_or_else(ended)?;
            }
        }
        let fixed = vertex
            .properties
            .iter()
            .all(|p| matches!(p, Property::Scalar(..)));
        // Each property's offset in a row without lists.
        let offsets: Vec<usize> = kinds
            .iter()
            .scan(0usize, |o, k| {
                let here = *o;
                *o += k.size();
                Some(here)
            })
            .collect();
        let stride: usize = kinds.iter().map(|k| k.size()).sum();
        if fixed {
            let need = vertex.count.checked_mul(stride).ok_or_else(ended)?;
            if data.len().saturating_sub(at) < need {
                return Err(ended());
            }
        }
        let value =
            |row: &[u8], offsets: &[usize], i: usize| kinds[i].value(&row[offsets[i]..], little);
        let mut own = Vec::with_capacity(kinds.len());
        for _ in 0..vertex.count {
            let size = if fixed {
                stride
            } else {
                // Offsets of this row, its lists' lengths read (row_size
                // checked that the row is within the data).
                let size = row_size(&data[at..], vertex, little).ok_or_else(ended)?;
                own.clear();
                let mut o = at;
                for p in &vertex.properties {
                    own.push(o - at);
                    match p {
                        Property::Scalar(_, k) => o += k.size(),
                        Property::List(count, item) => {
                            // A count is a whole number within the row.
                            let n = count.value(&data[o..], little) as usize;
                            o += count.size() + n * item.size();
                        }
                    }
                }
                size
            };
            let offsets: &[usize] = if fixed { &offsets } else { &own };
            let row = data.get(at..at + size).ok_or_else(ended)?;
            let point = columns.xyz.map(|i| value(row, offsets, i));
            let color = columns
                .rgb
                .map(|rgb| rgb.map(|i| color_of(value(row, offsets, i), kinds[i])));
            keeper.push(point, color);
            at += size;
        }
        Ok(())
    }
}

mod text {
    //! XYZ, PTS and CSV text.

    use super::{CloudRead, Keeper, channel};

    /// The columns a CSV header names.
    struct Named {
        xyz: [usize; 3],
        rgb: Option<[usize; 3]>,
    }

    fn split(line: &str) -> impl Iterator<Item = &str> {
        line.split(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .filter(|w| !w.is_empty())
    }

    fn comment(line: &str) -> bool {
        line.starts_with('#') || line.starts_with("//")
    }

    /// The columns of a CSV header line, when it names x, y and z.
    fn header(line: &str) -> Option<Named> {
        let names: Vec<String> = line
            .split([',', ';', '\t'])
            .map(|w| w.trim().trim_matches('"').to_ascii_lowercase())
            .collect();
        let at = |options: &[&str]| names.iter().position(|n| options.contains(&n.as_str()));
        let xyz = [at(&["x"])?, at(&["y"])?, at(&["z"])?];
        let rgb = match (at(&["r", "red"]), at(&["g", "green"]), at(&["b", "blue"])) {
            (Some(r), Some(g), Some(b)) => Some([r, g, b]),
            _ => None,
        };
        Some(Named { xyz, rgb })
    }

    /// A line's point and colour by position: `x y z`, `x y z r g b`, or
    /// `x y z i r g b` (PTS); None for a line that is no point (fewer than
    /// three numbers).
    type Read = Option<([f64; 3], Option<[f64; 3]>)>;

    fn by_position(line: &str) -> Read {
        let values: Vec<f64> = split(line)
            .map(|w| w.parse::<f64>().unwrap_or(f64::NAN))
            .collect();
        if values.len() < 3 {
            return None;
        }
        let point = [values[0], values[1], values[2]];
        let color = match values.len() {
            6 => Some([values[3], values[4], values[5]]),
            n if n >= 7 => Some([values[4], values[5], values[6]]),
            _ => None,
        };
        Some((point, color))
    }

    fn by_name(line: &str, named: &Named) -> Read {
        let values: Vec<f64> = line
            .split([',', ';', '\t'])
            .map(|w| {
                w.trim()
                    .trim_matches('"')
                    .parse::<f64>()
                    .unwrap_or(f64::NAN)
            })
            .collect();
        let get = |i: usize| values.get(i).copied();
        let point = [get(named.xyz[0])?, get(named.xyz[1])?, get(named.xyz[2])?];
        let color = named
            .rgb
            .and_then(|[r, g, b]| Some([get(r)?, get(g)?, get(b)?]));
        Some((point, color))
    }

    /// Whether a line starts like a number (a point's line, or a PTS count).
    fn numeric(line: &str) -> bool {
        line.chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.'))
    }

    pub(super) fn read(bytes: &[u8], csv: bool, max: usize) -> Result<CloudRead, String> {
        let text = String::from_utf8_lossy(bytes);
        let mut lines = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !comment(l))
            .peekable();
        // A CSV header names the columns; a header of another text file is
        // passed over as a line that is no point.
        let named = match lines.peek() {
            Some(first) if !numeric(first) => {
                let named = if csv { header(first) } else { None };
                lines.next();
                named
            }
            _ => None,
        };
        let rows: Vec<&str> = lines.collect();
        let parse = |line: &str| match &named {
            Some(named) => by_name(line, named),
            None => by_position(line),
        };
        // The points, counted first to thin evenly; the colours are bytes
        // unless every channel of the first coloured point is at most 1.
        let mut count = 0usize;
        let mut fraction = None;
        for line in &rows {
            if let Some((_, color)) = parse(line) {
                count += 1;
                if fraction.is_none()
                    && let Some(c) = color
                {
                    fraction = Some(c.iter().all(|v| *v <= 1.0));
                }
            }
        }
        let colored = fraction.is_some();
        let fraction = fraction.unwrap_or(false);
        let mut keeper = Keeper::new(count, max, colored);
        for line in &rows {
            match parse(line) {
                Some((point, color)) => {
                    let color = color.map(|c| c.map(|v| channel(v, fraction)));
                    keeper.push(point, color);
                }
                // A PTS file's count, or a line that is no point.
                None if split(line).count() <= 1 => {}
                None => keeper.unreadable(),
            }
        }
        Ok(keeper.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ascii_ply(points: &[[f64; 3]], colors: Option<&[[u8; 3]]>) -> Vec<u8> {
        let mut text = format!(
            "ply\nformat ascii 1.0\ncomment made by a test\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\n",
            points.len()
        );
        if colors.is_some() {
            text.push_str("property uchar red\nproperty uchar green\nproperty uchar blue\n");
        }
        text.push_str("element face 1\nproperty list uchar int vertex_indices\nend_header\n");
        for (i, p) in points.iter().enumerate() {
            text.push_str(&format!("{} {} {}", p[0], p[1], p[2]));
            if let Some(c) = colors {
                text.push_str(&format!(" {} {} {}", c[i][0], c[i][1], c[i][2]));
            }
            text.push('\n');
        }
        text.push_str("3 0 1 2\n");
        text.into_bytes()
    }

    fn binary_ply(points: &[[f64; 3]], little: bool) -> Vec<u8> {
        let format = if little {
            "binary_little_endian"
        } else {
            "binary_big_endian"
        };
        // A face element before the vertices, to be passed over; doubles
        // and a byte of intensity between the coordinates and the colour.
        let mut bytes = format!(
            "ply\nformat {format} 1.0\nelement face 1\nproperty list uchar int vertex_indices\nelement vertex {}\nproperty double x\nproperty double y\nproperty double z\nproperty uchar intensity\nproperty ushort red\nproperty ushort green\nproperty ushort blue\nend_header\n",
            points.len()
        )
        .into_bytes();
        let int = |v: i32| {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        bytes.push(3);
        for i in 0..3 {
            bytes.extend_from_slice(&int(i));
        }
        for p in points {
            for c in p {
                bytes.extend_from_slice(&if little {
                    c.to_le_bytes()
                } else {
                    c.to_be_bytes()
                });
            }
            bytes.push(7);
            for v in [65535u16, 0, 32896] {
                bytes.extend_from_slice(&if little {
                    v.to_le_bytes()
                } else {
                    v.to_be_bytes()
                });
            }
        }
        bytes
    }

    const POINTS: [[f64; 3]; 4] = [
        [0.0, 0.0, 0.0],
        [1.5, -2.0, 3.25],
        [10.0, 20.0, 30.0],
        [-4.0, 5.5, 0.125],
    ];

    #[test]
    fn ascii_ply_points_and_colours() {
        let colors = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [10, 20, 30]];
        let read =
            read_point_cloud(&ascii_ply(&POINTS, Some(&colors)), "scan.ply", 1.0, 100).unwrap();
        assert_eq!(read.points, POINTS.to_vec());
        assert_eq!(read.colors, Some(colors.to_vec()));
        assert_eq!(read.file_points, 4);
        assert_eq!(read.step, 1);
        assert!(read.warnings.is_empty());
        // Without colours, and in centimetres.
        let read = read_point_cloud(&ascii_ply(&POINTS, None), "scan.ply", 10.0, 100).unwrap();
        assert_eq!(read.colors, None);
        assert_eq!(read.points[1], [15.0, -20.0, 32.5]);
    }

    #[test]
    fn binary_ply_both_byte_orders() {
        for little in [true, false] {
            let read =
                read_point_cloud(&binary_ply(&POINTS, little), "scan.PLY", 1.0, 100).unwrap();
            assert_eq!(read.points, POINTS.to_vec(), "little endian {little}");
            assert_eq!(read.colors.as_ref().unwrap()[0], [255, 0, 128]);
        }
    }

    #[test]
    fn a_ply_that_ends_early_is_refused() {
        let mut bytes = binary_ply(&POINTS, true);
        bytes.truncate(bytes.len() - 5);
        let error = read_point_cloud(&bytes, "scan.ply", 1.0, 100).unwrap_err();
        assert!(error.contains("ends before"), "{error}");
        assert!(read_point_cloud(b"ply\nformat ascii 1.0\n", "a.ply", 1.0, 10).is_err());
    }

    #[test]
    fn xyz_and_pts_text() {
        let xyz = "# a scan\n0 0 0\n1.5,-2,3.25\n10;20;30\n-4\t5.5\t0.125\n";
        let read = read_point_cloud(xyz.as_bytes(), "scan.xyz", 1.0, 100).unwrap();
        assert_eq!(read.points, POINTS.to_vec());
        assert_eq!(read.colors, None);
        // PTS: a count line, then x y z intensity r g b.
        let pts = "2\n1 2 3 -500 255 128 0\n4 5 6 12 0 0 255\n";
        let read = read_point_cloud(pts.as_bytes(), "scan.pts", 1.0, 100).unwrap();
        assert_eq!(read.points, vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        assert_eq!(read.colors, Some(vec![[255, 128, 0], [0, 0, 255]]));
        // Colours from 0 to 1.
        let read = read_point_cloud(b"1 2 3 1 0.5 0\n", "a.xyz", 1.0, 10).unwrap();
        assert_eq!(read.colors, Some(vec![[255, 128, 0]]));
        // A point that is no number is left out with a warning.
        let read = read_point_cloud(b"1 2 3\n4 five 6\n7 8 9\n", "a.xyz", 1.0, 10).unwrap();
        assert_eq!(read.points.len(), 2);
        assert_eq!(read.unreadable, 1);
        assert_eq!(read.warnings.len(), 1);
        assert!(read_point_cloud(b"no points here\n", "a.xyz", 1.0, 10).is_err());
    }

    #[test]
    fn csv_with_a_header_names_its_columns() {
        let csv = "id,Z,X,Y,red,green,blue\n1,3,1,2,255,0,0\n2,6,4,5,0,255,0\n";
        let read = read_point_cloud(csv.as_bytes(), "scan.csv", 1.0, 100).unwrap();
        assert_eq!(read.points, vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        assert_eq!(read.colors, Some(vec![[255, 0, 0], [0, 255, 0]]));
        // Without a header: by position.
        let read = read_point_cloud(b"1,2,3\n4,5,6\n", "scan.csv", 1.0, 100).unwrap();
        assert_eq!(read.points.len(), 2);
    }

    #[test]
    fn thinning_keeps_every_nth_point() {
        let text: String = (0..10).map(|i| format!("{i} 0 0\n")).collect();
        let read = read_point_cloud(text.as_bytes(), "a.xyz", 1.0, 4).unwrap();
        // 10 points, at most 4: every 3rd, starting with the first.
        assert_eq!(read.step, 3);
        let xs: Vec<f64> = read.points.iter().map(|p| p[0]).collect();
        assert_eq!(xs, vec![0.0, 3.0, 6.0, 9.0]);
        assert_eq!(read.file_points, 10);
        assert!(
            read.warnings[0].contains("every 3rd kept"),
            "{:?}",
            read.warnings
        );
        let ply = read_point_cloud(&binary_ply(&POINTS, true), "a.ply", 1.0, 2).unwrap();
        assert_eq!(ply.points, vec![POINTS[0], POINTS[2]]);
        assert_eq!(thinning_step(10, 10), 1);
        assert_eq!(thinning_step(11, 10), 2);
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(11), "11th");
    }

    #[test]
    fn paths_by_extension() {
        assert!(is_point_cloud_path("/scans/part.PLY"));
        assert!(is_point_cloud_path("part.xyz"));
        assert!(is_point_cloud_path("part.csv"));
        assert!(!is_point_cloud_path("part.stl"));
    }
}

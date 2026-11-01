// SPDX-License-Identifier: MIT
//! HEIF files (mitcad#850): the size of the primary image of a HEIF file
//! (`.heic`, `.heif`) coded with HEVC, as it is shown: after the
//! transformations the file asks for in their order (`irot`'s quarter
//! turns swap the width and height, `clap` crops, `imir` mirrors without
//! changing them), which the application's decoder applies too.
//!
//! Read: still images whose primary item is an HEVC image (`hvc1`), a grid
//! of HEVC tiles (`grid`) or an identity-derived HEVC image (`iden`), with 8
//! or 10 bits per channel, with or without an alpha plane. Refused with a
//! message that says why: AVIF files (AV1), other codecs (JPEG, JPEG 2000,
//! VVC, uncompressed images), image sequences without a still image,
//! protected items and more than 10 bits per channel. A HEIF file states no
//! size of a pixel here.

use super::{ImageHeader, bytes_at, u16_be, u32_be};

/// The brands of the HEIF family (ISO/IEC 23008-12 and 23000-22) that a
/// file's `ftyp` names: still images, image sequences, HEVC and AV1 ones.
const BRANDS: [&[u8; 4]; 12] = [
    b"mif1", b"mif2", b"msf1", b"miaf", b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx",
    b"avif", b"avis",
];

/// The brands of image sequences.
const SEQUENCES: [&[u8; 4]; 4] = [b"msf1", b"hevc", b"hevx", b"avis"];

/// The most bits per channel taken.
const MAX_BITS: u8 = 10;

fn cut() -> String {
    "the HEIF file ends inside a box".to_owned()
}

fn u64_be(bytes: &[u8], at: usize) -> Option<u64> {
    bytes_at(bytes, at).map(u64::from_be_bytes)
}

/// A box: its type and its content (after its header).
type Child<'a> = ([u8; 4], &'a [u8]);

/// The boxes in `bytes`, one after the other: their types and contents.
fn boxes(bytes: &[u8]) -> Result<Vec<Child<'_>>, String> {
    let mut list = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        let size = u32_be(bytes, at).ok_or_else(cut)?;
        let kind: [u8; 4] = bytes_at(bytes, at + 4).ok_or_else(cut)?;
        let (header, size) = match size {
            // The size in the 64 bits after the type.
            1 => (16usize, u64_be(bytes, at + 8).ok_or_else(cut)?),
            // To the end of the file.
            0 => (8, (bytes.len() - at) as u64),
            size => (8, u64::from(size)),
        };
        let size = usize::try_from(size).map_err(|_| cut())?;
        if size < header {
            return Err(format!(
                "the HEIF file's box “{}” is shorter than its header",
                name(&kind)
            ));
        }
        let end = at
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(cut)?;
        list.push((kind, &bytes[at + header..end]));
        at = end;
    }
    Ok(list)
}

/// A four-character code as text.
fn name(kind: &[u8]) -> String {
    kind.iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '?'
            }
        })
        .collect()
}

/// The content of the first box of a type.
fn find<'a>(list: &[Child<'a>], kind: &[u8; 4]) -> Option<&'a [u8]> {
    list.iter().find(|(k, _)| k == kind).map(|(_, body)| *body)
}

/// A full box's version and the content after its version and flags.
fn full(body: &[u8]) -> Result<(u8, u32, &[u8]), String> {
    let head = u32_be(body, 0).ok_or_else(cut)?;
    Ok(((head >> 24) as u8, head & 0x00ff_ffff, &body[4..]))
}

/// An item ID of 16 or 32 bits at `at`, and where the next field starts.
fn item_id(bytes: &[u8], at: usize, wide: bool) -> Result<(u32, usize), String> {
    if wide {
        Ok((u32_be(bytes, at).ok_or_else(cut)?, at + 4))
    } else {
        Ok((u16_be(bytes, at).ok_or_else(cut)?, at + 2))
    }
}

/// Whether `bytes` start with a `ftyp` box that names a brand of the HEIF
/// family.
pub(super) fn is_heif(bytes: &[u8]) -> bool {
    brands(bytes).is_some_and(|brands| brands.iter().any(|brand| BRANDS.contains(brand)))
}

/// The major and compatible brands of the file's `ftyp` box.
fn brands(bytes: &[u8]) -> Option<Vec<&[u8; 4]>> {
    if bytes.get(4..8) != Some(b"ftyp".as_slice()) {
        return None;
    }
    let size = usize::try_from(u32_be(bytes, 0)?).ok()?;
    let body = bytes.get(8..size)?;
    let mut list: Vec<&[u8; 4]> = Vec::new();
    list.push(body.get(..4)?.try_into().ok()?);
    // The minor version, then the compatible brands.
    for brand in body.get(8..)?.as_chunks::<4>().0 {
        list.push(brand);
    }
    Some(list)
}

/// An item of the `iinf` box.
struct Item {
    id: u32,
    kind: [u8; 4],
    protected: bool,
}

fn items(iinf: &[u8]) -> Result<Vec<Item>, String> {
    let (version, _, rest) = full(iinf)?;
    let skip = if version == 0 { 2 } else { 4 };
    let mut list = Vec::new();
    for (kind, body) in boxes(rest.get(skip..).ok_or_else(cut)?)? {
        if &kind != b"infe" {
            continue;
        }
        let (version, _, infe) = full(body)?;
        // Versions 0 and 1 have no item type: no image.
        let item = match version {
            2 => Item {
                id: u16_be(infe, 0).ok_or_else(cut)?,
                protected: u16_be(infe, 2).ok_or_else(cut)? != 0,
                kind: bytes_at(infe, 4).ok_or_else(cut)?,
            },
            3 => Item {
                id: u32_be(infe, 0).ok_or_else(cut)?,
                protected: u16_be(infe, 4).ok_or_else(cut)? != 0,
                kind: bytes_at(infe, 6).ok_or_else(cut)?,
            },
            _ => continue,
        };
        list.push(item);
    }
    Ok(list)
}

/// The items an item refers to by a reference type (`dimg`: the images a
/// derived image is made of), in their order.
fn references(iref: Option<&[u8]>, from: u32, kind: &[u8; 4]) -> Result<Vec<u32>, String> {
    let Some(iref) = iref else {
        return Ok(Vec::new());
    };
    let (version, _, rest) = full(iref)?;
    let wide = version != 0;
    for (reference, body) in boxes(rest)? {
        if &reference != kind {
            continue;
        }
        let (id, at) = item_id(body, 0, wide)?;
        if id != from {
            continue;
        }
        let count = u16_be(body, at).ok_or_else(cut)?;
        let mut at = at + 2;
        let mut list = Vec::new();
        for _ in 0..count {
            let (to, next) = item_id(body, at, wide)?;
            list.push(to);
            at = next;
        }
        return Ok(list);
    }
    Ok(Vec::new())
}

/// The properties of an item, in the order the `ipma` boxes associate
/// them: their types and contents.
fn properties(iprp: &[u8], item: u32) -> Result<Vec<Child<'_>>, String> {
    let children = boxes(iprp)?;
    let ipco = find(&children, b"ipco")
        .ok_or_else(|| "the HEIF file has no item properties (ipco)".to_owned())?;
    let all = boxes(ipco)?;
    let mut list = Vec::new();
    for (kind, ipma) in &children {
        if kind != b"ipma" {
            continue;
        }
        let (version, flags, body) = full(ipma)?;
        let count = u32_be(body, 0).ok_or_else(cut)?;
        let mut at = 4usize;
        for _ in 0..count {
            let (id, next) = item_id(body, at, version >= 1)?;
            let associations = *body.get(next).ok_or_else(cut)?;
            at = next + 1;
            for _ in 0..associations {
                // The essential bit, then the property's index from 1 (0: none).
                let index = if flags & 1 != 0 {
                    let value = u16_be(body, at).ok_or_else(cut)?;
                    at += 2;
                    value & 0x7fff
                } else {
                    let value = u32::from(*body.get(at).ok_or_else(cut)?);
                    at += 1;
                    value & 0x7f
                };
                if id != item || index == 0 {
                    continue;
                }
                let property = all.get(index as usize - 1).ok_or_else(|| {
                    format!("the HEIF file's item {item} has a property {index} that is not there")
                })?;
                list.push(*property);
            }
        }
    }
    Ok(list)
}

/// The bits per channel of an HEVC item's `hvcC` property.
fn hevc_bits(iprp: &[u8], item: u32) -> Result<u8, String> {
    let list = properties(iprp, item)?;
    let hvcc = find(&list, b"hvcC")
        .ok_or_else(|| "the HEIF image has no HEVC decoder configuration (hvcC)".to_owned())?;
    let luma = hvcc.get(17).ok_or_else(cut)?;
    let chroma = hvcc.get(18).ok_or_else(cut)?;
    Ok((luma & 7).max(chroma & 7) + 8)
}

/// A clean aperture's width or height: a fraction of positive numbers,
/// rounded.
fn aperture(property: &[u8], at: usize) -> Result<u32, String> {
    let numerator = u32_be(property, at).ok_or_else(cut)?;
    let denominator = u32_be(property, at + 4).ok_or_else(cut)?;
    // Signed 32-bit numbers in the file.
    if denominator == 0 || numerator > i32::MAX as u32 || denominator > i32::MAX as u32 {
        return Err("the HEIF image's clean aperture (clap) is not a size".to_owned());
    }
    Ok(((f64::from(numerator) / f64::from(denominator)) + 0.5).floor() as u32)
}

/// The header of a HEIF file's primary image, or why it is none MitCad
/// reads.
pub(super) fn read(bytes: &[u8]) -> Result<ImageHeader, String> {
    let top = boxes(bytes)?;
    let Some(meta) = find(&top, b"meta") else {
        let sequence = brands(bytes)
            .is_some_and(|brands| brands.iter().any(|brand| SEQUENCES.contains(brand)));
        return Err(if sequence {
            "the HEIF file is an image sequence without a still image; MitCad reads still images"
                .to_owned()
        } else {
            "the HEIF file has no image (no meta box)".to_owned()
        });
    };
    let (_, _, meta) = full(meta)?;
    let meta = boxes(meta)?;
    let handler = find(&meta, b"hdlr").and_then(|hdlr| hdlr.get(8..12));
    if handler != Some(b"pict".as_slice()) {
        return Err("the HEIF file's metadata is not that of a picture".to_owned());
    }
    let pitm = find(&meta, b"pitm")
        .ok_or_else(|| "the HEIF file names no primary image (pitm)".to_owned())?;
    let (version, _, pitm) = full(pitm)?;
    let (primary, _) = item_id(pitm, 0, version != 0)?;
    let iinf = find(&meta, b"iinf")
        .ok_or_else(|| "the HEIF file has no item information (iinf)".to_owned())?;
    let all_items = items(iinf)?;
    let item = |id: u32| {
        all_items
            .iter()
            .find(|item| item.id == id)
            .ok_or_else(|| format!("the HEIF file has no item {id}"))
    };
    let image = item(primary)?;
    let iprp = find(&meta, b"iprp")
        .ok_or_else(|| "the HEIF file has no item properties (iprp)".to_owned())?;
    let iref = find(&meta, b"iref");

    // The coded HEVC images: the item itself, or what it is derived from.
    let coded = match &image.kind {
        b"hvc1" => vec![image],
        b"grid" | b"iden" => {
            let parts = references(iref, primary, b"dimg")?;
            if parts.is_empty() {
                return Err("the HEIF image's tiles are not there (dimg)".to_owned());
            }
            parts.into_iter().map(item).collect::<Result<Vec<_>, _>>()?
        }
        b"av01" => {
            return Err(
                "the file is an AVIF image (AV1); MitCad reads HEIF images coded with HEVC"
                    .to_owned(),
            );
        }
        kind => {
            return Err(format!(
                "the HEIF image is coded as “{}”; MitCad reads HEIF images coded with HEVC",
                name(kind)
            ));
        }
    };
    for part in std::iter::once(image).chain(coded.iter().copied()) {
        if part.protected {
            return Err("the HEIF image is protected (encrypted)".to_owned());
        }
    }
    for part in &coded {
        if &part.kind != b"hvc1" {
            return Err(format!(
                "the HEIF image's tiles are coded as “{}”; MitCad reads HEIF images coded with HEVC",
                name(&part.kind)
            ));
        }
        let bits = hevc_bits(iprp, part.id)?;
        if bits > MAX_BITS {
            return Err(format!(
                "the HEIF image has {bits} bits per channel; MitCad reads 8 to {MAX_BITS}"
            ));
        }
    }

    // The size the image's own `ispe` states, then its transformations.
    let mut size = None;
    for (kind, property) in properties(iprp, primary)? {
        match &kind {
            b"ispe" => {
                let (_, _, ispe) = full(property)?;
                size = Some([
                    u32_be(ispe, 0).ok_or_else(cut)?,
                    u32_be(ispe, 4).ok_or_else(cut)?,
                ]);
            }
            b"irot" => {
                let turns = property.first().ok_or_else(cut)? & 3;
                if let Some(turned) = size.as_mut().filter(|_| turns % 2 == 1) {
                    turned.swap(0, 1);
                }
            }
            b"clap" if size.is_some() => {
                size = Some([aperture(property, 0)?, aperture(property, 8)?]);
            }
            _ => {}
        }
    }
    let [width, height] = size.ok_or_else(|| "the HEIF image states no size (ispe)".to_owned())?;
    Ok(ImageHeader {
        format: "heif",
        width,
        height,
        pixel_size: None,
    })
}

#[cfg(test)]
mod tests {
    use super::super::read;

    fn boxed(kind: &[u8], body: &[u8]) -> Vec<u8> {
        let mut bytes = u32::try_from(8 + body.len())
            .unwrap()
            .to_be_bytes()
            .to_vec();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(body);
        bytes
    }

    fn full(kind: &[u8], version: u8, body: &[u8]) -> Vec<u8> {
        let mut content = vec![version, 0, 0, 0];
        content.extend_from_slice(body);
        boxed(kind, &content)
    }

    /// An `hvcC` property's first 19 bytes with the bits per channel.
    fn hvcc(bits: u8) -> Vec<u8> {
        let mut body = vec![
            1, 1, 0x60, 0, 0, 0, 0x90, 0, 0, 0, 0, 0, 90, 0xf0, 0, 0xfc, 0xfd,
        ];
        body.extend_from_slice(&[0xf8 | (bits - 8), 0xf8 | (bits - 8)]);
        boxed(b"hvcC", &body)
    }

    fn ispe(width: u32, height: u32) -> Vec<u8> {
        let mut body = width.to_be_bytes().to_vec();
        body.extend_from_slice(&height.to_be_bytes());
        full(b"ispe", 0, &body)
    }

    /// A HEIF file without image data: `ftyp` with `brand`, and `meta` with
    /// the items (id, type), the properties and each item's associations
    /// (property indices from 1), the references (type, from, to) and the
    /// primary item 1.
    fn heif_file(
        brand: &[u8; 4],
        items: &[(u16, &[u8; 4])],
        properties: &[Vec<u8>],
        associations: &[(u16, &[u8])],
        references: &[(&[u8; 4], u16, &[u16])],
    ) -> Vec<u8> {
        let mut ftyp = brand.to_vec();
        ftyp.extend_from_slice(&[0, 0, 0, 0]);
        ftyp.extend_from_slice(b"mif1");
        ftyp.extend_from_slice(brand);
        let mut bytes = boxed(b"ftyp", &ftyp);
        let mut meta = Vec::new();
        let mut hdlr = vec![0; 4];
        hdlr.extend_from_slice(b"pict");
        hdlr.extend_from_slice(&[0; 13]);
        meta.extend(full(b"hdlr", 0, &hdlr));
        meta.extend(full(b"pitm", 0, &1u16.to_be_bytes()));
        let mut iinf = u16::try_from(items.len()).unwrap().to_be_bytes().to_vec();
        for (id, kind) in items {
            let mut infe = id.to_be_bytes().to_vec();
            infe.extend_from_slice(&[0, 0]);
            infe.extend_from_slice(*kind);
            infe.push(0);
            iinf.extend(full(b"infe", 2, &infe));
        }
        meta.extend(full(b"iinf", 0, &iinf));
        if !references.is_empty() {
            let mut iref = Vec::new();
            for (kind, from, to) in references {
                let mut body = from.to_be_bytes().to_vec();
                body.extend_from_slice(&u16::try_from(to.len()).unwrap().to_be_bytes());
                for id in *to {
                    body.extend_from_slice(&id.to_be_bytes());
                }
                iref.extend(boxed(*kind, &body));
            }
            meta.extend(full(b"iref", 0, &iref));
        }
        let ipco = boxed(b"ipco", &properties.concat());
        let mut ipma = u32::try_from(associations.len())
            .unwrap()
            .to_be_bytes()
            .to_vec();
        for (id, indices) in associations {
            ipma.extend_from_slice(&id.to_be_bytes());
            ipma.push(u8::try_from(indices.len()).unwrap());
            ipma.extend_from_slice(indices);
        }
        let mut iprp = ipco;
        iprp.extend(full(b"ipma", 0, &ipma));
        meta.extend(boxed(b"iprp", &iprp));
        bytes.extend(full(b"meta", 0, &meta));
        bytes.extend(boxed(b"mdat", &[0; 16]));
        bytes
    }

    /// An HEVC image of `width` x `height` pixels with an `irot` of `turns`
    /// quarter turns (none for 0) and `bits` per channel.
    fn hevc_file(width: u32, height: u32, turns: u8, bits: u8) -> Vec<u8> {
        let mut properties = vec![hvcc(bits), ispe(width, height)];
        let mut indices: Vec<u8> = vec![0x81, 2];
        if turns != 0 {
            properties.push(boxed(b"irot", &[turns]));
            indices.push(0x83);
        }
        heif_file(
            b"heic",
            &[(1, b"hvc1")],
            &properties,
            &[(1, indices.as_slice())],
            &[],
        )
    }

    fn error(bytes: &[u8]) -> String {
        read(bytes).unwrap_err()
    }

    #[test]
    fn hevc_images_and_their_turns_are_read() {
        let header = read(&hevc_file(4032, 3024, 0, 8)).unwrap();
        assert_eq!(header.format, "heif");
        assert_eq!((header.width, header.height), (4032, 3024));
        assert_eq!(header.pixel_size, None);
        // A quarter and three quarters turn: shown upright, height across.
        for turns in [1, 3] {
            let header = read(&hevc_file(4032, 3024, turns, 10)).unwrap();
            assert_eq!((header.width, header.height), (3024, 4032), "{turns}");
        }
        let header = read(&hevc_file(4032, 3024, 2, 8)).unwrap();
        assert_eq!((header.width, header.height), (4032, 3024));
    }

    #[test]
    fn grids_crops_and_mirrors_are_read() {
        // A 2 x 1 grid of 512 x 512 tiles shown 1000 x 512, cropped to
        // 999.6 x 400 (rounded 1000 x 400), then turned and mirrored.
        let mut clap = Vec::new();
        for value in [4998u32, 5, 400, 1, 0, 1, 0, 1] {
            clap.extend_from_slice(&value.to_be_bytes());
        }
        let properties = vec![
            hvcc(8),
            ispe(512, 512),
            ispe(1000, 512),
            boxed(b"clap", &clap),
            boxed(b"irot", &[1]),
            boxed(b"imir", &[0]),
        ];
        let file = heif_file(
            b"heic",
            &[(1, b"grid"), (2, b"hvc1"), (3, b"hvc1")],
            &properties,
            &[
                (1, &[3, 0x84, 0x85, 0x86]),
                (2, &[0x81, 2]),
                (3, &[0x81, 2]),
            ],
            &[(b"dimg", 1, &[2, 3])],
        );
        let header = read(&file).unwrap();
        assert_eq!((header.width, header.height), (400, 1000));
        // A tile with 12 bits per channel.
        let properties = vec![hvcc(8), ispe(512, 512), ispe(1024, 512), hvcc(12)];
        let file = heif_file(
            b"heic",
            &[(1, b"grid"), (2, b"hvc1"), (3, b"hvc1")],
            &properties,
            &[(1, &[3]), (2, &[0x81, 2]), (3, &[0x84, 2])],
            &[(b"dimg", 1, &[2, 3])],
        );
        assert!(
            error(&file).contains("12 bits per channel"),
            "{}",
            error(&file)
        );
        // A grid without its tiles.
        let file = heif_file(b"heic", &[(1, b"grid")], &[ispe(10, 10)], &[(1, &[1])], &[]);
        assert!(
            error(&file).contains("tiles are not there"),
            "{}",
            error(&file)
        );
    }

    #[test]
    fn other_heif_files_are_refused_with_the_reason() {
        let avif = heif_file(b"avif", &[(1, b"av01")], &[ispe(8, 8)], &[(1, &[1])], &[]);
        assert!(error(&avif).contains("AVIF"), "{}", error(&avif));
        let jpeg = heif_file(b"heic", &[(1, b"jpeg")], &[ispe(8, 8)], &[(1, &[1])], &[]);
        assert!(error(&jpeg).contains("coded as “jpeg”"), "{}", error(&jpeg));
        let deep = hevc_file(64, 32, 0, 12);
        assert!(
            error(&deep).contains("12 bits per channel"),
            "{}",
            error(&deep)
        );
        // No size, no decoder configuration.
        let sizeless = heif_file(b"heic", &[(1, b"hvc1")], &[hvcc(8)], &[(1, &[1])], &[]);
        assert!(error(&sizeless).contains("no size"), "{}", error(&sizeless));
        let unconfigured = heif_file(b"heic", &[(1, b"hvc1")], &[ispe(8, 8)], &[(1, &[1])], &[]);
        assert!(
            error(&unconfigured).contains("hvcC"),
            "{}",
            error(&unconfigured)
        );
        // A sequence alone: an ftyp and a movie, no meta.
        let mut ftyp = b"msf1".to_vec();
        ftyp.extend_from_slice(&[0, 0, 0, 0]);
        ftyp.extend_from_slice(b"msf1hevc");
        let mut sequence = boxed(b"ftyp", &ftyp);
        sequence.extend(boxed(b"moov", &[]));
        assert!(
            error(&sequence).contains("image sequence"),
            "{}",
            error(&sequence)
        );
        // Cut inside its boxes, or larger than MitCad takes.
        let file = hevc_file(64, 32, 0, 8);
        for length in [30, 60, file.len() - 3] {
            assert!(read(&file[..length]).is_err(), "{length}");
        }
        assert!(error(&hevc_file(40_000, 30, 0, 8)).contains("more than 32768"));
        assert!(error(&hevc_file(0, 30, 0, 8)).contains("no pixels"));
        // An MP4 file is no HEIF file.
        let mut mp4 = boxed(b"ftyp", b"isom\0\0\0\0isomiso2mp41");
        mp4.extend(boxed(b"moov", &[]));
        assert!(error(&mp4).contains("no PNG, JPEG, BMP, WebP or HEIF"));
    }
}

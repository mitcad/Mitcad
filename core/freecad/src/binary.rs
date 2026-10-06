// SPDX-License-Identifier: MIT
//! Lists FreeCAD writes into files of their own in the archive, little
//! endian, each a 32-bit count followed by the items:
//!
//! - `PlacementList`: seven doubles each, the position and the quaternion
//!   (x, y, z, w).
//! - `VectorList`: three doubles each.
//! - `FloatList`: one double each.
//! - `ColorList`: a packed colour each (`r << 24 | g << 16 | b << 8 | a`).
//! - `MaterialList` (1.0's shape appearance): per material four packed
//!   colours (ambient, diffuse, specular, emissive) and two floats
//!   (shininess, transparency); after all materials, per material three
//!   length-prefixed strings (image, image path, library uuid).
//! - `FilletEdges` (the Part workbench's fillets and chamfers): per edge
//!   its number (`Edge<n>` of the base's shape, a 32-bit integer) and two
//!   doubles (the first and second radius or size).

use crate::placement::Placement;
use crate::value::{Color, Material, Value};

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.data.len());
        let end = end.ok_or_else(|| format!("{} bytes, more expected", self.data.len()))?;
        let bytes = &self.data[self.at..end];
        self.at = end;
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Result<f64, String> {
        Ok(f64::from(f32::from_bits(self.u32()?)))
    }

    fn f64(&mut self) -> Result<f64, String> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(f64::from_le_bytes(a))
    }

    /// The count of items of `size` bytes, checked against what is left.
    fn count(&mut self, size: usize) -> Result<usize, String> {
        let n = self.u32()? as usize;
        if n.saturating_mul(size) > self.data.len() - self.at {
            return Err(format!("{n} items do not fit in {} bytes", self.data.len()));
        }
        Ok(n)
    }

    fn string(&mut self) -> Result<String, String> {
        let n = self.u32()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
}

pub fn placement_list(data: &[u8]) -> Result<Vec<Placement>, String> {
    let mut r = Reader::new(data);
    let n = r.count(56)?;
    (0..n)
        .map(|_| {
            let p = [r.f64()?, r.f64()?, r.f64()?];
            let q = [r.f64()?, r.f64()?, r.f64()?, r.f64()?];
            Ok(Placement::from_quaternion(p, q))
        })
        .collect()
}

pub fn vector_list(data: &[u8]) -> Result<Vec<[f64; 3]>, String> {
    let mut r = Reader::new(data);
    let n = r.count(24)?;
    (0..n).map(|_| Ok([r.f64()?, r.f64()?, r.f64()?])).collect()
}

pub fn float_list(data: &[u8]) -> Result<Vec<f64>, String> {
    let mut r = Reader::new(data);
    let n = r.count(8)?;
    (0..n).map(|_| r.f64()).collect()
}

pub fn fillet_edges(data: &[u8]) -> Result<Vec<(i64, f64, f64)>, String> {
    let mut r = Reader::new(data);
    let n = r.count(20)?;
    (0..n)
        .map(|_| Ok((i64::from(r.u32()? as i32), r.f64()?, r.f64()?)))
        .collect()
}

pub fn color_list(data: &[u8]) -> Result<Vec<Color>, String> {
    let mut r = Reader::new(data);
    let n = r.count(4)?;
    (0..n).map(|_| Ok(Color::from_packed(r.u32()?))).collect()
}

pub fn material_list(data: &[u8]) -> Result<Vec<Material>, String> {
    let mut r = Reader::new(data);
    let n = r.count(24)?;
    let mut materials = (0..n)
        .map(|_| {
            Ok(Material {
                ambient: Color::from_packed(r.u32()?),
                diffuse: Color::from_packed(r.u32()?),
                specular: Color::from_packed(r.u32()?),
                emissive: Color::from_packed(r.u32()?),
                shininess: r.f32()?,
                transparency: r.f32()?,
                uuid: None,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    // Newer versions: the strings after the materials (absent in older
    // files; a short file keeps the materials without them).
    for material in &mut materials {
        let strings = (|| Ok::<_, String>([r.string()?, r.string()?, r.string()?]))();
        match strings {
            Ok([_, _, uuid]) => material.uuid = (!uuid.is_empty()).then_some(uuid),
            Err(_) => break,
        }
    }
    Ok(materials)
}

/// The value of a list kept in a file, or None for kinds not decoded.
pub(crate) fn decode(element: &str, data: &[u8]) -> Option<Result<Value, String>> {
    Some(match element {
        "PlacementList" => placement_list(data).map(Value::PlacementList),
        "VectorList" => vector_list(data).map(Value::VectorList),
        "FloatList" => float_list(data).map(Value::FloatList),
        "ColorList" => color_list(data).map(Value::ColorList),
        "MaterialList" => material_list(data).map(Value::MaterialList),
        "FilletEdges" => fillet_edges(data).map(Value::FilletEdges),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_lists() {
        let mut data = 2u32.to_le_bytes().to_vec();
        for v in [
            1.0f64, 2.0, 3.0, 0.0, 0.0, 0.0, 1.0, 4.0, 5.0, 6.0, 0.0, 0.0, 1.0, 0.0,
        ] {
            data.extend(v.to_le_bytes());
        }
        let list = placement_list(&data).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(list[1].rotation, [0.0, 0.0, 1.0, 0.0]);
        assert!(placement_list(&data[..data.len() - 1]).is_err());
        assert!(placement_list(&[0xff, 0xff, 0xff, 0xff]).is_err());

        let colors = color_list(&[1, 0, 0, 0, 0, 0, 0, 0xff]).unwrap();
        assert_eq!(colors[0].rgb8(), [255, 0, 0]);

        let mut data = 2u32.to_le_bytes().to_vec();
        for (edge, r1, r2) in [(10i32, 1.5f64, 1.5f64), (6, 2.0, 1.0)] {
            data.extend(edge.to_le_bytes());
            data.extend(r1.to_le_bytes());
            data.extend(r2.to_le_bytes());
        }
        assert_eq!(
            fillet_edges(&data).unwrap(),
            vec![(10, 1.5, 1.5), (6, 2.0, 1.0)]
        );
        assert!(fillet_edges(&data[..30]).is_err());
    }

    #[test]
    fn decodes_materials_with_and_without_strings() {
        let mut data = 1u32.to_le_bytes().to_vec();
        for packed in [0x5555_5500u32, 0x00cc_0000, 0x8888_8800, 0] {
            data.extend(packed.to_le_bytes());
        }
        data.extend(0.9f32.to_le_bytes());
        data.extend(0.25f32.to_le_bytes());
        let bare = material_list(&data).unwrap();
        assert_eq!(bare[0].diffuse.rgb8(), [0, 204, 0]);
        assert_eq!(bare[0].transparency, 0.25);
        data.extend(0u32.to_le_bytes());
        data.extend(0u32.to_le_bytes());
        data.extend(4u32.to_le_bytes());
        data.extend(b"abcd");
        assert_eq!(
            material_list(&data).unwrap()[0].uuid.as_deref(),
            Some("abcd")
        );
    }
}

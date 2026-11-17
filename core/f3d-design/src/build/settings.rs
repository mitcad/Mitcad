// SPDX-License-Identifier: MIT
//! The design's settings the import keeps (mitcad#1937): its length unit
//! and its named views, whose standard views tell the design's modelling
//! orientation.
//!
//! - **Length unit.** The design segment keeps six unit systems
//!   (`UnitSystems`: `CmMKS`, `MmMKS`, `MMKS`, `InchImperial`, `Imperial`
//!   and `Custom`); the `AssetSettings` record refers to the one the
//!   design uses. Its `ModelingLength` entry (property `modelingLengthName`)
//!   names the length unit: `millimeter`, `centimeter`, `meter`, `inch` or
//!   `foot`. Model coordinates are centimetres under every unit.
//! - **Named views.** Each named view record holds its name (`TOP`,
//!   `FRONT`, `RIGHT`, `HOME` for the standard views) and refers to its
//!   camera record, which ends with the eye, a `u32`, the target and the up
//!   vector (three `f64` each, model centimetres). A design modelled with
//!   Z up has a `FRONT` view looking along +Y with +Z up; one modelled with
//!   Y up looks along -Z with +Y up.

use crate::ir::NamedView;
use crate::stream::{Segment, f64s_at, str16s};

/// The record that refers to the design's unit system.
const ASSET_SETTINGS: &str = "23FD63FB-4EBE-4C85-A42B-5D9809AA3499";
/// A unit system (one of six).
const UNIT_SYSTEM: &str = "21086CE0-C398-4B1B-9CD6-86F092429833";
/// A unit entry of a unit system (its quantity and the unit's name).
const UNIT_ENTRY: &str = "B61F1D39-4984-47B0-9248-3A1113E86FC1";
/// A named view: its name and its camera.
const NAMED_VIEW: &str = "1787C882-91E2-4363-9C18-80C98EBE4C77";
/// A named view's camera.
const CAMERA: &str = "736DEAAF-2425-405E-B871-003AAB726295";

/// The design's length unit as the dump writes units (`mm`, `cm`, `m`,
/// `in`, `ft`); `None` where the file does not say.
pub(crate) fn length_unit(seg: &Segment) -> Option<String> {
    let settings = seg.objects_of(ASSET_SETTINGS).next()?;
    let system = seg
        .ref_ids(seg.data(settings))
        .into_iter()
        .find(|&r| seg.guid_of(r) == Some(UNIT_SYSTEM))?;
    seg.ref_ids(seg.data_of(system))
        .into_iter()
        .filter(|&r| seg.guid_of(r) == Some(UNIT_ENTRY))
        .find_map(|r| modeling_length(seg.data_of(r)))
        .map(str::to_string)
}

/// The length unit a `ModelingLength` unit entry names.
fn modeling_length(d: &[u8]) -> Option<&'static str> {
    const PROPERTY: &[u8] = b"modelingLengthName";
    if !d.windows(PROPERTY.len()).any(|w| w == PROPERTY) {
        return None;
    }
    str16s(d).iter().find_map(|s| unit_symbol(s))
}

/// The dump's unit for a unit system's length name.
pub(crate) fn unit_symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "millimeter" => "mm",
        "centimeter" => "cm",
        "meter" => "m",
        "inch" => "in",
        "foot" => "ft",
        _ => return None,
    })
}

/// The design's named views with their cameras, in stream order; a view
/// whose camera cannot be read is left out.
pub(crate) fn named_views(seg: &Segment) -> Vec<NamedView> {
    seg.objects_of(NAMED_VIEW)
        .filter_map(|o| {
            let d = seg.data(o);
            let name = str16s(d).into_iter().next()?;
            let camera = seg
                .ref_ids(d)
                .into_iter()
                .find(|&r| seg.guid_of(r) == Some(CAMERA))?;
            let (eye, target, up) = camera_of(seg.data_of(camera))?;
            Some(NamedView {
                name: Some(name),
                eye: Some(eye),
                target: Some(target),
                up: Some(up),
                ..NamedView::default()
            })
        })
        .collect()
}

/// A camera record's eye, target and up vector, read from its end: `eye |
/// u32 | target | up`. `None` unless they are finite, the eye is not the
/// target and the up vector is a unit vector.
pub(crate) fn camera_of(d: &[u8]) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
    let n = d.len();
    if n < 76 {
        return None;
    }
    let vector = |p: usize| -> Option<[f64; 3]> {
        let v = f64s_at(d, p, 3)?;
        let v = [v[0], v[1], v[2]];
        v.iter().all(|x| x.is_finite()).then_some(v)
    };
    let (eye, target, up) = (vector(n - 76)?, vector(n - 48)?, vector(n - 24)?);
    let length = |v: [f64; 3]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
    let sight = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
    ((length(up) - 1.0).abs() < 1e-6 && length(sight) > 1e-9).then_some((eye, target, up))
}

#[cfg(test)]
mod tests {
    //! A synthetic design segment with the records as the design files
    //! write them (the layouts above).

    use super::*;
    use crate::writer::DesignWriter;

    const ROOT: &str = "98542EB9-A4F2-4137-A808-DBB5B3CD6159";
    /// The unit systems' collection (any class will do).
    const COLLECTION: &str = "A1675506-5D2D-47A5-8AD2-8FD5CE946DA0";

    fn s8(s: &str) -> Vec<u8> {
        let mut v = (s.len() as u32).to_le_bytes().to_vec();
        v.extend(s.as_bytes());
        v
    }

    fn s16(s: &str) -> Vec<u8> {
        let units: Vec<u16> = s.encode_utf16().collect();
        let mut v = (units.len() as u32).to_le_bytes().to_vec();
        v.extend(units.iter().flat_map(|u| u.to_le_bytes()));
        v
    }

    /// A reference: `01 | u64 id | 00 00`.
    fn r(id: u64) -> Vec<u8> {
        let mut v = vec![1];
        v.extend(id.to_le_bytes());
        v.extend([0, 0]);
        v
    }

    fn f64s(v: &[f64]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// A unit entry: key, label, `01`, property, namespace, four zero
    /// bytes and the unit's name, with the back reference to its system.
    fn unit_entry(key: &str, property: &str, unit: &str, system: u64) -> Vec<u8> {
        [
            vec![0; 10],
            s8(key),
            s8(key),
            vec![1],
            s8(property),
            s8("NsCommonData"),
            vec![0; 4],
            s16(unit),
            r(system),
        ]
        .concat()
    }

    /// A unit system: key, label, `01`, its name property, namespace, four
    /// zero bytes and the counted references to its entries.
    fn unit_system(key: &str, entries: &[u64], collection: u64) -> Vec<u8> {
        let mut v = [
            vec![0; 10],
            s8(key),
            s8(key),
            vec![1],
            s8(&format!("{key}UnitSystemName")),
            s8("NaFusion"),
            vec![0; 4],
            (entries.len() as u32).to_le_bytes().to_vec(),
        ]
        .concat();
        for e in entries {
            v.extend(r(*e));
        }
        v.extend(r(collection));
        v
    }

    /// A camera: field of view, eye, `u32 1`, target, up (as the file's
    /// records end).
    fn camera(eye: [f64; 3], target: [f64; 3], up: [f64; 3]) -> Vec<u8> {
        [
            vec![0; 40],
            f64s(&[std::f64::consts::FRAC_PI_8]),
            f64s(&eye),
            1u32.to_le_bytes().to_vec(),
            f64s(&target),
            f64s(&up),
        ]
        .concat()
    }

    /// A design whose unit system `active` (0: millimetres, 1: inches, 2:
    /// custom in feet) is the one its settings refer to, with the four
    /// standard views of a design modelled with Y up.
    fn design(active: usize) -> DesignWriter {
        let mut d = DesignWriter::default();
        for g in [
            ROOT,
            ASSET_SETTINGS,
            COLLECTION,
            UNIT_SYSTEM,
            UNIT_ENTRY,
            NAMED_VIEW,
            CAMERA,
        ] {
            d.class(g, if g == ROOT { "" } else { ROOT }, 0);
        }
        let systems = [
            (10, "MmMKS", "millimeter"),
            (20, "InchImperial", "inch"),
            (30, "Custom", "foot"),
        ];
        d.obj(
            1,
            ASSET_SETTINGS,
            [
                vec![0; 10],
                s8("AssetSettings"),
                vec![0; 6],
                r(systems[active].0),
                vec![0; 12],
            ]
            .concat(),
        );
        let mut list = [
            vec![0; 10],
            s8("UnitSystems"),
            vec![0; 2],
            3u32.to_le_bytes().to_vec(),
        ]
        .concat();
        for (id, _, _) in systems {
            list.extend(r(id));
        }
        d.obj(2, COLLECTION, list);
        for (id, key, unit) in systems {
            d.obj(id, UNIT_SYSTEM, unit_system(key, &[id + 1, id + 2], 2));
            d.obj(
                id + 1,
                UNIT_ENTRY,
                unit_entry("Length", "lengthName", "meter", id),
            );
            d.obj(
                id + 2,
                UNIT_ENTRY,
                unit_entry("ModelingLength", "modelingLengthName", unit, id),
            );
        }
        let target = [10.0, 5.0, -8.0];
        let views = [
            ("TOP", [10.0, 105.0, -8.0], [0.0, 0.0, -1.0]),
            ("FRONT", [10.0, 5.0, 92.0], [0.0, 1.0, 0.0]),
            ("RIGHT", [110.0, 5.0, -8.0], [0.0, 1.0, 0.0]),
            ("HOME", [70.0, 65.0, 52.0], [0.0, 1.0, 0.0]),
        ];
        for (i, (name, eye, up)) in views.into_iter().enumerate() {
            let (view, cam) = (100 + 2 * i as u64, 101 + 2 * i as u64);
            d.obj(cam, CAMERA, camera(eye, target, up));
            d.obj(view, NAMED_VIEW, [vec![0; 11], s16(name), r(cam)].concat());
        }
        d
    }

    #[test]
    fn the_length_unit_is_the_unit_system_the_settings_refer_to() {
        let units: Vec<_> = (0..3).map(|a| length_unit(&design(a).segment())).collect();
        assert_eq!(
            units,
            [Some("mm".to_string()), Some("in".into()), Some("ft".into())]
        );
        // A design without the settings does not say.
        let mut d = design(1);
        d.objects.retain(|o| o.0 != 1);
        assert_eq!(length_unit(&d.segment()), None);
    }

    #[test]
    fn named_views_keep_their_cameras() {
        let views = named_views(&design(0).segment());
        let names: Vec<_> = views.iter().map(|v| v.name.as_deref().unwrap()).collect();
        assert_eq!(names, ["TOP", "FRONT", "RIGHT", "HOME"]);
        let front = &views[1];
        assert_eq!(front.eye, Some([10.0, 5.0, 92.0]));
        assert_eq!(front.target, Some([10.0, 5.0, -8.0]));
        assert_eq!(front.up, Some([0.0, 1.0, 0.0]));
    }

    #[test]
    fn a_camera_without_a_unit_up_vector_or_a_sight_is_not_read() {
        let p = [1.0, 2.0, 3.0];
        assert!(camera_of(&camera(p, [0.0; 3], [0.0, 0.0, 1.0])).is_some());
        assert!(camera_of(&camera(p, [0.0; 3], [0.0, 0.0, 2.0])).is_none());
        assert!(camera_of(&camera(p, p, [0.0, 0.0, 1.0])).is_none());
        assert!(camera_of(&[0; 40]).is_none());
    }

    #[test]
    fn unit_names_become_the_dumps_units() {
        assert_eq!(unit_symbol("inch"), Some("in"));
        assert_eq!(unit_symbol("parsec"), None);
    }
}

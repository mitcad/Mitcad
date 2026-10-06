// SPDX-License-Identifier: MIT
//! Lenient JSON records for the dump IR.
//!
//! External dumps may lack any key, report a failed property as
//! `{"_error": ...}` in place of its value, and carry keys this crate does
//! not model. A record therefore keeps every modelled key as an `Option`,
//! and anything else, including a modelled key whose value has an
//! unexpected shape, unchanged in `other`. Reading never fails for a JSON
//! object, and writing a record back gives the same keys and values.

/// Declares a record: a struct whose fields are `Option<T>` named by JSON
/// keys, plus `other` for the remaining keys, with lenient deserialisation.
///
/// ```ignore
/// record! {
///     /// Doc.
///     pub struct Example {
///         /// Field doc.
///         "jsonKey" => field: Type,
///     }
/// }
/// ```
///
/// A field of type `Option<T>` distinguishes an absent key (`None`) from a
/// JSON `null` (`Some(None)`). The `@custom_de` form leaves out the
/// `Deserialize` impl (the type then uses `from_map` in its own impl).
macro_rules! record {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $(
                $(#[$fmeta:meta])*
                $key:literal => $field:ident : $ty:ty,
            )*
        }
    ) => {
        $crate::design::record::record!(
            @custom_de $(#[$meta])* pub struct $name { $( $(#[$fmeta])* $key => $field : $ty, )* }
        );

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                <serde_json::Map<String, serde_json::Value> as serde::Deserialize>::deserialize(d)
                    .map(Self::from_map)
            }
        }
    };
    (
        @custom_de
        $(#[$meta:meta])*
        pub struct $name:ident {
            $(
                $(#[$fmeta:meta])*
                $key:literal => $field:ident : $ty:ty,
            )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
        pub struct $name {
            $(
                $(#[$fmeta])*
                #[serde(rename = $key, skip_serializing_if = "Option::is_none")]
                pub $field: Option<$ty>,
            )*
            /// Keys that are not modelled, or whose value did not have the
            /// expected shape (kept unchanged).
            #[serde(flatten)]
            pub other: serde_json::Map<String, serde_json::Value>,
        }

        impl $name {
            /// Takes the modelled keys out of a JSON object.
            #[allow(unused_mut)]
            pub fn from_map(mut map: serde_json::Map<String, serde_json::Value>) -> Self {
                let mut out = Self::default();
                $(
                    if let Some(v) = map.remove($key) {
                        match <$ty as serde::Deserialize>::deserialize(&v) {
                            Ok(x) => out.$field = Some(x),
                            Err(_) => {
                                map.insert($key.to_string(), v);
                            }
                        }
                    }
                )*
                out.other = map;
                out
            }
        }
    };
}

pub(crate) use record;

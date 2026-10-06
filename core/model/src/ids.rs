// SPDX-License-Identifier: MIT
//! Stable identities of features, bodies and sketch entities. They are
//! assigned when an item is created, saved in the project file and never
//! derived from positions, so inserting, deleting or reordering features
//! does not change what a reference points at.

use std::fmt;
use std::str::FromStr;

/// A feature, written `F<n>`. Its position in the timeline is separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FeatureUid(pub u64);

/// A body: the feature that created it and its number among the bodies that
/// feature created, written `F7.b0`. Its display name (`Body1`) is separate
/// and editable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BodyUid {
    pub feature: FeatureUid,
    pub index: u32,
}

impl BodyUid {
    pub fn new(feature: FeatureUid, index: u32) -> Self {
        Self { feature, index }
    }
}

/// A sketch entity, unique within its sketch. Curves are written `c<n>` and
/// points `p<n>`; curves and points share the numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityUid(pub u32);

impl EntityUid {
    /// `c<n>`
    pub fn curve_name(self) -> String {
        format!("c{}", self.0)
    }

    pub fn parse_curve(text: &str) -> Result<Self, IdError> {
        parse_number(text, "c")
            .map(EntityUid)
            .ok_or_else(|| IdError::new("curve", text, "c<number>"))
    }
}

/// A component, written `C<n>`; the root component is `C0`. Its display
/// name (`Component1`) is separate and editable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ComponentUid(pub u32);

impl ComponentUid {
    /// The root component: the design itself, which every other component
    /// is placed in, directly or through subassemblies.
    pub const ROOT: Self = Self(0);

    pub fn is_root(self) -> bool {
        self == Self::ROOT
    }
}

impl Default for ComponentUid {
    fn default() -> Self {
        Self::ROOT
    }
}

/// An occurrence: one placement of a component in a parent component,
/// written `O<n>`. Its display name (`Component1:2`) follows the
/// component's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OccurrenceUid(pub u32);

/// A malformed identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdError {
    what: &'static str,
    text: String,
    expected: &'static str,
}

impl IdError {
    pub(crate) fn new(what: &'static str, text: &str, expected: &'static str) -> Self {
        Self {
            what,
            text: text.to_owned(),
            expected,
        }
    }
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid {} id '{}' (expected {})",
            self.what, self.text, self.expected
        )
    }
}

impl std::error::Error for IdError {}

/// The number after `prefix`, in canonical form (no sign, no leading zero).
pub(crate) fn parse_number<T: FromStr>(text: &str, prefix: &str) -> Option<T> {
    let digits = text.strip_prefix(prefix)?;
    let canonical = !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'));
    if canonical { digits.parse().ok() } else { None }
}

impl fmt::Display for FeatureUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "F{}", self.0)
    }
}

impl FromStr for FeatureUid {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        parse_number(text, "F")
            .map(FeatureUid)
            .ok_or_else(|| IdError::new("feature", text, "F<number>"))
    }
}

impl fmt::Display for BodyUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.b{}", self.feature, self.index)
    }
}

impl FromStr for BodyUid {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        let error = || IdError::new("body", text, "F<number>.b<number>");
        let (feature, index) = text.split_once('.').ok_or_else(error)?;
        Ok(Self {
            feature: feature.parse().map_err(|_| error())?,
            index: parse_number(index, "b").ok_or_else(error)?,
        })
    }
}

impl fmt::Display for ComponentUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C{}", self.0)
    }
}

impl FromStr for ComponentUid {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        parse_number(text, "C")
            .map(ComponentUid)
            .ok_or_else(|| IdError::new("component", text, "C<number>"))
    }
}

impl fmt::Display for OccurrenceUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "O{}", self.0)
    }
}

impl FromStr for OccurrenceUid {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        parse_number(text, "O")
            .map(OccurrenceUid)
            .ok_or_else(|| IdError::new("occurrence", text, "O<number>"))
    }
}

/// Serializes an id as its string form.
macro_rules! string_serde {
    ($type:ty, $expecting:literal) => {
        impl serde::Serialize for $type {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> serde::Deserialize<'de> for $type {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Visitor;
                impl serde::de::Visitor<'_> for Visitor {
                    type Value = $type;
                    fn expecting(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                        f.write_str($expecting)
                    }
                    fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<$type, E> {
                        text.parse().map_err(E::custom)
                    }
                }
                deserializer.deserialize_str(Visitor)
            }
        }
    };
}

pub(crate) use string_serde;

string_serde!(FeatureUid, "a feature id like \"F3\"");
string_serde!(BodyUid, "a body id like \"F3.b0\"");
string_serde!(ComponentUid, "a component id like \"C2\"");
string_serde!(OccurrenceUid, "an occurrence id like \"O3\"");

/// Curve ids in files and commands: `"c3"`.
pub(crate) mod curve_serde {
    use super::EntityUid;

    pub fn serialize<S: serde::Serializer>(
        id: &EntityUid,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&id.curve_name())
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<EntityUid, D::Error> {
        let text: String = serde::Deserialize::deserialize(deserializer)?;
        EntityUid::parse_curve(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_through_their_string_form() {
        assert_eq!(FeatureUid(7).to_string(), "F7");
        assert_eq!("F7".parse(), Ok(FeatureUid(7)));
        let body = BodyUid::new(FeatureUid(12), 3);
        assert_eq!(body.to_string(), "F12.b3");
        assert_eq!("F12.b3".parse(), Ok(body));
        assert_eq!(EntityUid(4).curve_name(), "c4");
        assert_eq!(EntityUid::parse_curve("c4"), Ok(EntityUid(4)));
        assert_eq!(serde_json::to_string(&body).unwrap(), "\"F12.b3\"");
        assert_eq!(
            serde_json::from_str::<FeatureUid>("\"F0\"").unwrap(),
            FeatureUid(0)
        );
        assert_eq!(ComponentUid::ROOT.to_string(), "C0");
        assert_eq!("C12".parse(), Ok(ComponentUid(12)));
        assert_eq!("O3".parse(), Ok(OccurrenceUid(3)));
        assert_eq!(serde_json::to_string(&OccurrenceUid(3)).unwrap(), "\"O3\"");
        assert!("C01".parse::<ComponentUid>().is_err());
        assert!("F3".parse::<OccurrenceUid>().is_err());
    }

    #[test]
    fn only_canonical_forms_parse() {
        for text in [
            "",
            "F",
            "f3",
            "F03",
            "F-3",
            "F+3",
            "F3 ",
            "3",
            "F99999999999999999999",
        ] {
            assert!(text.parse::<FeatureUid>().is_err(), "{text}");
        }
        for text in [
            "F3", "F3.", "F3.0", "F3.b", "F3.b01", "F3.c0", "x.b0", "F3.b0.b1",
        ] {
            assert!(text.parse::<BodyUid>().is_err(), "{text}");
        }
        assert!(EntityUid::parse_curve("p3").is_err());
        assert_eq!(
            "F3.b".parse::<BodyUid>().unwrap_err().to_string(),
            "invalid body id 'F3.b' (expected F<number>.b<number>)"
        );
        let error = serde_json::from_str::<FeatureUid>("\"Sketch1\"").unwrap_err();
        assert!(
            error.to_string().contains("invalid feature id 'Sketch1'"),
            "{error}"
        );
    }
}

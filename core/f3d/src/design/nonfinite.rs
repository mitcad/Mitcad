// SPDX-License-Identifier: MIT
//! Paths of non-finite numbers in a serialisable value.
//!
//! JSON has no NaN or infinity; serde_json writes them as `null`. The
//! dump records where that happened in `_f3d.non_finite`, with the
//! reference decoder's path syntax: `.key` for object members, `[]` for
//! array items (e.g. `.timeline.items[].detail.curves[].length`).

use std::collections::BTreeSet;
use std::fmt::Display;

use serde::ser::{self, Serialize};

/// Sorted, unique paths of the non-finite floats in `value`.
pub fn paths<T: Serialize + ?Sized>(value: &T) -> Vec<String> {
    let mut out = BTreeSet::new();
    let _ = value.serialize(Finder {
        path: String::new(),
        out: &mut out,
    });
    out.into_iter().collect()
}

#[derive(Debug)]
struct Error;

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unsupported")
    }
}

impl std::error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: Display>(_msg: T) -> Self {
        Error
    }
}

struct Finder<'a> {
    path: String,
    out: &'a mut BTreeSet<String>,
}

impl<'a> Finder<'a> {
    fn child(&mut self, suffix: &str) -> Finder<'_> {
        Finder {
            path: format!("{}{}", self.path, suffix),
            out: &mut *self.out,
        }
    }

    fn float(self, v: f64) -> Result<(), Error> {
        if !v.is_finite() {
            self.out.insert(self.path);
        }
        Ok(())
    }
}

/// Collects a map key as text.
struct KeyText;

macro_rules! key_scalar {
    ($($f:ident: $t:ty),*) => {
        $(fn $f(self, v: $t) -> Result<String, Error> { Ok(v.to_string()) })*
    };
}

impl ser::Serializer for KeyText {
    type Ok = String;
    type Error = Error;
    type SerializeSeq = ser::Impossible<String, Error>;
    type SerializeTuple = ser::Impossible<String, Error>;
    type SerializeTupleStruct = ser::Impossible<String, Error>;
    type SerializeTupleVariant = ser::Impossible<String, Error>;
    type SerializeMap = ser::Impossible<String, Error>;
    type SerializeStruct = ser::Impossible<String, Error>;
    type SerializeStructVariant = ser::Impossible<String, Error>;

    key_scalar!(serialize_bool: bool, serialize_i8: i8, serialize_i16: i16, serialize_i32: i32,
        serialize_i64: i64, serialize_u8: u8, serialize_u16: u16, serialize_u32: u32,
        serialize_u64: u64, serialize_f32: f32, serialize_f64: f64, serialize_char: char,
        serialize_str: &str);

    fn serialize_bytes(self, _v: &[u8]) -> Result<String, Error> {
        Err(Error)
    }
    fn serialize_none(self) -> Result<String, Error> {
        Err(Error)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<String, Error> {
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<String, Error> {
        Err(Error)
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<String, Error> {
        Err(Error)
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<String, Error> {
        Ok(variant.to_string())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        v: &T,
    ) -> Result<String, Error> {
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _v: &T,
    ) -> Result<String, Error> {
        Err(Error)
    }
    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Error> {
        Err(Error)
    }
    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Error> {
        Err(Error)
    }
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, Error> {
        Err(Error)
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Error> {
        Err(Error)
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Error> {
        Err(Error)
    }
    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, Error> {
        Err(Error)
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Error> {
        Err(Error)
    }
}

impl<'a> ser::Serializer for Finder<'a> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Compound<'a>;
    type SerializeTuple = Compound<'a>;
    type SerializeTupleStruct = Compound<'a>;
    type SerializeTupleVariant = Compound<'a>;
    type SerializeMap = Compound<'a>;
    type SerializeStruct = Compound<'a>;
    type SerializeStructVariant = Compound<'a>;

    fn serialize_bool(self, _v: bool) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_i8(self, _v: i8) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_i16(self, _v: i16) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_i32(self, _v: i32) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_i64(self, _v: i64) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_u8(self, _v: u8) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_u16(self, _v: u16) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_u32(self, _v: u32) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_u64(self, _v: u64) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_f32(self, v: f32) -> Result<(), Error> {
        self.float(v as f64)
    }
    fn serialize_f64(self, v: f64) -> Result<(), Error> {
        self.float(v)
    }
    fn serialize_char(self, _v: char) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_str(self, _v: &str) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_bytes(self, _v: &[u8]) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_none(self) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<(), Error> {
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
    ) -> Result<(), Error> {
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        mut self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        v.serialize(self.child(&format!(".{variant}")))
    }
    fn serialize_seq(self, _len: Option<usize>) -> Result<Compound<'a>, Error> {
        Ok(Compound::new(self))
    }
    fn serialize_tuple(self, _len: usize) -> Result<Compound<'a>, Error> {
        Ok(Compound::new(self))
    }
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Compound<'a>, Error> {
        Ok(Compound::new(self))
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Compound<'a>, Error> {
        let mut c = Compound::new(self);
        c.finder.path.push('.');
        c.finder.path.push_str(variant);
        Ok(c)
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Compound<'a>, Error> {
        Ok(Compound::new(self))
    }
    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Compound<'a>, Error> {
        Ok(Compound::new(self))
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Compound<'a>, Error> {
        let mut c = Compound::new(self);
        c.finder.path.push('.');
        c.finder.path.push_str(variant);
        Ok(c)
    }
}

/// Arrays (`[]`) and objects (`.key`).
struct Compound<'a> {
    finder: Finder<'a>,
    key: String,
}

impl<'a> Compound<'a> {
    fn new(finder: Finder<'a>) -> Self {
        Compound {
            finder,
            key: String::new(),
        }
    }

    fn item<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        v.serialize(self.finder.child("[]"))
    }

    fn member<T: Serialize + ?Sized>(&mut self, key: &str, v: &T) -> Result<(), Error> {
        v.serialize(self.finder.child(&format!(".{key}")))
    }
}

impl ser::SerializeSeq for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.item(v)
    }
    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}

impl ser::SerializeTuple for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.item(v)
    }
    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}

impl ser::SerializeTupleStruct for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.item(v)
    }
    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}

impl ser::SerializeTupleVariant for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.item(v)
    }
    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}

impl ser::SerializeMap for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Error> {
        self.key = key.serialize(KeyText)?;
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        let key = std::mem::take(&mut self.key);
        self.member(&key, v)
    }
    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}

impl ser::SerializeStruct for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.member(key, v)
    }
    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}

impl ser::SerializeStructVariant for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.member(key, v)
    }
    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(serde::Serialize)]
    struct Inner {
        length: f64,
        ok: f64,
    }

    #[derive(serde::Serialize)]
    struct Outer {
        #[serde(rename = "items")]
        list: Vec<Inner>,
        #[serde(flatten)]
        extra: BTreeMap<String, Vec<f64>>,
        t: (String, f64),
    }

    #[test]
    fn finds_nan_and_infinity() {
        let mut extra = BTreeMap::new();
        extra.insert("xyz".to_string(), vec![1.0, f64::INFINITY]);
        let v = Outer {
            list: vec![
                Inner {
                    length: 1.0,
                    ok: 2.0,
                },
                Inner {
                    length: f64::NAN,
                    ok: 2.0,
                },
            ],
            extra,
            t: ("a".into(), f64::NEG_INFINITY),
        };
        assert_eq!(paths(&v), vec![".items[].length", ".t[]", ".xyz[]"]);
        assert!(paths(&1.0f64).is_empty());
    }
}

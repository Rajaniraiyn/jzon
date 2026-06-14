//! Drop-in `serde_json` replacement routing hot-path functions through `jzon_serde`.

use std::io;

pub use serde_json::{Error, Map, Number, Result, Value};
pub use serde_json::{from_value, to_value};
pub use serde_json::json;
pub use serde_json::{Deserializer, Serializer, StreamDeserializer};

pub mod de    { pub use serde_json::de::*; }
pub mod ser   { pub use serde_json::ser::*; }
pub mod error { pub use serde_json::error::*; }
pub mod map   { pub use serde_json::map::*; }
pub mod value { pub use serde_json::value::*; }

#[inline]
pub fn from_str<'de, T: serde::Deserialize<'de>>(s: &'de str) -> Result<T> {
    jzon_serde::from_str(s).map_err(|e| serde::de::Error::custom(e.to_string()))
}

#[inline]
pub fn from_slice<'de, T: serde::Deserialize<'de>>(v: &'de [u8]) -> Result<T> {
    jzon_serde::from_slice(v).map_err(|e| serde::de::Error::custom(e.to_string()))
}

#[inline]
pub fn from_reader<R: io::Read, T: serde::de::DeserializeOwned>(mut r: R) -> Result<T> {
    let mut buf = Vec::new();
    r.read_to_end(&mut buf).map_err(Error::io)?;
    from_slice(&buf)
}

#[inline]
pub fn to_string<T: serde::Serialize>(v: &T) -> Result<String> {
    jzon_serde::to_string(v).map_err(|e| serde::ser::Error::custom(e.to_string()))
}

#[inline]
pub fn to_string_pretty<T: serde::Serialize>(v: &T) -> Result<String> {
    serde_json::to_string_pretty(v)
}

#[inline]
pub fn to_vec<T: serde::Serialize>(v: &T) -> Result<Vec<u8>> {
    jzon_serde::to_bytes(v).map_err(|e| serde::ser::Error::custom(e.to_string()))
}

#[inline]
pub fn to_vec_pretty<T: serde::Serialize>(v: &T) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(v)
}

#[inline]
pub fn to_writer<W: io::Write, T: serde::Serialize>(w: W, v: &T) -> Result<()> {
    jzon_serde::to_writer(w, v).map_err(|e| serde::ser::Error::custom(e.to_string()))
}

#[inline]
pub fn to_writer_pretty<W: io::Write, T: serde::Serialize>(w: W, v: &T) -> Result<()> {
    serde_json::to_writer_pretty(w, v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct User {
        id: u64,
        name: String,
        score: f64,
    }

    #[test]
    fn roundtrip_via_compat() {
        let u = User { id: 1, name: "Alice".into(), score: 9.5 };
        let json = to_string(&u).unwrap();
        let u2: User = from_str(&json).unwrap();
        assert_eq!(u, u2);
    }

    #[test]
    fn value_roundtrip() {
        let v: Value = from_str(r#"{"key": 42}"#).unwrap();
        assert_eq!(v["key"], Value::Number(Number::from(42)));
    }

    #[test]
    fn matches_serde_json_output() {
        let v = vec![1u64, 2, 3];
        assert_eq!(to_string(&v).unwrap(), serde_json::to_string(&v).unwrap());
    }

    #[test]
    fn from_slice_works() {
        let data = br#"{"id":7,"name":"Bob","score":3.14}"#;
        let u: User = from_slice(data).unwrap();
        assert_eq!(u.id, 7);
        assert_eq!(u.name, "Bob");
    }

    #[test]
    fn to_vec_works() {
        let u = User { id: 2, name: "Carol".into(), score: 1.0 };
        let bytes = to_vec(&u).unwrap();
        let u2: User = from_slice(&bytes).unwrap();
        assert_eq!(u, u2);
    }

    #[test]
    fn from_reader_works() {
        let data = br#"{"id":3,"name":"Dave","score":0.0}"#;
        let cursor = std::io::Cursor::new(data);
        let u: User = from_reader(cursor).unwrap();
        assert_eq!(u.id, 3);
        assert_eq!(u.name, "Dave");
    }

    #[test]
    fn to_writer_works() {
        let u = User { id: 4, name: "Eve".into(), score: 2.718 };
        let mut buf = Vec::new();
        to_writer(&mut buf, &u).unwrap();
        let u2: User = from_slice(&buf).unwrap();
        assert_eq!(u, u2);
    }

    #[test]
    fn pretty_functions_work() {
        let v = vec![1u32, 2, 3];
        let pretty = to_string_pretty(&v).unwrap();
        assert!(pretty.contains('\n'));
        let pretty_bytes = to_vec_pretty(&v).unwrap();
        assert!(pretty_bytes.contains(&b'\n'));
    }

    #[test]
    fn json_macro_works() {
        let v = json!({"hello": "world", "n": 42});
        assert_eq!(v["hello"], Value::String("world".into()));
        assert_eq!(v["n"], Value::Number(Number::from(42)));
    }

    #[test]
    fn from_value_to_value_roundtrip() {
        let u = User { id: 99, name: "Zara".into(), score: 100.0 };
        let v = to_value(&u).unwrap();
        let u2: User = from_value(v).unwrap();
        assert_eq!(u, u2);
    }
}

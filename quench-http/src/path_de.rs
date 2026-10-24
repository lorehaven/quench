//! Reads captured path parameters into a type, backing
//! [`crate::extract::Path`]: a single value (`Path<String>`, `Path<u64>`)
//! reads the only capture, a tuple reads captures positionally, and a
//! struct / map reads them by name. Values are the percent-decoded path text,
//! parsed into numbers / bools on demand.

use quench_serde::{DeserializeOwned, Map, Value};

pub use quench_serde::Error;

/// Deserializes the captures into `T`. Values are percent-decoded first (a
/// `%2f` in a capture becomes `/`, so a handler validating a name sees what
/// the client meant, not the wire form); invalid UTF-8 is an error.
pub fn from_params<T: DeserializeOwned>(params: &[(String, String)]) -> Result<T, Error> {
    let decoded = params
        .iter()
        .map(|(name, value)| Ok((name.clone(), percent_decode(value)?)))
        .collect::<Result<Vec<_>, Error>>()?;

    // The type decides which shape it reads, so offer each in turn: by name
    // (structs, maps), the lone capture (strings, numbers), then by position
    // (tuples). The first that fits wins.
    let _lenient = quench_serde::private::Lenient::enter();
    let by_name = Value::Object(
        decoded
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect::<Map>(),
    );
    let positional = Value::Array(
        decoded
            .iter()
            .map(|(_, v)| Value::String(v.clone()))
            .collect(),
    );
    let mut shapes = vec![by_name];
    if let [(_, only)] = decoded.as_slice() {
        shapes.push(Value::String(only.clone()));
    }
    shapes.push(positional);

    let mut first_error = None;
    let mut content_error = None;
    for shape in &shapes {
        match T::deserialize(shape) {
            Ok(value) => return Ok(value),
            Err(error) => {
                // "invalid type: expected .." means this shape was the wrong
                // one; anything else is about the data and worth reporting.
                if content_error.is_none() && !error.message().starts_with("invalid type:") {
                    content_error = Some(error.clone());
                }
                first_error.get_or_insert(error);
            }
        }
    }
    Err(content_error
        .or(first_error)
        .unwrap_or_else(|| Error::new("no path parameters")))
}

fn percent_decode(value: &str) -> Result<String, Error> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16).and_then(|d| u8::try_from(d).ok());
        if bytes[i] == b'%'
            && let (Some(h), Some(l)) = (
                bytes.get(i + 1).and_then(|b| hex(*b)),
                bytes.get(i + 2).and_then(|b| hex(*b)),
            )
        {
            out.push(h * 16 + l);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| Error::new("path parameter is not valid UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_serde::Deserialize;

    fn p(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[quench_test::test]
    fn single_string_and_number() {
        assert_eq!(from_params::<String>(&p(&[("a", "x/y")])).unwrap(), "x/y");
        assert_eq!(from_params::<u64>(&p(&[("id", "42")])).unwrap(), 42);
        assert!(from_params::<u64>(&p(&[("id", "nope")])).is_err());
        assert!(
            from_params::<u64>(&p(&[("id", "nope")]))
                .unwrap_err()
                .to_string()
                .contains("invalid integer")
        );
        assert!(from_params::<bool>(&p(&[("flag", "true")])).unwrap());
    }

    #[quench_test::test]
    fn tuple_reads_positionally() {
        let v: (String, i64) = from_params(&p(&[("a", "n"), ("b", "-7")])).unwrap();
        assert_eq!(v, ("n".to_string(), -7));
        assert!(from_params::<(String, String)>(&p(&[("a", "n")])).is_err());
        let single: (String,) = from_params(&p(&[("a", "only")])).unwrap();
        assert_eq!(single.0, "only");
    }

    #[quench_test::test]
    fn captures_are_percent_decoded() {
        assert_eq!(
            from_params::<String>(&p(&[("n", "..%2fetc%20x")])).unwrap(),
            "../etc x"
        );
        assert_eq!(from_params::<String>(&p(&[("n", "100%")])).unwrap(), "100%");
        assert_eq!(from_params::<String>(&p(&[("n", "a+b")])).unwrap(), "a+b");
        assert!(from_params::<String>(&p(&[("n", "%ff")])).is_err());
        assert!(
            from_params::<String>(&p(&[("n", "%ff")]))
                .unwrap_err()
                .to_string()
                .contains("UTF-8")
        );
    }

    #[derive(Debug, Deserialize)]
    struct Blob {
        name: String,
        digest: String,
    }

    #[derive(Debug, Deserialize)]
    struct Numbered {
        id: u32,
    }

    #[quench_test::test]
    fn struct_reads_by_name() {
        let b: Blob = from_params(&p(&[("name", "a/b"), ("digest", "sha256:1")])).unwrap();
        assert_eq!((b.name.as_str(), b.digest.as_str()), ("a/b", "sha256:1"));
        // a struct with one field works with one capture too
        let n: Numbered = from_params(&p(&[("id", "9")])).unwrap();
        assert_eq!(n.id, 9);
    }

    #[quench_test::test]
    fn struct_errors_describe_the_data_not_the_shape() {
        let e = from_params::<Numbered>(&p(&[("id", "x")])).unwrap_err();
        assert!(
            e.to_string().contains("id") && e.to_string().contains("invalid integer"),
            "{e}"
        );
        let e = from_params::<Blob>(&p(&[("name", "a")])).unwrap_err();
        assert!(e.to_string().contains("missing field `digest`"), "{e}");
    }

    #[quench_test::test]
    fn no_captures() {
        assert!(from_params::<String>(&[]).is_err());
        assert_eq!(
            from_params::<Vec<String>>(&[]).unwrap(),
            Vec::<String>::new()
        );
    }
}

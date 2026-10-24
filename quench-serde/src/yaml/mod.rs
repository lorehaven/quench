//! YAML 1.2 on [`Value`](crate::Value) (feature `yaml`).
//!
//! Reads block and flow collections, plain/quoted/block scalars with the core
//! schema's types, anchors and aliases, tags (ignored), comments and multiple
//! documents; writes block style. Not supported: complex `? ` keys, and the
//! YAML 1.1 extras (`yes`/`on` booleans, `<<` merge keys, sexagesimal numbers).
//! Mapping keys that are not strings are read as their text.

mod parse;
mod resolve;
mod write;

pub use crate::{Error, Map, Result, Value};
pub use parse::{parse, parse_all};
pub use write::{to_string, to_string_all};

use crate::{Deserialize, Serialize};

/// Reads a `T` from a single YAML document.
pub fn from_str<T: Deserialize>(text: &str) -> Result<T> {
    T::deserialize(&parse(text)?)
}

/// Reads one `T` per document in a `---`-separated stream.
pub fn from_str_all<T: Deserialize>(text: &str) -> Result<Vec<T>> {
    parse_all(text)?.iter().map(T::deserialize).collect()
}

/// Everything a document can hold, for code that wants the dynamic form.
pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Value {
    value.serialize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, quench_serde::Serialize, quench_serde::Deserialize)]
    struct Deployment {
        name: String,
        replicas: u32,
        #[serde(default)]
        labels: std::collections::BTreeMap<String, String>,
        containers: Vec<Container>,
    }

    #[derive(Debug, PartialEq, quench_serde::Serialize, quench_serde::Deserialize)]
    struct Container {
        image: String,
        args: Vec<String>,
        env: Option<Vec<String>>,
    }

    const MANIFEST: &str = "name: web\nreplicas: 3\nlabels:\n  app: web\n  tier: frontend\ncontainers:\n- image: nginx:1.25\n  args: [--port, '8080']\n  env:\n  - A=1\n- image: sidecar\n  args: []\n";

    #[quench_test::test]
    fn typed_round_trip() {
        let parsed: Deployment = from_str(MANIFEST).unwrap();
        assert_eq!(parsed.replicas, 3);
        assert_eq!(parsed.labels["tier"], "frontend");
        assert_eq!(parsed.containers[0].args, ["--port", "8080"]);
        assert_eq!(parsed.containers[1].env, None);
        let text = to_string(&parsed).unwrap();
        assert_eq!(from_str::<Deployment>(&text).unwrap(), parsed);
    }

    #[quench_test::test]
    fn editing_a_manifest_as_a_value() {
        // what gantry does: read, annotate, write back
        let mut doc =
            parse("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: cfg\ndata:\n  k: v\n")
                .unwrap();
        let metadata = doc
            .get_mut("metadata")
            .and_then(Value::as_object_mut)
            .unwrap();
        let mut annotations = Map::new();
        annotations.insert("edited", Value::from("true"));
        metadata.insert("annotations", Value::Object(annotations));
        assert_eq!(
            to_string(&doc).unwrap(),
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: cfg\n  annotations:\n    edited: 'true'\ndata:\n  k: v\n"
        );
    }

    #[quench_test::test]
    fn type_errors_name_the_path() {
        let e = from_str::<Deployment>("name: x\nreplicas: many\ncontainers: []\n").unwrap_err();
        assert_eq!(e.path(), "replicas");
        assert!(from_str::<Deployment>("name: x").is_err());
    }

    #[quench_test::test]
    fn several_documents_into_typed_values() {
        let docs: Vec<Container> =
            from_str_all("image: a\nargs: []\n---\nimage: b\nargs: [x]\n").unwrap();
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[1].args, ["x"]);
        assert!(from_str::<Container>("image: a\nargs: []\n---\nimage: b\nargs: []\n").is_err());
    }

    #[quench_test::test]
    fn to_value_is_the_dynamic_form() {
        assert_eq!(to_value(&vec![1_u8, 2]), Value::from(vec![1, 2]));
    }
}

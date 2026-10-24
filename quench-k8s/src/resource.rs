//! Kinds of object and where they live in the API.

use quench_core::time::Time;
use quench_serde::Value;

/// One kind of resource at one version: enough to build its URLs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApiResource {
    /// `""` for the core group (`Pod`, `Secret`), else `apps`, `batch`, `traefik.io`, ...
    pub group: String,
    pub version: String,
    pub kind: String,
    /// The lower-case plural used in URLs: `pods`, `deployments`.
    pub plural: String,
    /// Whether objects live in a namespace.
    pub namespaced: bool,
}

impl ApiResource {
    #[must_use]
    pub fn new(group: &str, version: &str, kind: &str, plural: &str, namespaced: bool) -> Self {
        Self {
            group: group.to_string(),
            version: version.to_string(),
            kind: kind.to_string(),
            plural: plural.to_string(),
            namespaced,
        }
    }

    /// `v1` or `apps/v1`, as written in a manifest's `apiVersion`.
    #[must_use]
    pub fn api_version(&self) -> String {
        if self.group.is_empty() {
            self.version.clone()
        } else {
            format!("{}/{}", self.group, self.version)
        }
    }

    /// The URL prefix of this group and version.
    pub(crate) fn base_path(&self) -> String {
        if self.group.is_empty() {
            format!("/api/{}", self.version)
        } else {
            format!("/apis/{}/{}", self.group, self.version)
        }
    }

    /// The resource a manifest describes, with the plural guessed from the kind (`Ingress` -> `ingresses`).
    /// Prefer discovery when the guess might be wrong (irregular plurals in custom resources).
    #[must_use]
    pub fn from_manifest(manifest: &Value, namespaced: bool) -> Option<Self> {
        let api_version = manifest.get("apiVersion")?.as_str()?;
        let kind = manifest.get("kind")?.as_str()?;
        let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));
        Some(Self::new(
            group,
            version,
            kind,
            &guess_plural(kind),
            namespaced,
        ))
    }

    #[must_use]
    pub fn pod() -> Self {
        Self::new("", "v1", "Pod", "pods", true)
    }

    #[must_use]
    pub fn service() -> Self {
        Self::new("", "v1", "Service", "services", true)
    }

    #[must_use]
    pub fn secret() -> Self {
        Self::new("", "v1", "Secret", "secrets", true)
    }

    #[must_use]
    pub fn config_map() -> Self {
        Self::new("", "v1", "ConfigMap", "configmaps", true)
    }

    #[must_use]
    pub fn namespace() -> Self {
        Self::new("", "v1", "Namespace", "namespaces", false)
    }

    #[must_use]
    pub fn node() -> Self {
        Self::new("", "v1", "Node", "nodes", false)
    }

    #[must_use]
    pub fn deployment() -> Self {
        Self::new("apps", "v1", "Deployment", "deployments", true)
    }

    #[must_use]
    pub fn stateful_set() -> Self {
        Self::new("apps", "v1", "StatefulSet", "statefulsets", true)
    }

    #[must_use]
    pub fn daemon_set() -> Self {
        Self::new("apps", "v1", "DaemonSet", "daemonsets", true)
    }

    #[must_use]
    pub fn job() -> Self {
        Self::new("batch", "v1", "Job", "jobs", true)
    }
}

/// The usual plural of a kind: lower case, `s`, `es` after `s`/`x`/`ch`/`sh`, `ies` after a consonant + `y`.
#[must_use]
pub fn guess_plural(kind: &str) -> String {
    let lower = kind.to_ascii_lowercase();
    if let Some(stem) = lower.strip_suffix('y')
        && !stem.is_empty()
        && !stem.ends_with(['a', 'e', 'i', 'o', 'u'])
    {
        return format!("{stem}ies");
    }
    if lower.ends_with(['s', 'x']) || lower.ends_with("ch") || lower.ends_with("sh") {
        return format!("{lower}es");
    }
    format!("{lower}s")
}

/// Read-only access to the common parts of an object (`metadata` and friends), without copying.
#[derive(Debug, Clone, Copy)]
pub struct Object<'a>(pub &'a Value);

impl<'a> Object<'a> {
    fn meta(self, key: &str) -> Option<&'a Value> {
        self.0.get("metadata")?.get(key)
    }

    #[must_use]
    pub fn name(self) -> Option<&'a str> {
        self.meta("name")?.as_str()
    }

    #[must_use]
    pub fn namespace(self) -> Option<&'a str> {
        self.meta("namespace")?.as_str()
    }

    #[must_use]
    pub fn uid(self) -> Option<&'a str> {
        self.meta("uid")?.as_str()
    }

    #[must_use]
    pub fn resource_version(self) -> Option<&'a str> {
        self.meta("resourceVersion")?.as_str()
    }

    #[must_use]
    pub fn kind(self) -> Option<&'a str> {
        self.0.get("kind")?.as_str()
    }

    /// One label's value.
    #[must_use]
    pub fn label(self, key: &str) -> Option<&'a str> {
        self.meta("labels")?.get(key)?.as_str()
    }

    /// One annotation's value.
    #[must_use]
    pub fn annotation(self, key: &str) -> Option<&'a str> {
        self.meta("annotations")?.get(key)?.as_str()
    }

    /// Every label.
    pub fn labels(self) -> impl Iterator<Item = (&'a str, &'a str)> {
        string_pairs(self.meta("labels"))
    }

    /// Every annotation.
    pub fn annotations(self) -> impl Iterator<Item = (&'a str, &'a str)> {
        string_pairs(self.meta("annotations"))
    }

    #[must_use]
    pub fn creation_timestamp(self) -> Option<Time> {
        Time::parse_rfc3339(self.meta("creationTimestamp")?.as_str()?).ok()
    }

    /// Set once the object is being deleted.
    #[must_use]
    pub fn deletion_timestamp(self) -> Option<Time> {
        Time::parse_rfc3339(self.meta("deletionTimestamp")?.as_str()?).ok()
    }
}

fn string_pairs(map: Option<&Value>) -> impl Iterator<Item = (&str, &str)> {
    match map {
        Some(Value::Object(map)) => Some(
            map.iter()
                .filter_map(|(k, v)| Some((k.as_str(), v.as_str()?))),
        ),
        _ => None,
    }
    .into_iter()
    .flatten()
}

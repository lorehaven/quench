//! Which resources a cluster serves.

use quench_core::future::join_all;
use quench_serde::Value;

use crate::client::Client;
use crate::error::Result;
use crate::resource::ApiResource;

/// Where an API resource lives and what can be done with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Cluster,
    Namespaced,
}

/// What a resource supports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub scope: Scope,
    /// `get`, `list`, `watch`, `create`, `delete`, ...
    pub verbs: Vec<String>,
}

impl Capabilities {
    /// Whether the verb (`"list"`, `"delete"`, ...) is allowed on the resource.
    #[must_use]
    pub fn supports_operation(&self, verb: &str) -> bool {
        self.verbs.iter().any(|v| v == verb)
    }
}

/// One API group with its preferred version's resources.
#[derive(Debug, Clone)]
pub struct ApiGroup {
    /// `""` for the core group.
    pub name: String,
    pub version: String,
    resources: Vec<(ApiResource, Capabilities)>,
}

impl ApiGroup {
    /// The group's resources at its preferred version (subresources such as `pods/log` left out).
    #[must_use]
    pub fn recommended_resources(&self) -> &[(ApiResource, Capabilities)] {
        &self.resources
    }
}

/// What a cluster serves.
#[derive(Debug, Clone, Default)]
pub struct Discovery {
    groups: Vec<ApiGroup>,
}

impl Discovery {
    pub(crate) async fn run(client: &Client) -> Result<Self> {
        // (group, preferred version) for the core group and every named one.
        let mut targets = vec![(String::new(), "v1".to_string())];
        let groups = client.get_json("/apis").await?;
        for group in groups
            .get("groups")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = group.get("name").and_then(Value::as_str);
            let version = group
                .get("preferredVersion")
                .and_then(|v| v.get("version"))
                .and_then(Value::as_str);
            if let (Some(name), Some(version)) = (name, version) {
                targets.push((name.to_string(), version.to_string()));
            }
        }
        let fetched = join_all(targets.iter().map(|(group, version)| {
            let path = if group.is_empty() {
                format!("/api/{version}")
            } else {
                format!("/apis/{group}/{version}")
            };
            async move { client.get_json(&path).await }
        }))
        .await;

        let mut out = Vec::new();
        for ((group, version), list) in targets.into_iter().zip(fetched) {
            // A group the account may not read, or one that is unavailable, is simply not listed.
            let Ok(list) = list else { continue };
            out.push(ApiGroup {
                resources: resources_of(&group, &version, &list),
                name: group,
                version,
            });
        }
        Ok(Self { groups: out })
    }

    #[must_use]
    pub fn groups(&self) -> &[ApiGroup] {
        &self.groups
    }

    /// The resource of that `kind` (any group), if the cluster serves one.
    #[must_use]
    pub fn find_kind(&self, kind: &str) -> Option<&(ApiResource, Capabilities)> {
        self.groups
            .iter()
            .flat_map(|g| g.resources.iter())
            .find(|(r, _)| r.kind == kind)
    }

    /// The resource a manifest's `apiVersion` and `kind` name.
    #[must_use]
    pub fn find_manifest(&self, manifest: &Value) -> Option<&(ApiResource, Capabilities)> {
        let api_version = manifest.get("apiVersion")?.as_str()?;
        let kind = manifest.get("kind")?.as_str()?;
        let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));
        self.groups
            .iter()
            .flat_map(|g| g.resources.iter())
            .find(|(r, _)| r.kind == kind && r.group == group && r.version == version)
    }
}

fn resources_of(group: &str, version: &str, list: &Value) -> Vec<(ApiResource, Capabilities)> {
    let mut out = Vec::new();
    for resource in list
        .get("resources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let text = |key: &str| resource.get(key).and_then(Value::as_str);
        let (Some(plural), Some(kind)) = (text("name"), text("kind")) else {
            continue;
        };
        if plural.contains('/') {
            continue;
        }
        let namespaced = resource.get("namespaced").and_then(Value::as_bool) == Some(true);
        let verbs = resource
            .get("verbs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        out.push((
            ApiResource::new(group, version, kind, plural, namespaced),
            Capabilities {
                scope: if namespaced {
                    Scope::Namespaced
                } else {
                    Scope::Cluster
                },
                verbs,
            },
        ));
    }
    out
}

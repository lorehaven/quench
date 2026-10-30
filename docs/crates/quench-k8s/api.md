---
title: API reference
parent: quench-k8s
grand_parent: Crates
nav_order: 2
---

Public items of `quench_k8s`, in source order. Re-exports in `src/lib.rs`:

```rust
pub use client::{Api, Client, DeleteParams, ListParams, LogParams, ObjectList, Patch, PatchParams,
                 PostParams, Propagation, WatchEvent, lines};
pub use config::{Auth, Config};
pub use discovery::{ApiGroup, Capabilities, Discovery, Scope};
pub use error::{Error, Result, Status};
pub use resource::{ApiResource, Object, guess_plural};
```

All objects are `quench_serde::Value`. No `unsafe`. `Client` and `Api` are `Clone + Send + Sync` (an `Arc` inside). Configuration errors: see [Errors](errors.md).

## `client`

### `Client`

```rust
#[derive(Debug, Clone)]
pub struct Client { /* Arc<Inner { http: quench_client::transport::Client, config: Config }> */ }

impl Client {
    pub fn new(config: Config) -> Result<Self>;
    pub fn infer() -> Result<Self>;
    #[must_use] pub fn config(&self) -> &Config;
    #[must_use] pub fn default_namespace(&self) -> &str;
    #[must_use] pub fn api(&self, resource: &ApiResource) -> Api;
    #[must_use] pub fn namespaced(&self, resource: &ApiResource, namespace: &str) -> Api;
    pub async fn discover(&self) -> Result<Discovery>;
    pub async fn get_json(&self, path: &str) -> Result<Value>;
}
```

* `new`: builds the HTTP client with user agent `quench-k8s`; applies `config.timeout` if `Some`; `danger_accept_invalid_certs(true)` if `config.insecure`; adds `config.ca_pem` as a root; for `Auth::ClientCertificate` sets the client identity. `Error::Config` if the HTTP client cannot be built (bad certificate material).
* `infer`: `Client::new(Config::infer()?)`.
* `default_namespace`: `config.namespace`.
* `api`: all namespaces, or a cluster-scoped kind. `namespaced`: one namespace (ignored for cluster-scoped kinds).
* `discover`: [`Discovery`](#discovery).
* `get_json(path)`: `GET {server}{path}` with `Accept: application/json`, body decoded as JSON. `Error::Api` for a non-success status, `Error::Transport` when unreachable, `Error::Decode` when the body is not JSON.

Each request carries `Accept: application/json` and the authentication from `Config::auth`: `Bearer` and `BearerFile` as a bearer token (the file is read on every request and trimmed; unreadable gives `Error::Config("cannot read the token file <path>: <e>")`), `Basic` as basic auth, `None` and `ClientCertificate` add no header.

### `Api`

```rust
#[derive(Debug, Clone)]
pub struct Api { /* client, resource, namespace: Option<String> */ }

impl Api {
    #[must_use] pub const fn resource(&self) -> &ApiResource;
    pub async fn get(&self, name: &str) -> Result<Value>;
    pub async fn get_opt(&self, name: &str) -> Result<Option<Value>>;
    pub async fn list(&self, params: &ListParams) -> Result<ObjectList>;
    pub async fn create(&self, object: &Value, params: &PostParams) -> Result<Value>;
    pub async fn replace(&self, name: &str, object: &Value) -> Result<Value>;
    pub async fn patch(&self, name: &str, patch: &Patch, params: &PatchParams) -> Result<Value>;
    pub async fn delete(&self, name: &str, params: &DeleteParams) -> Result<Value>;
    pub async fn logs(&self, name: &str, params: &LogParams) -> Result<String>;
    pub async fn log_stream(&self, name: &str, params: &LogParams)
        -> Result<BoxStream<'static, Result<Bytes>>>;
    pub async fn watch(&self, params: &ListParams, resource_version: &str)
        -> Result<BoxStream<'static, Result<WatchEvent>>>;
}
```

`BoxStream` is `quench_core::stream::BoxStream`, `Bytes` is `quench_core::bytes::Bytes`.

| Method | HTTP | Path | Notes |
|---|---|---|---|
| `get` | GET | object path | 404 gives `Error::Api`. |
| `get_opt` | GET | object path | 404 gives `Ok(None)`; any other error is returned. |
| `list` | GET | collection | Pages are followed (see below). |
| `create` | POST | collection | JSON body. Query: `dryRun=All` if `dry_run`; `fieldManager`. |
| `replace` | PUT | object path | JSON body; no query. |
| `patch` | PATCH | object path | Body and content type from [`Patch`](#patch). Query: `dryRun=All`, `fieldManager`, `force=true`. |
| `delete` | DELETE | object path | Query `dryRun=All`; optional `DeleteOptions` body (below). Returns the answer (deleted object or `Status`). |
| `logs` | GET | `<object path>/log` | Text body. |
| `log_stream` | GET | `<object path>/log` | Streaming response; chunks as they arrive. |
| `watch` | GET | collection | Query `watch=true`, `allowWatchBookmarks=true`, plus `resourceVersion` if non-empty; selectors from `params` (`page_size` is ignored). |

Paths: the collection is `<base>/namespaces/<ns>/<plural>` when the resource is namespaced and a namespace is set, else `<base>/<plural>`; `<base>` is `/api/<version>` for the core group, `/apis/<group>/<version>` otherwise. Object path is `<collection>/<name>`; name and namespace are percent-encoded. For a namespaced resource on an `Api` without a namespace, every per-object method (`get`, `get_opt`, `replace`, `patch`, `delete`, `logs`, `log_stream`) fails with `Error::Config("<Kind> is namespaced: use `namespaced(..)` to name the object's namespace")`. `list`, `create` and `watch` on such an `Api` use the all-namespaces collection path (for `create` that is the cluster-wide collection, which the API server refuses).

`list`: sends `labelSelector`, `fieldSelector`, `limit` (if `page_size`), `continue` (after the first page); concatenates `items`; `resource_version` is that of the last page; stops when `metadata.continue` is absent or empty.

`logs` errors: 400 while the container has not started, 404 for no such pod (as `Error::Api`). `log_stream`: `Error::Api` if refused before the stream starts; later chunk errors are `Error::Transport`.

`watch`: the stream yields one `WatchEvent` per line of the response (see [Internals](internals.md)). It ends when the server closes the watch; resume from the last version.

### `ListParams`

```rust
#[derive(Debug, Clone, Default)]
pub struct ListParams {
    pub label_selector: Option<String>,
    pub field_selector: Option<String>,
    pub page_size: Option<u32>,
}
impl ListParams {
    #[must_use] pub fn labels(self, selector: &str) -> Self;
    #[must_use] pub fn fields(self, selector: &str) -> Self;
}
```

### `PostParams`

```rust
#[derive(Debug, Clone, Default)]
pub struct PostParams { pub dry_run: bool, pub field_manager: Option<String> }
```

### `PatchParams`

```rust
#[derive(Debug, Clone, Default)]
pub struct PatchParams { pub dry_run: bool, pub field_manager: Option<String>, pub force: bool }
```

`force` is for `Patch::Apply`: take over fields other managers own.

### `Patch`

```rust
#[derive(Debug, Clone)]
pub enum Patch { Merge(Value), StrategicMerge(Value), Json(Value), Apply(Value) }
```

| Variant | `Content-Type` |
|---|---|
| `Merge` (RFC 7386) | `application/merge-patch+json` |
| `StrategicMerge` | `application/strategic-merge-patch+json` |
| `Json` (RFC 6902, an array of operations) | `application/json-patch+json` |
| `Apply` (server-side apply) | `application/apply-patch+yaml` |

The body is always the value serialized as JSON (also for `Apply`, JSON being valid YAML).

### `Propagation`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Propagation { Background, Foreground, Orphan }
```

Sent as `propagationPolicy` `"Background"`, `"Foreground"`, `"Orphan"`.

### `DeleteParams`

```rust
#[derive(Debug, Clone, Default)]
pub struct DeleteParams {
    pub propagation: Option<Propagation>,
    pub grace_period_seconds: Option<u32>,
    pub dry_run: bool,
}
impl DeleteParams { #[must_use] pub fn background() -> Self; } // propagation = Some(Background)
```

Body: when `propagation` or `grace_period_seconds` is set, `{"propagationPolicy": .., "gracePeriodSeconds": .., "kind": "DeleteOptions", "apiVersion": "v1"}` (absent keys omitted) with `Content-Type: application/json`; otherwise no body. `dry_run` goes in the query as `dryRun=All`.

### `LogParams`

```rust
#[derive(Debug, Clone, Default)]
pub struct LogParams {
    pub container: Option<String>,
    pub follow: bool,
    pub tail_lines: Option<u32>,
    pub previous: bool,
    pub timestamps: bool,
    pub since_seconds: Option<u32>,
}
```

Query: `container`, `follow=true`, `previous=true`, `timestamps=true` (only when true), `tailLines`, `sinceSeconds`.

### `ObjectList`

```rust
#[derive(Debug, Clone, Default)]
pub struct ObjectList { pub items: Vec<Value>, pub resource_version: Option<String> }
```

### `WatchEvent`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum WatchEvent {
    Added(Value), Modified(Value), Deleted(Value),
    Bookmark(Value),   // progress marker carrying the latest resource version
    Error(Status),     // e.g. an expired resource version (410)
}
```

### `lines`

```rust
#[must_use]
pub fn lines(bytes: BoxStream<'static, Result<Bytes>>) -> BoxStream<'static, Result<String>>;
```

Splits a byte stream into lines without the `\n` (and a preceding `\r`); a final line without a break is kept; invalid UTF-8 is replaced lossily. A stream error is yielded as an `Err` item, the buffered partial line is discarded and the stream ends.

## `config`

### `Auth`

```rust
#[derive(Clone, PartialEq, Eq)]
pub enum Auth {
    None,
    Bearer(String),
    BearerFile(PathBuf),
    ClientCertificate { certificate: Vec<u8>, key: Vec<u8> }, // PEM
    Basic { username: String, password: String },
}
```

`Debug` prints only `Auth::None`, `Auth::Bearer(..)`, `Auth::BearerFile(..)`, `Auth::ClientCertificate(..)`, `Auth::Basic(..)`.

### `Config`

```rust
#[derive(Debug, Clone)]
pub struct Config {
    pub server: String,
    pub namespace: String,
    pub ca_pem: Option<Vec<u8>>,
    pub insecure: bool,
    pub auth: Auth,
    pub timeout: Option<Duration>,
}

impl Config {
    #[must_use] pub fn new(server: &str) -> Self;
    pub fn infer() -> Result<Self>;
    pub fn infer_with_context(context: Option<&str>) -> Result<Self>;
    pub fn in_cluster() -> Result<Self>;
    pub fn in_cluster_at(directory: &Path, host: &str, port: &str) -> Result<Self>;
    pub fn from_kubeconfig(path: &Path, context: Option<&str>) -> Result<Self>;
    pub fn from_kubeconfig_str(text: &str, context: Option<&str>, base: Option<&Path>) -> Result<Self>;
}
```

Details are in [Configuration](configuration.md). `new` trims trailing `/` from `server`, namespace `default`, no CA, `insecure: false`, `Auth::None`, `timeout: Some(30 s)`.

## `discovery`

### `Scope`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope { Cluster, Namespaced }
```

### `Capabilities`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities { pub scope: Scope, pub verbs: Vec<String> }
impl Capabilities { #[must_use] pub fn supports_operation(&self, verb: &str) -> bool; }
```

`verbs` are as served (`get`, `list`, `watch`, `create`, `delete`, ...).

### `ApiGroup`

```rust
#[derive(Debug, Clone)]
pub struct ApiGroup { pub name: String, pub version: String, /* private resources */ }
impl ApiGroup { #[must_use] pub fn recommended_resources(&self) -> &[(ApiResource, Capabilities)]; }
```

`name` is `""` for the core group. Resources are those of the group's preferred version; resources whose name contains `/` (subresources such as `pods/log`) are left out.

### `Discovery`

```rust
#[derive(Debug, Clone, Default)]
pub struct Discovery { /* private groups */ }
impl Discovery {
    #[must_use] pub fn groups(&self) -> &[ApiGroup];
    #[must_use] pub fn find_kind(&self, kind: &str) -> Option<&(ApiResource, Capabilities)>;
    #[must_use] pub fn find_manifest(&self, manifest: &Value) -> Option<&(ApiResource, Capabilities)>;
}
```

Created only by `Client::discover`. `find_kind` returns the first resource of that kind in any group (group order: core first, then the order of `/apis`). `find_manifest` splits `apiVersion` at `/` (no slash: core group) and matches kind, group and version; `None` if either field is missing or not a string.

## `error`

See [Errors](errors.md): `Status`, `Error`, `Result<T>`.

## `resource`

### `ApiResource`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApiResource {
    pub group: String,       // "" for the core group
    pub version: String,
    pub kind: String,
    pub plural: String,      // lower-case plural used in URLs
    pub namespaced: bool,
}

impl ApiResource {
    #[must_use] pub fn new(group: &str, version: &str, kind: &str, plural: &str, namespaced: bool) -> Self;
    #[must_use] pub fn api_version(&self) -> String;                 // "v1" or "apps/v1"
    #[must_use] pub fn from_manifest(manifest: &Value, namespaced: bool) -> Option<Self>;
    #[must_use] pub fn pod() -> Self;          // ""/v1 Pod pods, namespaced
    #[must_use] pub fn service() -> Self;      // ""/v1 Service services, namespaced
    #[must_use] pub fn secret() -> Self;       // ""/v1 Secret secrets, namespaced
    #[must_use] pub fn config_map() -> Self;   // ""/v1 ConfigMap configmaps, namespaced
    #[must_use] pub fn namespace() -> Self;    // ""/v1 Namespace namespaces, cluster
    #[must_use] pub fn node() -> Self;         // ""/v1 Node nodes, cluster
    #[must_use] pub fn deployment() -> Self;   // apps/v1 Deployment deployments, namespaced
    #[must_use] pub fn stateful_set() -> Self; // apps/v1 StatefulSet statefulsets, namespaced
    #[must_use] pub fn daemon_set() -> Self;   // apps/v1 DaemonSet daemonsets, namespaced
    #[must_use] pub fn job() -> Self;          // batch/v1 Job jobs, namespaced
}
```

`from_manifest` reads `apiVersion` and `kind` (strings, else `None`), splits `apiVersion` at its first `/`, and guesses the plural with `guess_plural`; `namespaced` is the caller's choice.

### `guess_plural`

```rust
#[must_use]
pub fn guess_plural(kind: &str) -> String;
```

Lower-cases the kind, then: consonant + `y` becomes `ies`; ending in `s`, `x`, `ch` or `sh` gets `es`; otherwise `s`. Examples: `Ingress` to `ingresses`, `NetworkPolicy` to `networkpolicies`, `Gateway` to `gateways`.

### `Object`

```rust
#[derive(Debug, Clone, Copy)]
pub struct Object<'a>(pub &'a Value);

impl<'a> Object<'a> {
    #[must_use] pub fn name(self) -> Option<&'a str>;                 // metadata.name
    #[must_use] pub fn namespace(self) -> Option<&'a str>;            // metadata.namespace
    #[must_use] pub fn uid(self) -> Option<&'a str>;                  // metadata.uid
    #[must_use] pub fn resource_version(self) -> Option<&'a str>;     // metadata.resourceVersion
    #[must_use] pub fn kind(self) -> Option<&'a str>;                 // kind (top level)
    #[must_use] pub fn label(self, key: &str) -> Option<&'a str>;
    #[must_use] pub fn annotation(self, key: &str) -> Option<&'a str>;
    pub fn labels(self) -> impl Iterator<Item = (&'a str, &'a str)>;
    pub fn annotations(self) -> impl Iterator<Item = (&'a str, &'a str)>;
    #[must_use] pub fn creation_timestamp(self) -> Option<Time>;      // RFC 3339
    #[must_use] pub fn deletion_timestamp(self) -> Option<Time>;
}
```

Read-only, no copying. `labels`/`annotations` skip entries whose value is not a string. Timestamps are `quench_core::time::Time`, `None` if absent or not RFC 3339.

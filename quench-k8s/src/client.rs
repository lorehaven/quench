//! The HTTP side: authenticated requests, and the dynamic [`Api`] for one resource.

use std::sync::Arc;

use quench_client::transport::{self, Method};
use quench_core::bytes::Bytes;
use quench_core::stream::{BoxStream, StreamExt, unfold};
use quench_serde::{Value, json};

use crate::config::{Auth, Config};
use crate::discovery::Discovery;
use crate::error::{Error, Result, Status};
use crate::resource::ApiResource;

const JSON: &str = "application/json";

/// A connection to one cluster. Cloning is cheap; clones share the connection pool.
#[derive(Debug, Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    http: transport::Client,
    config: Config,
}

impl Client {
    /// Opens a client for `config`.
    ///
    /// # Errors
    ///
    /// [`Error::Config`] when the certificate material is unusable.
    pub fn new(config: Config) -> Result<Self> {
        let mut builder = transport::Client::builder().user_agent("quench-k8s");
        if let Some(timeout) = config.timeout {
            builder = builder.timeout(timeout);
        }
        if config.insecure {
            builder = builder.danger_accept_invalid_certs(true);
        }
        if let Some(ca) = &config.ca_pem {
            builder = builder.add_root_certificate_pem(ca.clone());
        }
        if let Auth::ClientCertificate { certificate, key } = &config.auth {
            builder = builder.client_identity_pem(certificate.clone(), key.clone());
        }
        let http = builder.build().map_err(|e| Error::Config(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(Inner { http, config }),
        })
    }

    /// A client for the cluster [`Config::infer`] finds.
    ///
    /// # Errors
    ///
    /// As [`Config::infer`] and [`Client::new`].
    pub fn infer() -> Result<Self> {
        Self::new(Config::infer()?)
    }

    #[must_use]
    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    /// The namespace used when none is named.
    #[must_use]
    pub fn default_namespace(&self) -> &str {
        &self.inner.config.namespace
    }

    /// The resource's objects across all namespaces (or the cluster-scoped ones).
    #[must_use]
    pub fn api(&self, resource: &ApiResource) -> Api {
        Api {
            client: self.clone(),
            resource: resource.clone(),
            namespace: None,
        }
    }

    /// The resource's objects in one namespace (ignored for cluster-scoped kinds).
    #[must_use]
    pub fn namespaced(&self, resource: &ApiResource, namespace: &str) -> Api {
        Api {
            client: self.clone(),
            resource: resource.clone(),
            namespace: Some(namespace.to_string()),
        }
    }

    /// Every resource the cluster serves (the preferred version of each group).
    ///
    /// # Errors
    ///
    /// Any failure talking to the API server.
    pub async fn discover(&self) -> Result<Discovery> {
        Discovery::run(self).await
    }

    /// `GET path` as JSON, for endpoints without a typed method (`/version`, `/healthz`, custom paths).
    ///
    /// # Errors
    ///
    /// [`Error::Api`] for an error status, [`Error::Transport`] when the server cannot be reached.
    pub async fn get_json(&self, path: &str) -> Result<Value> {
        self.send_json(Method::GET, path, &[], None).await
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.inner.config.server)
    }

    fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
    ) -> Result<transport::RequestBuilder> {
        let mut request = self
            .inner
            .http
            .request(method, self.url(path))
            .header("accept", JSON);
        if !query.is_empty() {
            request = request.query(query);
        }
        Ok(match &self.inner.config.auth {
            Auth::Bearer(token) => request.bearer_auth(token),
            Auth::BearerFile(path) => {
                let token = std::fs::read_to_string(path).map_err(|e| {
                    Error::Config(format!(
                        "cannot read the token file {}: {e}",
                        path.display()
                    ))
                })?;
                request.bearer_auth(token.trim())
            }
            Auth::Basic { username, password } => request.basic_auth(username, Some(password)),
            Auth::None | Auth::ClientCertificate { .. } => request,
        })
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        body: Option<(Vec<u8>, &str)>,
        streaming: bool,
    ) -> Result<transport::Response> {
        let mut request = self.request(method, path, query)?;
        if let Some((bytes, content_type)) = body {
            request = request.header("content-type", content_type).body(bytes);
        }
        let response = if streaming {
            request.send_streaming().await
        } else {
            request.send().await
        }
        .map_err(|e| Error::Transport(e.to_string()))?;
        if response.status().is_success() {
            return Ok(response);
        }
        let code = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        Err(Error::Api(parse_status(code, &text)))
    }

    async fn send_json(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        body: Option<(Vec<u8>, &str)>,
    ) -> Result<Value> {
        let response = self.send(method, path, query, body, false).await?;
        let bytes = response
            .bytes()
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        json::from_slice(&bytes).map_err(|e| Error::Decode(e.to_string()))
    }
}

/// The server's error body as a [`Status`]; any other text becomes the message.
fn parse_status(code: u16, body: &str) -> Status {
    if let Ok(value) = json::from_str::<Value>(body)
        && value.get("kind").and_then(Value::as_str) == Some("Status")
    {
        let text = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        return Status {
            code: value
                .get("code")
                .and_then(Value::as_u64)
                .and_then(|c| u16::try_from(c).ok())
                .unwrap_or(code),
            reason: text("reason"),
            message: text("message"),
        };
    }
    Status {
        code,
        reason: String::new(),
        message: body.trim().to_string(),
    }
}

/// Which objects a list or watch covers.
#[derive(Debug, Clone, Default)]
pub struct ListParams {
    /// `app=web,tier!=db` style selector.
    pub label_selector: Option<String>,
    /// `status.phase=Running` style selector.
    pub field_selector: Option<String>,
    /// Page size for [`Api::list`], which follows the continuation itself (the server's default if `None`).
    pub page_size: Option<u32>,
}

impl ListParams {
    #[must_use]
    pub fn labels(mut self, selector: &str) -> Self {
        self.label_selector = Some(selector.to_string());
        self
    }

    #[must_use]
    pub fn fields(mut self, selector: &str) -> Self {
        self.field_selector = Some(selector.to_string());
        self
    }

    fn query(&self) -> Vec<(String, String)> {
        let mut query = Vec::new();
        if let Some(labels) = &self.label_selector {
            query.push(("labelSelector".to_string(), labels.clone()));
        }
        if let Some(fields) = &self.field_selector {
            query.push(("fieldSelector".to_string(), fields.clone()));
        }
        query
    }
}

/// Options for creating an object.
#[derive(Debug, Clone, Default)]
pub struct PostParams {
    /// Validate and report what would happen without storing anything.
    pub dry_run: bool,
    pub field_manager: Option<String>,
}

/// Options for patching an object.
#[derive(Debug, Clone, Default)]
pub struct PatchParams {
    pub dry_run: bool,
    pub field_manager: Option<String>,
    /// For [`Patch::Apply`]: take over fields other managers own.
    pub force: bool,
}

/// How a patch is written.
#[derive(Debug, Clone)]
pub enum Patch {
    /// RFC 7386 JSON merge patch.
    Merge(Value),
    /// Kubernetes strategic merge patch (lists of known kinds merge by key).
    StrategicMerge(Value),
    /// RFC 6902 JSON patch: an array of operations.
    Json(Value),
    /// Server-side apply of a full object; needs a `field_manager`.
    Apply(Value),
}

impl Patch {
    const fn content_type(&self) -> &'static str {
        match self {
            Self::Merge(_) => "application/merge-patch+json",
            Self::StrategicMerge(_) => "application/strategic-merge-patch+json",
            Self::Json(_) => "application/json-patch+json",
            Self::Apply(_) => "application/apply-patch+yaml",
        }
    }

    fn body(&self) -> Vec<u8> {
        let (Self::Merge(v) | Self::StrategicMerge(v) | Self::Json(v) | Self::Apply(v)) = self;
        json::to_vec(v)
    }
}

/// What happens to the objects an object owns when it is deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Propagation {
    /// Delete the object at once; owned objects follow in the background.
    Background,
    /// Wait for owned objects to be deleted first.
    Foreground,
    /// Leave owned objects behind.
    Orphan,
}

/// Options for deleting an object.
#[derive(Debug, Clone, Default)]
pub struct DeleteParams {
    pub propagation: Option<Propagation>,
    pub grace_period_seconds: Option<u32>,
    pub dry_run: bool,
}

impl DeleteParams {
    /// Delete now and let the garbage collector remove what the object owns (a Job's pods).
    #[must_use]
    pub fn background() -> Self {
        Self {
            propagation: Some(Propagation::Background),
            ..Self::default()
        }
    }

    fn body(&self) -> Option<Vec<u8>> {
        let mut map = quench_serde::Map::new();
        if let Some(p) = self.propagation {
            let name = match p {
                Propagation::Background => "Background",
                Propagation::Foreground => "Foreground",
                Propagation::Orphan => "Orphan",
            };
            map.insert("propagationPolicy", Value::String(name.to_string()));
        }
        if let Some(seconds) = self.grace_period_seconds {
            map.insert("gracePeriodSeconds", Value::UInt(u64::from(seconds)));
        }
        if map.is_empty() {
            return None;
        }
        map.insert("kind", Value::String("DeleteOptions".to_string()));
        map.insert("apiVersion", Value::String("v1".to_string()));
        Some(json::to_vec(&Value::Object(map)))
    }
}

/// Options for reading a pod's log.
#[derive(Debug, Clone, Default)]
pub struct LogParams {
    pub container: Option<String>,
    /// Keep the connection open and send new lines as they appear.
    pub follow: bool,
    pub tail_lines: Option<u32>,
    /// The previous instance of the container.
    pub previous: bool,
    pub timestamps: bool,
    pub since_seconds: Option<u32>,
}

impl LogParams {
    fn query(&self) -> Vec<(String, String)> {
        let mut query = Vec::new();
        if let Some(container) = &self.container {
            query.push(("container".to_string(), container.clone()));
        }
        for (name, on) in [
            ("follow", self.follow),
            ("previous", self.previous),
            ("timestamps", self.timestamps),
        ] {
            if on {
                query.push((name.to_string(), "true".to_string()));
            }
        }
        if let Some(lines) = self.tail_lines {
            query.push(("tailLines".to_string(), lines.to_string()));
        }
        if let Some(seconds) = self.since_seconds {
            query.push(("sinceSeconds".to_string(), seconds.to_string()));
        }
        query
    }
}

/// The objects a list returned.
#[derive(Debug, Clone, Default)]
pub struct ObjectList {
    pub items: Vec<Value>,
    /// Where a watch can pick up: pass it to [`Api::watch`].
    pub resource_version: Option<String>,
}

/// A change reported by a watch.
#[derive(Debug, Clone, PartialEq)]
pub enum WatchEvent {
    Added(Value),
    Modified(Value),
    Deleted(Value),
    /// A progress marker carrying the latest resource version.
    Bookmark(Value),
    /// The watch ended with an error (an expired resource version, 410).
    Error(Status),
}

/// One kind of resource, optionally within one namespace; every call returns plain [`Value`]s.
#[derive(Debug, Clone)]
pub struct Api {
    client: Client,
    resource: ApiResource,
    namespace: Option<String>,
}

impl Api {
    #[must_use]
    pub const fn resource(&self) -> &ApiResource {
        &self.resource
    }

    /// The path of the collection (namespaced when the resource is and a namespace is set).
    fn collection(&self) -> String {
        let base = self.resource.base_path();
        match (&self.namespace, self.resource.namespaced) {
            (Some(ns), true) => {
                format!("{base}/namespaces/{}/{}", encode(ns), self.resource.plural)
            }
            _ => format!("{base}/{}", self.resource.plural),
        }
    }

    /// The path of one object; a namespaced resource needs the API to have a namespace.
    fn object(&self, name: &str) -> Result<String> {
        if self.resource.namespaced && self.namespace.is_none() {
            return Err(Error::Config(format!(
                "{} is namespaced: use `namespaced(..)` to name the object's namespace",
                self.resource.kind
            )));
        }
        Ok(format!("{}/{}", self.collection(), encode(name)))
    }

    /// The object called `name`.
    ///
    /// # Errors
    ///
    /// [`Error::Api`] (404 when it does not exist).
    pub async fn get(&self, name: &str) -> Result<Value> {
        self.client
            .send_json(Method::GET, &self.object(name)?, &[], None)
            .await
    }

    /// Like [`get`](Self::get), `None` when it does not exist.
    ///
    /// # Errors
    ///
    /// Any error but 404.
    pub async fn get_opt(&self, name: &str) -> Result<Option<Value>> {
        match self.get(name).await {
            Ok(object) => Ok(Some(object)),
            Err(e) if e.is_not_found() => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Every matching object (following the server's pages).
    ///
    /// # Errors
    ///
    /// Any failure talking to the API server.
    pub async fn list(&self, params: &ListParams) -> Result<ObjectList> {
        let mut out = ObjectList::default();
        let mut token: Option<String> = None;
        loop {
            let mut query = params.query();
            if let Some(size) = params.page_size {
                query.push(("limit".to_string(), size.to_string()));
            }
            if let Some(token) = &token {
                query.push(("continue".to_string(), token.clone()));
            }
            let page = self
                .client
                .send_json(Method::GET, &self.collection(), &query, None)
                .await?;
            if let Some(Value::Array(items)) = page.get("items") {
                out.items.extend(items.iter().cloned());
            }
            let meta = page.get("metadata");
            out.resource_version = meta
                .and_then(|m| m.get("resourceVersion"))
                .and_then(Value::as_str)
                .map(str::to_string);
            token = meta
                .and_then(|m| m.get("continue"))
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .map(str::to_string);
            if token.is_none() {
                return Ok(out);
            }
        }
    }

    /// Stores a new object.
    ///
    /// # Errors
    ///
    /// [`Error::Api`] (409 when it already exists).
    pub async fn create(&self, object: &Value, params: &PostParams) -> Result<Value> {
        let mut query = Vec::new();
        if params.dry_run {
            query.push(("dryRun".to_string(), "All".to_string()));
        }
        if let Some(manager) = &params.field_manager {
            query.push(("fieldManager".to_string(), manager.clone()));
        }
        self.client
            .send_json(
                Method::POST,
                &self.collection(),
                &query,
                Some((json::to_vec(object), JSON)),
            )
            .await
    }

    /// Replaces the object called `name` (the object should carry the `resourceVersion` it was read at).
    ///
    /// # Errors
    ///
    /// [`Error::Api`] (409 when it changed meanwhile).
    pub async fn replace(&self, name: &str, object: &Value) -> Result<Value> {
        self.client
            .send_json(
                Method::PUT,
                &self.object(name)?,
                &[],
                Some((json::to_vec(object), JSON)),
            )
            .await
    }

    /// Changes parts of the object called `name`.
    ///
    /// # Errors
    ///
    /// [`Error::Api`].
    pub async fn patch(&self, name: &str, patch: &Patch, params: &PatchParams) -> Result<Value> {
        let mut query = Vec::new();
        if params.dry_run {
            query.push(("dryRun".to_string(), "All".to_string()));
        }
        if let Some(manager) = &params.field_manager {
            query.push(("fieldManager".to_string(), manager.clone()));
        }
        if params.force {
            query.push(("force".to_string(), "true".to_string()));
        }
        self.client
            .send_json(
                Method::PATCH,
                &self.object(name)?,
                &query,
                Some((patch.body(), patch.content_type())),
            )
            .await
    }

    /// Deletes the object called `name`; the answer is the deleted object or a `Status`.
    ///
    /// # Errors
    ///
    /// [`Error::Api`] (404 when it does not exist).
    pub async fn delete(&self, name: &str, params: &DeleteParams) -> Result<Value> {
        let mut query = Vec::new();
        if params.dry_run {
            query.push(("dryRun".to_string(), "All".to_string()));
        }
        let body = params.body().map(|b| (b, JSON));
        self.client
            .send_json(Method::DELETE, &self.object(name)?, &query, body)
            .await
    }

    /// A pod's log as text.
    ///
    /// # Errors
    ///
    /// [`Error::Api`] (400 while the container has not started, 404 for no such pod).
    pub async fn logs(&self, name: &str, params: &LogParams) -> Result<String> {
        let path = format!("{}/log", self.object(name)?);
        let response = self
            .client
            .send(Method::GET, &path, &params.query(), None, false)
            .await?;
        response
            .text()
            .await
            .map_err(|e| Error::Transport(e.to_string()))
    }

    /// A pod's log as it arrives (set `follow` to keep it open); each item is a piece of the byte stream.
    ///
    /// # Errors
    ///
    /// [`Error::Api`] when the server refuses before the stream starts.
    pub async fn log_stream(
        &self,
        name: &str,
        params: &LogParams,
    ) -> Result<BoxStream<'static, Result<Bytes>>> {
        let path = format!("{}/log", self.object(name)?);
        let response = self
            .client
            .send(Method::GET, &path, &params.query(), None, true)
            .await?;
        Ok(Box::pin(response.bytes_stream().map(|chunk| {
            chunk.map_err(|e| Error::Transport(e.to_string()))
        })))
    }

    /// Changes to the matching objects from `resource_version` on (use [`ObjectList::resource_version`]).
    ///
    /// The stream ends when the server closes the watch (it does so periodically); start another from the
    /// last version seen.
    ///
    /// # Errors
    ///
    /// [`Error::Api`] when the server refuses before the stream starts.
    pub async fn watch(
        &self,
        params: &ListParams,
        resource_version: &str,
    ) -> Result<BoxStream<'static, Result<WatchEvent>>> {
        let mut query = params.query();
        query.push(("watch".to_string(), "true".to_string()));
        query.push(("allowWatchBookmarks".to_string(), "true".to_string()));
        if !resource_version.is_empty() {
            query.push(("resourceVersion".to_string(), resource_version.to_string()));
        }
        let response = self
            .client
            .send(Method::GET, &self.collection(), &query, None, true)
            .await?;
        let bytes: BoxStream<'static, Result<Bytes>> = Box::pin(
            response
                .bytes_stream()
                .map(|chunk| chunk.map_err(|e| Error::Transport(e.to_string()))),
        );
        Ok(Box::pin(lines(bytes).map(|line| {
            let line = line?;
            let value: Value = json::from_str(&line).map_err(|e| Error::Decode(e.to_string()))?;
            decode_event(&value)
        })))
    }
}

fn decode_event(value: &Value) -> Result<WatchEvent> {
    let object = value
        .get("object")
        .cloned()
        .ok_or_else(|| Error::Decode("a watch event without an object".to_string()))?;
    match value.get("type").and_then(Value::as_str) {
        Some("ADDED") => Ok(WatchEvent::Added(object)),
        Some("MODIFIED") => Ok(WatchEvent::Modified(object)),
        Some("DELETED") => Ok(WatchEvent::Deleted(object)),
        Some("BOOKMARK") => Ok(WatchEvent::Bookmark(object)),
        Some("ERROR") => Ok(WatchEvent::Error(parse_status(
            object
                .get("code")
                .and_then(Value::as_u64)
                .and_then(|c| u16::try_from(c).ok())
                .unwrap_or(0),
            &json::to_string(&object),
        ))),
        other => Err(Error::Decode(format!("unknown watch event type {other:?}"))),
    }
}

/// Splits a byte stream into lines (without the line break; a final line without one is kept).
#[must_use]
pub fn lines(bytes: BoxStream<'static, Result<Bytes>>) -> BoxStream<'static, Result<String>> {
    struct State {
        bytes: BoxStream<'static, Result<Bytes>>,
        buffer: Vec<u8>,
        done: bool,
    }
    Box::pin(unfold(
        State {
            bytes,
            buffer: Vec::new(),
            done: false,
        },
        |mut state| async move {
            loop {
                if let Some(at) = state.buffer.iter().position(|&b| b == b'\n') {
                    let mut line: Vec<u8> = state.buffer.drain(..=at).collect();
                    line.pop();
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    return Some((Ok(String::from_utf8_lossy(&line).into_owned()), state));
                }
                if state.done {
                    if state.buffer.is_empty() {
                        return None;
                    }
                    let line =
                        String::from_utf8_lossy(&std::mem::take(&mut state.buffer)).into_owned();
                    return Some((Ok(line), state));
                }
                match state.bytes.next().await {
                    Some(Ok(chunk)) => state.buffer.extend_from_slice(&chunk),
                    Some(Err(error)) => {
                        state.done = true;
                        state.buffer.clear();
                        return Some((Err(error), state));
                    }
                    None => state.done = true,
                }
            }
        },
    ))
}

fn encode(segment: &str) -> String {
    quench_serde::urlencoded::percent_encode(segment)
}

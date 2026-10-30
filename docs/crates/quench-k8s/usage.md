---
title: Usage
parent: quench-k8s
grand_parent: Crates
nav_order: 1
---

## 1. Connect

```rust
use quench_k8s::{Client, Config};

let client = Client::infer()?;                       // KUBECONFIG, ~/.kube/config, else the pod's service account
let other = Client::new(Config::infer_with_context(Some("staging"))?)?; // another kubeconfig context
# Ok::<(), quench_k8s::Error>(())
```

See [Configuration](configuration.md) for the exact order. `Client` is cheap to clone; clones share the connection pool.

## 2. List with selectors

```rust
# use quench_k8s::{ApiResource, Client, ListParams, Object};
# async fn f(client: Client) -> quench_k8s::Result<()> {
let pods = client.namespaced(&ApiResource::pod(), "forge");
let list = pods
    .list(&ListParams::default().labels("app=web,tier!=db").fields("status.phase=Running"))
    .await?;
for pod in &list.items {
    println!("{} {:?}", Object(pod).name().unwrap_or("?"), Object(pod).label("app"));
}
# Ok(()) }
```

`list` follows the server's continuation tokens until there are none. Set `ListParams::page_size` to control the page (`limit`) size.

## 3. Get, with and without a 404

```rust
# use quench_k8s::{ApiResource, Client};
# async fn f(client: Client) -> quench_k8s::Result<()> {
let api = client.namespaced(&ApiResource::config_map(), "default");
match api.get_opt("settings").await? {
    Some(cm) => println!("{:?}", cm.get("data").and_then(|d| d.get("key"))),
    None => println!("absent"),
}
if let Err(e) = api.get("missing").await {
    assert!(e.is_not_found());
}
# Ok(()) }
```

## 4. Create and patch

```rust
# use quench_k8s::*;
# use quench_serde::Value;
# async fn f(client: Client, manifest: Value) -> Result<()> {
let deployments = client.namespaced(&ApiResource::deployment(), "default");
deployments.create(&manifest, &PostParams::default()).await?;

// merge patch
let patch = Patch::Merge(quench_serde::json::from_str(r#"{"spec":{"replicas":3}}"#).unwrap());
deployments.patch("web", &patch, &PatchParams::default()).await?;

// server-side apply
deployments
    .patch("web", &Patch::Apply(manifest.clone()),
           &PatchParams { field_manager: Some("forge".into()), force: true, ..PatchParams::default() })
    .await?;
# Ok(()) }
```

`create` returns the 409 as `Error::Api` with `is_conflict()` true when the object exists. For `Patch::Apply` set `field_manager`; the client does not enforce it, the API server does.

## 5. Delete

```rust
# use quench_k8s::*;
# async fn f(jobs: Api) -> Result<()> {
jobs.delete("migrate", &DeleteParams::background()).await?; // pods follow in the background
# Ok(()) }
```

With no propagation policy and no grace period, no request body is sent.

## 6. Logs

```rust
# use quench_k8s::*;
# use quench_core::stream::StreamExt;
# async fn f(pods: Api) -> Result<()> {
let text = pods.logs("web-1", &LogParams { tail_lines: Some(100), ..LogParams::default() }).await?;

let bytes = pods.log_stream("web-1", &LogParams { follow: true, ..LogParams::default() }).await?;
let mut lines = quench_k8s::lines(bytes);
while let Some(line) = lines.next().await {
    println!("{}", line?);
}
# Ok(()) }
```

## 7. Watch and resume

```rust
# use quench_k8s::*;
# use quench_core::stream::StreamExt;
# async fn f(pods: Api) -> Result<()> {
let list = pods.list(&ListParams::default()).await?;
let mut version = list.resource_version.clone().unwrap_or_default();
loop {
    let mut events = pods.watch(&ListParams::default(), &version).await?;
    while let Some(event) = events.next().await {
        match event? {
            WatchEvent::Added(o) | WatchEvent::Modified(o) | WatchEvent::Deleted(o) => {
                if let Some(v) = Object(&o).resource_version() { version = v.to_string(); }
            }
            WatchEvent::Bookmark(o) => {
                if let Some(v) = Object(&o).resource_version() { version = v.to_string(); }
            }
            WatchEvent::Error(status) => return Err(Error::Api(status)), // e.g. 410: list again
        }
    }
    // the server closed the watch; start another from the last version
}
# }
```

## 8. Discover what the cluster serves; custom resources

```rust
# use quench_k8s::*;
# async fn f(client: Client, manifest: quench_serde::Value) -> Result<()> {
let discovery = client.discover().await?;
if let Some((resource, caps)) = discovery.find_manifest(&manifest) {
    assert!(caps.supports_operation("create"));
    client.namespaced(resource, "default").create(&manifest, &PostParams::default()).await?;
}
// without discovery: the plural is guessed from the kind
let guessed = ApiResource::from_manifest(&manifest, true);
# Ok(()) }
```

`guess_plural("Ingress")` is `"ingresses"`; irregular plurals of custom resources need discovery or `ApiResource::new`.

## 9. Anything else

```rust
# async fn f(client: quench_k8s::Client) -> quench_k8s::Result<()> {
let version = client.get_json("/version").await?;
# Ok(()) }
```

`get_json` is GET only and expects a JSON body; `/healthz` returning plain `ok` therefore fails with `Error::Decode`.

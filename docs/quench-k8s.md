# Quench K8s

`quench-k8s` (crate `quench_k8s`) is a generic Kubernetes API client that replaces `kube` and `k8s-openapi` (about 110 crates: the hyper/tower/h2 stack, generated types for every API). It sits on `quench-client`, so it shares the workspace's TLS and HTTP code. Objects are plain `quench_serde::Value`s, which is what makes it generic: any kind, custom resources included, goes through the same calls.

```rust
use quench_k8s::{ApiResource, Client, ListParams, LogParams, Object};

let client = Client::infer()?;                                   // kubeconfig, else the pod's service account
let pods = client.namespaced(&ApiResource::pod(), "forge");
for pod in pods.list(&ListParams::default().labels("app=web")).await?.items {
    println!("{}", Object(&pod).name().unwrap_or("?"));
}
let log = pods.logs("web-1", &LogParams { tail_lines: Some(100), ..LogParams::default() }).await?;
```

## Configuration

`Config::infer()` uses the first existing file of `KUBECONFIG`, then `~/.kube/config`, then the service account mounted in the pod (`KUBERNETES_SERVICE_HOST`). `from_kubeconfig(path, context)` / `from_kubeconfig_str` read a kubeconfig: the named or current context, its cluster (`server`, `certificate-authority[-data]`, `insecure-skip-tls-verify`) and user (`token`, `tokenFile`, `client-certificate[-data]` with `client-key[-data]`, `username`/`password`); relative file paths resolve against the kubeconfig's directory. A user that authenticates through an `exec` credential plugin or an `auth-provider` is `Error::Unsupported` with a message saying so. `in_cluster()` reads `ca.crt`, `token` (re-read for every request, because the token is rotated) and `namespace`. Client certificates use mutual TLS from `quench-tls`. `Config` and `Auth` never print credentials in `Debug`.

## Calls

- `client.api(&ApiResource)` (all namespaces, or cluster-scoped kinds) and `client.namespaced(&resource, ns)` give an `Api`. `ApiResource` has presets (`pod()`, `service()`, `secret()`, `config_map()`, `namespace()`, `node()`, `deployment()`, `stateful_set()`, `daemon_set()`, `job()`), `new(group, version, kind, plural, namespaced)` and `from_manifest` (plural guessed from the kind).
- `Api`: `get`, `get_opt` (`None` for 404), `list(&ListParams)` (label and field selectors; follows the server's pages), `create(&Value, &PostParams)`, `replace`, `patch(name, &Patch, &PatchParams)` (`Merge`, `StrategicMerge`, `Json`, `Apply` for server-side apply with `field_manager` and `force`), `delete(name, &DeleteParams)` (`DeleteParams::background()`, `Foreground`, `Orphan`, grace period, dry run), `logs` (text) and `log_stream` (bytes as they arrive; `quench_k8s::lines` splits them), `watch(&ListParams, resource_version)` (a stream of `WatchEvent::{Added, Modified, Deleted, Bookmark, Error}`; it ends when the server closes it, so resume from the last version).
- `client.discover()` returns the groups' preferred versions with every resource's `ApiResource` and `Capabilities` (`scope`, `supports_operation("list")`); `find_kind` and `find_manifest` look a resource up. Subresources are left out and a group the account cannot read is skipped.
- `client.get_json(path)` for anything else (`/version`, `/healthz`).
- `Object(&value)` reads `metadata` without copying: `name`, `namespace`, `uid`, `resource_version`, `label`, `annotation`, `labels()`, `annotations()`, `creation_timestamp()`, `deletion_timestamp()` as `Time`.
- Errors: `Error::Api(Status { code, reason, message })` for the server's refusals (`is_not_found()`, `is_conflict()`, `is_forbidden()`), `Transport`, `Decode`, `Config`, `Unsupported`.

## Not included

Exec and attach (they need WebSocket upgrades), port forwarding, bookmark-driven reflectors/informers, credential plugins, and typed models of the built-in kinds. A caller that wants a typed view writes a few accessors over the `Value`.

## Testing

Twenty-one tests run the client against a mock API server built on `quench-http` (paths, selectors, pagination, every patch content type, delete options, log streaming, watch events, auth headers, error mapping, discovery) and exercise the kubeconfig parser with embedded and file credentials and every failure it reports. The whole client was also checked read-only against a real k3s cluster using its kubeconfig's client certificate: discovery found 104 resources, and the namespace, pod and per-namespace counts, a pod's log (text and streamed), a 404 and an opened watch all matched `kubectl`.

[Home](../README.md)

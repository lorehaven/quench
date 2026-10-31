# quench-k8s
A generic Kubernetes API client: kubeconfig and in-cluster auth, dynamic resources, discovery, logs and watches.

## What it is
`quench-k8s` is a generic Kubernetes API client. It replaces `kube` and `k8s-openapi`: objects are plain `quench_serde::Value`s, so any kind, custom resources included, goes through the same calls to read, create, replace, patch, delete, watch, and read a pod's log.

It authenticates from a kubeconfig (bearer token, token file, client certificate, basic) or from the pod's service account. HTTP and TLS come from `quench-client`. Exec, attach, port forwarding, informers and credential plugins are not included.

## Use cases
- List and filter pods or any other kind with label and field selectors.
- Create, merge-patch or server-side-apply manifests.
- Stream pod logs and follow them.
- Watch resources and resume from a resource version.
- Work with custom resources and API discovery.

## Example
```rust
use quench_k8s::{ApiResource, Client, ListParams, Object};

async fn demo() -> Result<(), quench_k8s::Error> {
    let client = Client::infer()?;
    let pods = client.namespaced(&ApiResource::pod(), "default");
    for pod in pods.list(&ListParams::default().labels("app=web")).await?.items {
        println!("{}", Object(&pod).name().unwrap_or("?"));
    }
    Ok(())
}
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-k8s/

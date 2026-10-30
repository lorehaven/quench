---
title: Configuration
parent: quench-k8s
grand_parent: Crates
nav_order: 3
---

The crate reads these environment variables (through `quench_core::env`): `KUBECONFIG`, `HOME`, `KUBERNETES_SERVICE_HOST`, `KUBERNETES_SERVICE_PORT`. Everything else is in `Config`.

## `Config` fields

| Field | Type | Default (`Config::new`) | Meaning |
|---|---|---|---|
| `server` | `String` | the argument, trailing `/` removed | API server URL, e.g. `https://ossiriand:6443`. |
| `namespace` | `String` | `"default"` | Namespace used when none is named (`Client::default_namespace`). |
| `ca_pem` | `Option<Vec<u8>>` | `None` | Extra trusted CA certificates (PEM). |
| `insecure` | `bool` | `false` | Skip server certificate verification. Development only. |
| `auth` | `Auth` | `Auth::None` | Authentication. |
| `timeout` | `Option<Duration>` | `Some(30 s)` | Limit for a request to produce its response headers. A streaming response (`log_stream`, `watch`) is not limited after the headers arrive (see the note on `send_streaming` in `quench-client`). |

## `Config::infer()` resolution

1. `KUBECONFIG`: the value is split like `PATH`; the first entry that is an existing file is used (the other entries are not merged).
2. Else `$HOME/.kube/config`, if it is a file.
3. If a kubeconfig file was found: `Config::from_kubeconfig(path, context)`. Its errors are returned as they are (there is no fallback to in-cluster once a file was found).
4. Else `Config::in_cluster()`; on failure `Error::Config("no kubeconfig (KUBECONFIG or ~/.kube/config) and not running inside a cluster")`.

`infer_with_context(Some(name))` is the same with a kubeconfig context other than `current-context`; the argument is ignored for in-cluster.

## `Config::in_cluster()`

* Host from `KUBERNETES_SERVICE_HOST` (`Error::Config("KUBERNETES_SERVICE_HOST is not set")` if unset); port from `KUBERNETES_SERVICE_PORT`, default `"443"`.
* `in_cluster_at(directory, host, port)` (the directory is `/var/run/secrets/kubernetes.io/serviceaccount` for `in_cluster`): server `https://<host>:<port>`, where a host containing `:` and not starting with `[` is wrapped in brackets (IPv6). `ca_pem` from `<directory>/ca.crt` (unreadable: `Error::Config("cannot read the service account CA: ...")`), `auth = Auth::BearerFile(<directory>/token)` (re-read for each request, because the token is rotated), `namespace` from `<directory>/namespace` trimmed if readable (otherwise `default`).

## Kubeconfig parsing

`from_kubeconfig(path, context)` reads the file (`Error::Config("cannot read <path>: <e>")`) and calls `from_kubeconfig_str(text, context, path.parent())`. `from_kubeconfig_str` parses YAML (`Error::Config("not a kubeconfig: <e>")`) and:

1. Context name: the argument, else `current-context` (else `Error::Config("the kubeconfig names no current-context")`).
2. Finds `contexts[name=<ctx>].context` (else `Error::Config("the kubeconfig has no context `<ctx>`")`); its `cluster` is required (`"the context names no cluster"`); finds `clusters[name=<cluster>].cluster` (`"the kubeconfig has no cluster `<name>`"`).
3. `server` of the cluster is required (`"cluster `<name>` has no server"`). `namespace` from the context if present. `insecure` = `insecure-skip-tls-verify` is `true`.
4. `ca_pem`: `certificate-authority-data` (base64, trimmed; invalid gives `Error::Config("`certificate-authority-data` is not valid base64")`), else the file named by `certificate-authority`, else none.
5. If the context names a `user`, finds `users[name=<user>].user` (`"the kubeconfig has no user `<name>`"`) and chooses the first match of:
   * `exec` or `auth-provider` present: `Error::Unsupported("user `<name>` authenticates through a credential plugin; use a token or a client certificate")`.
   * `token`: `Auth::Bearer`.
   * `tokenFile`: `Auth::BearerFile` (relative path resolved against `base`).
   * `client-certificate[-data]` and `client-key[-data]`: both present gives `Auth::ClientCertificate`; only one gives `Error::Config("user `<name>` has a client certificate without its key, or the reverse")`.
   * `username` and `password` both present: `Auth::Basic`.
   * Otherwise `Auth::None`.
   A context without a `user` gives `Auth::None`.

`<key>-data` takes priority over the `<key>` file. Relative file paths are joined to `base` (the kubeconfig's directory); absolute paths and a `None` base are used as they are. A file that cannot be read: `Error::Config("cannot read <path>: <e>")`.

Not read: `proxy-url`, `tls-server-name`, `extensions`, multiple-file merging.

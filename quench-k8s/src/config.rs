//! Where the cluster is and how to prove who we are: a kubeconfig, or the service account of the pod we run in.

use std::path::{Path, PathBuf};
use std::time::Duration;

use quench_core::base64;
use quench_serde::{Value, yaml};

use crate::error::{Error, Result};

const SERVICE_ACCOUNT: &str = "/var/run/secrets/kubernetes.io/serviceaccount";

/// How requests are authenticated.
#[derive(Clone, PartialEq, Eq)]
pub enum Auth {
    /// No credentials.
    None,
    /// A bearer token.
    Bearer(String),
    /// A bearer token read from a file for every request (a service account token is rotated).
    BearerFile(PathBuf),
    /// A client certificate and private key, both PEM.
    ClientCertificate { certificate: Vec<u8>, key: Vec<u8> },
    /// HTTP basic authentication.
    Basic { username: String, password: String },
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Credentials never appear in logs.
        f.write_str(match self {
            Self::None => "Auth::None",
            Self::Bearer(_) => "Auth::Bearer(..)",
            Self::BearerFile(_) => "Auth::BearerFile(..)",
            Self::ClientCertificate { .. } => "Auth::ClientCertificate(..)",
            Self::Basic { .. } => "Auth::Basic(..)",
        })
    }
}

/// Everything needed to open a [`Client`](crate::Client).
#[derive(Debug, Clone)]
pub struct Config {
    /// The API server, e.g. `https://ossiriand:6443`.
    pub server: String,
    /// The namespace used when none is named (`default` unless the kubeconfig or service account says).
    pub namespace: String,
    /// Extra trusted CA certificates, PEM.
    pub ca_pem: Option<Vec<u8>>,
    /// Do not verify the server's certificate (development only).
    pub insecure: bool,
    pub auth: Auth,
    /// Limit for a request to produce its response headers.
    pub timeout: Option<Duration>,
}

impl Config {
    /// A configuration for `server` with no credentials, the `default` namespace and the default trust roots.
    #[must_use]
    pub fn new(server: &str) -> Self {
        Self {
            server: server.trim_end_matches('/').to_string(),
            namespace: "default".to_string(),
            ca_pem: None,
            insecure: false,
            auth: Auth::None,
            timeout: Some(Duration::from_secs(30)),
        }
    }

    /// The configuration this process should use: the kubeconfig named by `KUBECONFIG` (first path of the
    /// list) or `~/.kube/config` if there is one, else the pod's service account.
    ///
    /// # Errors
    ///
    /// [`Error::Config`] when neither is available or the one found is unusable.
    pub fn infer() -> Result<Self> {
        Self::infer_with_context(None)
    }

    /// [`infer`](Self::infer) with a kubeconfig context other than its `current-context`; the context only
    /// matters when a kubeconfig is found.
    ///
    /// # Errors
    ///
    /// As [`infer`](Self::infer), plus [`Error::Config`] for a context the kubeconfig does not have.
    pub fn infer_with_context(context: Option<&str>) -> Result<Self> {
        if let Some(path) = default_kubeconfig() {
            return Self::from_kubeconfig(&path, context);
        }
        Self::in_cluster().map_err(|_| {
            Error::Config(
                "no kubeconfig (KUBECONFIG or ~/.kube/config) and not running inside a cluster"
                    .to_string(),
            )
        })
    }

    /// From the service account mounted in the pod and `KUBERNETES_SERVICE_HOST` / `_PORT`.
    ///
    /// # Errors
    ///
    /// [`Error::Config`] outside a cluster, or when the mounted files are unreadable.
    pub fn in_cluster() -> Result<Self> {
        let host = quench_core::env::var("KUBERNETES_SERVICE_HOST")
            .ok_or_else(|| Error::Config("KUBERNETES_SERVICE_HOST is not set".to_string()))?;
        let port =
            quench_core::env::var("KUBERNETES_SERVICE_PORT").unwrap_or_else(|| "443".to_string());
        Self::in_cluster_at(Path::new(SERVICE_ACCOUNT), &host, &port)
    }

    /// [`in_cluster`](Self::in_cluster) with the service-account directory and the service address given.
    ///
    /// # Errors
    ///
    /// [`Error::Config`] when the CA certificate is unreadable.
    pub fn in_cluster_at(directory: &Path, host: &str, port: &str) -> Result<Self> {
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        let mut config = Self::new(&format!("https://{host}:{port}"));
        config.ca_pem = Some(
            std::fs::read(directory.join("ca.crt"))
                .map_err(|e| Error::Config(format!("cannot read the service account CA: {e}")))?,
        );
        config.auth = Auth::BearerFile(directory.join("token"));
        if let Ok(namespace) = std::fs::read_to_string(directory.join("namespace")) {
            config.namespace = namespace.trim().to_string();
        }
        Ok(config)
    }

    /// From a kubeconfig file, using `context` or else its `current-context`.
    ///
    /// # Errors
    ///
    /// [`Error::Config`] when the file is unreadable or incomplete, [`Error::Unsupported`] when the user
    /// authenticates through a credential plugin.
    pub fn from_kubeconfig(path: &Path, context: Option<&str>) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("cannot read {}: {e}", path.display())))?;
        Self::from_kubeconfig_str(&text, context, path.parent())
    }

    /// From kubeconfig text. Relative paths inside it are read against `base`.
    ///
    /// # Errors
    ///
    /// As [`from_kubeconfig`](Self::from_kubeconfig).
    pub fn from_kubeconfig_str(
        text: &str,
        context: Option<&str>,
        base: Option<&Path>,
    ) -> Result<Self> {
        let doc: Value =
            yaml::from_str(text).map_err(|e| Error::Config(format!("not a kubeconfig: {e}")))?;
        let bad = |m: String| Error::Config(m);
        let context_name = context
            .map(str::to_string)
            .or_else(|| {
                doc.get("current-context")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .ok_or_else(|| bad("the kubeconfig names no current-context".to_string()))?;
        let named = |list: &str, key: &str, name: &str| -> Result<Value> {
            doc.get(list)
                .and_then(Value::as_array)
                .and_then(|items| {
                    items
                        .iter()
                        .find(|i| i.get("name").and_then(Value::as_str) == Some(name))
                })
                .and_then(|i| i.get(key))
                .cloned()
                .ok_or_else(|| bad(format!("the kubeconfig has no {key} `{name}`")))
        };
        let ctx = named("contexts", "context", &context_name)?;
        let field =
            |value: &Value, key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        let cluster_name = field(&ctx, "cluster")
            .ok_or_else(|| bad("the context names no cluster".to_string()))?;
        let cluster = named("clusters", "cluster", &cluster_name)?;

        let server = field(&cluster, "server")
            .ok_or_else(|| bad(format!("cluster `{cluster_name}` has no server")))?;
        let mut config = Self::new(&server);
        if let Some(namespace) = field(&ctx, "namespace") {
            config.namespace = namespace;
        }
        config.insecure = cluster
            .get("insecure-skip-tls-verify")
            .and_then(Value::as_bool)
            == Some(true);
        config.ca_pem = embedded_or_file(&cluster, "certificate-authority", base)?;

        if let Some(user_name) = field(&ctx, "user") {
            let user = named("users", "user", &user_name)?;
            config.auth = user_auth(&user, base, &user_name)?;
        }
        Ok(config)
    }
}

fn user_auth(user: &Value, base: Option<&Path>, name: &str) -> Result<Auth> {
    let text = |key: &str| user.get(key).and_then(Value::as_str).map(str::to_string);
    if user.get("exec").is_some() || user.get("auth-provider").is_some() {
        return Err(Error::Unsupported(format!(
            "user `{name}` authenticates through a credential plugin; use a token or a client certificate"
        )));
    }
    if let Some(token) = text("token") {
        return Ok(Auth::Bearer(token));
    }
    if let Some(file) = text("tokenFile") {
        return Ok(Auth::BearerFile(resolve(base, &file)));
    }
    let certificate = embedded_or_file(user, "client-certificate", base)?;
    let key = embedded_or_file(user, "client-key", base)?;
    match (certificate, key) {
        (Some(certificate), Some(key)) => return Ok(Auth::ClientCertificate { certificate, key }),
        (None, None) => {}
        _ => {
            return Err(Error::Config(format!(
                "user `{name}` has a client certificate without its key, or the reverse"
            )));
        }
    }
    if let (Some(username), Some(password)) = (text("username"), text("password")) {
        return Ok(Auth::Basic { username, password });
    }
    Ok(Auth::None)
}

/// `<key>-data` (base64) if present, else the file `<key>` names, else nothing.
fn embedded_or_file(section: &Value, key: &str, base: Option<&Path>) -> Result<Option<Vec<u8>>> {
    if let Some(data) = section.get(format!("{key}-data")).and_then(Value::as_str) {
        return base64::STANDARD
            .decode(data.trim())
            .map(Some)
            .map_err(|_| Error::Config(format!("`{key}-data` is not valid base64")));
    }
    if let Some(file) = section.get(key).and_then(Value::as_str) {
        let path = resolve(base, file);
        return std::fs::read(&path)
            .map(Some)
            .map_err(|e| Error::Config(format!("cannot read {}: {e}", path.display())));
    }
    Ok(None)
}

fn resolve(base: Option<&Path>, file: &str) -> PathBuf {
    let path = PathBuf::from(file);
    match base {
        Some(base) if path.is_relative() => base.join(path),
        _ => path,
    }
}

/// The first existing kubeconfig of `KUBECONFIG`, else `~/.kube/config`.
fn default_kubeconfig() -> Option<PathBuf> {
    if let Some(list) = quench_core::env::var_os("KUBECONFIG") {
        let first = std::env::split_paths(&list).find(|p| p.is_file());
        if first.is_some() {
            return first;
        }
    }
    let home = quench_core::env::var_os("HOME")?;
    let path = Path::new(&home).join(".kube/config");
    path.is_file().then_some(path)
}

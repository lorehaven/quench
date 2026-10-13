//! The small HTTPS client the rest of the workspace builds on: connection
//! pooling, HTTP/1.1 + HTTP/2, JSON / form bodies, basic / bearer auth, a
//! per-request timeout, redirect following, and an opt-out of certificate
//! verification for development. Roots come from the operating system's
//! trust store (so a private CA installed there works).

use bytes::Bytes;
use http::header::{
    AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HeaderName, HeaderValue, LOCATION,
};
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::{BodyExt, Full};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::Client as PoolClient;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, ring};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use rustls_platform_verifier::BuilderVerifierExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::Duration;

const MAX_REDIRECTS: usize = 10;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid request: {0}")]
    Request(String),
    #[error("request failed: {0}")]
    Transport(String),
    #[error("request timed out")]
    Timeout,
    #[error("could not decode response body: {0}")]
    Decode(String),
    #[error("could not build the client: {0}")]
    Build(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
pub struct Client {
    pool: PoolClient<HttpsConnector<HttpConnector>, Full<Bytes>>,
    timeout: Option<Duration>,
}

pub struct ClientBuilder {
    timeout: Option<Duration>,
    accept_invalid_certs: bool,
}

impl Client {
    pub fn new() -> Self {
        Self::builder()
            .build()
            .expect("default client configuration is valid")
    }

    pub fn builder() -> ClientBuilder {
        ClientBuilder {
            timeout: None,
            accept_invalid_certs: false,
        }
    }

    pub fn get(&self, url: impl AsRef<str>) -> RequestBuilder {
        self.request(Method::GET, url)
    }

    pub fn post(&self, url: impl AsRef<str>) -> RequestBuilder {
        self.request(Method::POST, url)
    }

    pub fn put(&self, url: impl AsRef<str>) -> RequestBuilder {
        self.request(Method::PUT, url)
    }

    pub fn delete(&self, url: impl AsRef<str>) -> RequestBuilder {
        self.request(Method::DELETE, url)
    }

    pub fn request(&self, method: Method, url: impl AsRef<str>) -> RequestBuilder {
        RequestBuilder {
            client: self.clone(),
            method,
            url: url
                .as_ref()
                .parse::<Uri>()
                .map_err(|e| Error::Request(format!("{}: {e}", url.as_ref()))),
            headers: HeaderMap::new(),
            body: Bytes::new(),
            timeout: self.timeout,
            error: None,
        }
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientBuilder {
    /// Applies to every request from the built client unless overridden.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Skips certificate verification entirely - development only.
    pub fn danger_accept_invalid_certs(mut self, accept: bool) -> Self {
        self.accept_invalid_certs = accept;
        self
    }

    pub fn build(self) -> Result<Client> {
        let provider = Arc::new(ring::default_provider());
        let builder = ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(|e| Error::Build(e.to_string()))?;

        let tls = if self.accept_invalid_certs {
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AcceptAnyCert(provider)))
                .with_no_client_auth()
        } else {
            builder
                .with_platform_verifier()
                .map_err(|e| Error::Build(e.to_string()))?
                .with_no_client_auth()
        };

        let https = HttpsConnectorBuilder::new()
            .with_tls_config(tls)
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();

        Ok(Client {
            pool: PoolClient::builder(TokioExecutor::new()).build(https),
            timeout: self.timeout,
        })
    }
}

#[derive(Debug)]
struct AcceptAnyCert(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

pub struct RequestBuilder {
    client: Client,
    method: Method,
    url: Result<Uri>,
    headers: HeaderMap,
    body: Bytes,
    timeout: Option<Duration>,
    error: Option<Error>,
}

impl RequestBuilder {
    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        match (
            HeaderName::from_bytes(name.as_ref().as_bytes()),
            HeaderValue::from_str(value.as_ref()),
        ) {
            (Ok(name), Ok(value)) => {
                self.headers.append(name, value);
            }
            _ => {
                self.error.get_or_insert(Error::Request(format!(
                    "invalid header `{}`",
                    name.as_ref()
                )));
            }
        }
        self
    }

    pub fn basic_auth(self, username: impl AsRef<str>, password: Option<&str>) -> Self {
        let credentials = format!("{}:{}", username.as_ref(), password.unwrap_or(""));
        self.header(
            AUTHORIZATION.as_str(),
            format!("Basic {}", base64_encode(credentials.as_bytes())),
        )
    }

    pub fn bearer_auth(self, token: impl AsRef<str>) -> Self {
        self.header(AUTHORIZATION.as_str(), format!("Bearer {}", token.as_ref()))
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn json<T: Serialize + ?Sized>(mut self, body: &T) -> Self {
        match serde_json::to_vec(body) {
            Ok(bytes) => {
                self.body = bytes.into();
                self.header(CONTENT_TYPE.as_str(), "application/json")
            }
            Err(e) => {
                self.error.get_or_insert(Error::Request(e.to_string()));
                self
            }
        }
    }

    pub fn form<T: Serialize + ?Sized>(mut self, body: &T) -> Self {
        match serde_urlencoded::to_string(body) {
            Ok(encoded) => {
                self.body = encoded.into();
                self.header(CONTENT_TYPE.as_str(), "application/x-www-form-urlencoded")
            }
            Err(e) => {
                self.error.get_or_insert(Error::Request(e.to_string()));
                self
            }
        }
    }

    pub async fn send(self) -> Result<Response> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let url = self.url?;
        let exchange = exchange(&self.client, self.method, url, self.headers, self.body);
        match self.timeout {
            Some(limit) => tokio::time::timeout(limit, exchange)
                .await
                .map_err(|_| Error::Timeout)?,
            None => exchange.await,
        }
    }
}

async fn exchange(
    client: &Client,
    mut method: Method,
    mut url: Uri,
    headers: HeaderMap,
    mut body: Bytes,
) -> Result<Response> {
    let mut headers = headers;
    for _ in 0..=MAX_REDIRECTS {
        if url.scheme().is_none() || url.authority().is_none() {
            return Err(Error::Request(format!("`{url}` is not an absolute URL")));
        }

        let mut request = http::Request::builder()
            .method(method.clone())
            .uri(url.clone());
        *request.headers_mut().expect("builder has no error yet") = headers.clone();
        if !body.is_empty() || matches!(method, Method::POST | Method::PUT | Method::PATCH) {
            request
                .headers_mut()
                .unwrap()
                .insert(CONTENT_LENGTH, HeaderValue::from(body.len()));
        }
        let request = request
            .body(Full::new(body.clone()))
            .map_err(|e| Error::Request(e.to_string()))?;

        let response = client
            .pool
            .request(request)
            .await
            .map_err(|e| Error::Transport(error_chain(&e)))?;

        let status = response.status();
        let redirect = status.is_redirection()
            && status != StatusCode::NOT_MODIFIED
            && response.headers().contains_key(LOCATION);
        if redirect {
            let location = response.headers()[LOCATION]
                .to_str()
                .map_err(|e| Error::Transport(e.to_string()))?
                .to_string();
            url = resolve(&url, &location)?;
            // 301/302/303 turn a write into a GET; 307/308 replay it as is.
            if matches!(
                status,
                StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND | StatusCode::SEE_OTHER
            ) && method != Method::GET
                && method != Method::HEAD
            {
                method = Method::GET;
                body = Bytes::new();
                headers.remove(CONTENT_TYPE);
            }
            continue;
        }

        let (parts, incoming) = response.into_parts();
        let bytes = incoming
            .collect()
            .await
            .map_err(|e| Error::Transport(error_chain(&e)))?
            .to_bytes();
        return Ok(Response {
            status: parts.status,
            headers: parts.headers,
            body: bytes,
        });
    }
    Err(Error::Transport("too many redirects".to_string()))
}

fn resolve(base: &Uri, location: &str) -> Result<Uri> {
    let target = if location.contains("://") {
        location.to_string()
    } else if location.starts_with('/') {
        format!(
            "{}://{}{location}",
            base.scheme_str().unwrap_or("http"),
            base.authority().map(|a| a.as_str()).unwrap_or("")
        )
    } else {
        let dir = base.path().rsplit_once('/').map_or("", |(dir, _)| dir);
        format!(
            "{}://{}{dir}/{location}",
            base.scheme_str().unwrap_or("http"),
            base.authority().map(|a| a.as_str()).unwrap_or("")
        )
    };
    target
        .parse()
        .map_err(|e| Error::Transport(format!("bad redirect target `{target}`: {e}")))
}

fn error_chain(error: &dyn std::error::Error) -> String {
    let mut out = error.to_string();
    let mut source = error.source();
    while let Some(next) = source {
        out.push_str(": ");
        out.push_str(&next.to_string());
        source = next.source();
    }
    out
}

pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl Response {
    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub async fn bytes(self) -> Result<Bytes> {
        Ok(self.body)
    }

    pub async fn text(self) -> Result<String> {
        String::from_utf8(self.body.to_vec())
            .or_else(|e| Ok(String::from_utf8_lossy(e.as_bytes()).into_owned()))
    }

    pub async fn json<T: DeserializeOwned>(self) -> Result<T> {
        serde_json::from_slice(&self.body).map_err(|e| Error::Decode(e.to_string()))
    }
}

fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_rfc_vectors() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_encode(input.as_bytes()), expected);
        }
    }

    #[test]
    fn redirect_targets_resolve_against_the_current_url() {
        let base: Uri = "https://h:8443/a/b?x=1".parse().unwrap();
        assert_eq!(
            resolve(&base, "/c").unwrap().to_string(),
            "https://h:8443/c"
        );
        assert_eq!(
            resolve(&base, "d").unwrap().to_string(),
            "https://h:8443/a/d"
        );
        assert_eq!(
            resolve(&base, "http://o/e").unwrap().to_string(),
            "http://o/e"
        );
    }
}

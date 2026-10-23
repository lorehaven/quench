use quench_core::{Context, Error, Result, err};
use quench_serde::{DeserializeOwned, Serialize};
use transport::{Client, Response};

#[cfg(feature = "blocking")]
pub mod blocking;
pub mod transport;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("Request failed: {0}")]
    RequestFailed(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Connection error: {0}")]
    ConnectionError(String),
}

pub type ClientResult<T> = std::result::Result<T, ClientError>;

#[derive(Debug, Clone)]
pub struct HttpClient {
    http: Client,
    base_url: String,
}

impl HttpClient {
    pub fn new(base_url: &str) -> Result<Self> {
        Self::builder(base_url).build()
    }

    #[must_use]
    pub fn builder(base_url: &str) -> HttpClientBuilder {
        HttpClientBuilder {
            base_url: base_url.to_string(),
            tls_verify: true,
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let res = self
            .http
            .get(&url)
            .send()
            .await
            .context("Failed to send GET request")?;

        self.handle_response::<T>(res).await
    }

    pub async fn post<S: Serialize + Sync, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &S,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let res = self
            .http
            .post(&url)
            .json(body)
            .send()
            .await
            .context("Failed to send POST request")?;

        self.handle_response::<T>(res).await
    }

    pub async fn put<S: Serialize + Sync, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &S,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let res = self
            .http
            .put(&url)
            .json(body)
            .send()
            .await
            .context("Failed to send PUT request")?;

        self.handle_response::<T>(res).await
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let res = self
            .http
            .delete(&url)
            .send()
            .await
            .context("Failed to send DELETE request")?;

        self.handle_response::<T>(res).await
    }

    async fn handle_response<T: DeserializeOwned>(&self, res: Response) -> Result<T> {
        let status = res.status();

        if !status.is_success() {
            let body = res
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(err!("HTTP {status}: {body}"));
        }

        res.json::<T>().await.context("Failed to parse response")
    }
}

#[derive(Debug)]
pub struct HttpClientBuilder {
    base_url: String,
    tls_verify: bool,
}

impl HttpClientBuilder {
    #[must_use]
    pub const fn tls_verify(mut self, verify: bool) -> Self {
        self.tls_verify = verify;
        self
    }

    pub fn build(self) -> Result<HttpClient> {
        let http = Client::builder()
            .danger_accept_invalid_certs(!self.tls_verify)
            .build()?;

        Ok(HttpClient {
            http,
            base_url: self.base_url,
        })
    }
}

/// HTTP client with Basic authentication
#[derive(Clone)]
pub struct BasicAuthClient {
    client: HttpClient,
    username: String,
    password: String,
}

impl std::fmt::Debug for BasicAuthClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BasicAuthClient")
            .field("client", &self.client)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl BasicAuthClient {
    pub fn new(base_url: &str, username: &str, password: &str) -> Result<Self> {
        let client = HttpClient::new(base_url)?;
        Ok(Self {
            client,
            username: username.to_string(),
            password: password.to_string(),
        })
    }

    #[must_use]
    pub fn builder(base_url: &str) -> BasicAuthClientBuilder {
        BasicAuthClientBuilder {
            base_url: base_url.to_string(),
            tls_verify: true,
            username: String::new(),
            password: String::new(),
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.client.base_url, path);
        quench_log::debug!(
            "BasicAuthClient::get - Sending request to {} with username: {}",
            url,
            self.username
        );
        let res = self
            .client
            .http
            .get(&url)
            .basic_auth(&self.username, Some(self.password.as_str()))
            .send()
            .await
            .context("Failed to send GET request")?;

        quench_log::debug!("BasicAuthClient::get - Response status: {}", res.status());
        self.client.handle_response::<T>(res).await
    }

    pub async fn post<S: Serialize + Sync, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &S,
    ) -> Result<T> {
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .client
            .http
            .post(&url)
            .basic_auth(&self.username, Some(self.password.as_str()))
            .json(body)
            .send()
            .await
            .context("Failed to send POST request")?;

        self.client.handle_response::<T>(res).await
    }

    pub async fn put<S: Serialize + Sync, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &S,
    ) -> Result<T> {
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .client
            .http
            .put(&url)
            .basic_auth(&self.username, Some(self.password.as_str()))
            .json(body)
            .send()
            .await
            .context("Failed to send PUT request")?;

        self.client.handle_response::<T>(res).await
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .client
            .http
            .delete(&url)
            .basic_auth(&self.username, Some(self.password.as_str()))
            .send()
            .await
            .context("Failed to send DELETE request")?;

        self.client.handle_response::<T>(res).await
    }

    /// Send a DELETE request and treat any 2xx status as success, discarding
    /// the response body. Use for endpoints that return HTML or empty bodies
    /// that cannot be deserialized into a typed response.
    pub async fn delete_expect_success(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .client
            .http
            .delete(&url)
            .basic_auth(&self.username, Some(self.password.as_str()))
            .send()
            .await
            .context("Failed to send DELETE request")?;

        let status = res.status();
        if !status.is_success() {
            let body = res
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(err!("HTTP {status}: {body}"));
        }
        Ok(())
    }
}

pub struct BasicAuthClientBuilder {
    base_url: String,
    tls_verify: bool,
    username: String,
    password: String,
}

impl std::fmt::Debug for BasicAuthClientBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BasicAuthClientBuilder")
            .field("base_url", &self.base_url)
            .field("tls_verify", &self.tls_verify)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl BasicAuthClientBuilder {
    #[must_use]
    pub fn username(mut self, username: &str) -> Self {
        self.username = username.to_string();
        self
    }

    #[must_use]
    pub fn password(mut self, password: &str) -> Self {
        self.password = password.to_string();
        self
    }

    #[must_use]
    pub const fn tls_verify(mut self, verify: bool) -> Self {
        self.tls_verify = verify;
        self
    }

    pub fn build(self) -> Result<BasicAuthClient> {
        let http = Client::builder()
            .danger_accept_invalid_certs(!self.tls_verify)
            .build()?;

        Ok(BasicAuthClient {
            client: HttpClient {
                http,
                base_url: self.base_url,
            },
            username: self.username,
            password: self.password,
        })
    }
}

/// Bearer token authentication client
#[derive(Clone)]
pub struct BearerAuthClient {
    client: HttpClient,
    token: String,
}

impl std::fmt::Debug for BearerAuthClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BearerAuthClient")
            .field("client", &self.client)
            .field("token", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl BearerAuthClient {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        Self::with_tls_verify(base_url, token, true)
    }

    pub fn with_tls_verify(base_url: &str, token: &str, tls_verify: bool) -> Result<Self> {
        let client = HttpClient::builder(base_url)
            .tls_verify(tls_verify)
            .build()?;
        Ok(Self {
            client,
            token: token.to_string(),
        })
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .client
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .context("Failed to send GET request")?;

        self.client.handle_response::<T>(res).await
    }

    pub async fn post<S: Serialize + Sync, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &S,
    ) -> Result<T> {
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .client
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .context("Failed to send POST request")?;

        self.client.handle_response::<T>(res).await
    }
}

/// HTTP client authenticating via the `client_credentials` grant against
/// gatehouse - what a machine identity (sage calling switchboard) uses
/// instead of the HTTP Basic it used to send.
///
/// Exchanges once, caches the
/// access token, and re-exchanges shortly before it expires; the target
/// service never sees the client secret, only the bearer token gatehouse
/// issued for it.
#[derive(Clone)]
pub struct ClientCredentialsClient {
    client: HttpClient,
    http: Client,
    token_url: String,
    client_id: String,
    client_secret: String,
    cached: std::sync::Arc<tokio::sync::Mutex<Option<CachedToken>>>,
}

impl std::fmt::Debug for ClientCredentialsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientCredentialsClient")
            .field("client", &self.client)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct CachedToken {
    access_token: String,
    expires_at: std::time::Instant,
}

impl std::fmt::Debug for CachedToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedToken")
            .field("access_token", &"<redacted>")
            .finish_non_exhaustive()
    }
}

#[derive(quench_serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
}

impl ClientCredentialsClient {
    #[must_use]
    pub fn builder(base_url: &str) -> ClientCredentialsClientBuilder {
        ClientCredentialsClientBuilder {
            base_url: base_url.to_string(),
            token_url: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            tls_verify: true,
        }
    }

    /// The cached token if it is still fresh (with a 30s safety margin),
    /// otherwise a fresh `client_credentials` exchange.
    async fn access_token(&self) -> Result<String> {
        {
            let guard = self.cached.lock().await;
            if let Some(cached) = guard.as_ref()
                && cached.expires_at > std::time::Instant::now()
            {
                return Ok(cached.access_token.clone());
            }
        }
        self.refresh_token().await
    }

    async fn refresh_token(&self) -> Result<String> {
        let form = [
            ("grant_type", "client_credentials"),
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
        ];
        let response = self
            .http
            .post(&self.token_url)
            .form(&form)
            .send()
            .await
            .context("client_credentials token request failed")?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(err!("client_credentials grant was refused: {body}"));
        }

        let tokens: TokenResponse = response
            .json()
            .await
            .context("failed to parse the client_credentials token response")?;

        let ttl = tokens.expires_in.max(30).cast_unsigned() - 30;
        *self.cached.lock().await = Some(CachedToken {
            access_token: tokens.access_token.clone(),
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(ttl),
        });
        Ok(tokens.access_token)
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let token = self.access_token().await?;
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .context("Failed to send GET request")?;
        self.client.handle_response::<T>(res).await
    }

    pub async fn post<S: Serialize + Sync, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &S,
    ) -> Result<T> {
        let token = self.access_token().await?;
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .http
            .post(&url)
            .bearer_auth(&token)
            .json(body)
            .send()
            .await
            .context("Failed to send POST request")?;
        self.client.handle_response::<T>(res).await
    }

    /// Send a DELETE request and treat any 2xx status as success, discarding
    /// the response body - matching `BasicAuthClient::delete_expect_success`.
    pub async fn delete_expect_success(&self, path: &str) -> Result<()> {
        let token = self.access_token().await?;
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .http
            .delete(&url)
            .bearer_auth(&token)
            .send()
            .await
            .context("Failed to send DELETE request")?;

        let status = res.status();
        if !status.is_success() {
            let body = res
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(err!("HTTP {status}: {body}"));
        }
        Ok(())
    }
}

pub struct ClientCredentialsClientBuilder {
    base_url: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    tls_verify: bool,
}

impl std::fmt::Debug for ClientCredentialsClientBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientCredentialsClientBuilder")
            .field("base_url", &self.base_url)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field("tls_verify", &self.tls_verify)
            .field("client_secret", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl ClientCredentialsClientBuilder {
    /// Gatehouse's token endpoint, e.g. `https://localhost:5443/gatehouse/api/v1/token`.
    #[must_use]
    pub fn token_url(mut self, url: &str) -> Self {
        self.token_url = url.to_string();
        self
    }

    #[must_use]
    pub fn client_id(mut self, id: &str) -> Self {
        self.client_id = id.to_string();
        self
    }

    #[must_use]
    pub fn client_secret(mut self, secret: &str) -> Self {
        self.client_secret = secret.to_string();
        self
    }

    #[must_use]
    pub const fn tls_verify(mut self, verify: bool) -> Self {
        self.tls_verify = verify;
        self
    }

    pub fn build(self) -> Result<ClientCredentialsClient> {
        let http = Client::builder()
            .danger_accept_invalid_certs(!self.tls_verify)
            .build()?;
        Ok(ClientCredentialsClient {
            client: HttpClient {
                http: http.clone(),
                base_url: self.base_url,
            },
            http,
            token_url: self.token_url,
            client_id: self.client_id,
            client_secret: self.client_secret,
            cached: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        })
    }
}

pub mod prelude {
    pub use crate::{BasicAuthClient, BearerAuthClient, ClientCredentialsClient, HttpClient};
}

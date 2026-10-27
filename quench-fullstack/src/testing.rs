//! A gatehouse-compatible token service for development and tests.
//!
//! It answers the same three calls gatehouse does (`/api/v1/auth/login`, `/refresh`, `/logout`)
//! with the same shapes, issues real signed access tokens that a [`JwtConfig`] verifies, and
//! rotates refresh tokens: using one twice is refused, as the real service does. It is not a user
//! database; the accounts are the ones you give it. Use it to try the sign-in flows without a
//! gatehouse, never in front of real users.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::realm;
use quench_core::async_trait;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::{Endpoint, Request, Response};
use quench_serde::{Deserialize, Serialize, json};

#[derive(Deserialize)]
struct Login {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct Refresh {
    refresh_token: String,
}

#[derive(Serialize)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    token_type: String,
    expires_in: i64,
}

#[derive(Serialize)]
struct Refusal {
    error: &'static str,
}

struct Inner {
    jwt: JwtConfig,
    users: Vec<(String, String)>,
    /// Live refresh tokens: token -> (user, session id).
    refresh: Mutex<HashMap<String, (String, String)>>,
    access_ttl: AtomicI64,
    logins: AtomicUsize,
    refreshes: AtomicUsize,
    sessions: AtomicUsize,
}

/// The stub. Clone it freely: clones share their state.
#[derive(Clone)]
pub struct StubGatehouse {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for StubGatehouse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StubGatehouse")
            .field("users", &self.inner.users.len())
            .finish_non_exhaustive()
    }
}

fn refusal(status: StatusCode, error: &'static str) -> Response {
    Response::json(status, &Refusal { error }).unwrap_or_else(|_| Response::new(status))
}

fn clearing_cookie(name: &str) -> String {
    format!("{name}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax")
}

impl StubGatehouse {
    /// A stub that knows these `(username, password)` accounts.
    #[must_use]
    pub fn new(users: &[(&str, &str)]) -> Self {
        Self {
            inner: Arc::new(Inner {
                jwt: JwtConfig::for_tests_with_signing(),
                users: users
                    .iter()
                    .map(|(u, p)| ((*u).to_string(), (*p).to_string()))
                    .collect(),
                refresh: Mutex::new(HashMap::new()),
                access_ttl: AtomicI64::new(900),
                logins: AtomicUsize::new(0),
                refreshes: AtomicUsize::new(0),
                sessions: AtomicUsize::new(0),
            }),
        }
    }

    /// The configuration that verifies the tokens this stub issues: give it to the service under
    /// test (`ContainerBuilder::provide`).
    #[must_use]
    pub fn jwt(&self) -> JwtConfig {
        self.inner.jwt.clone()
    }

    /// How long newly issued access tokens live, in seconds. Negative gives tokens that are
    /// already expired, to try the refresh path (token validation allows 60 seconds of clock skew, so
    /// use -120 or less).
    pub fn set_access_ttl(&self, secs: i64) {
        self.inner.access_ttl.store(secs, Ordering::SeqCst);
    }

    /// Ends every session: all refresh tokens stop working.
    pub fn revoke_all(&self) {
        self.inner
            .refresh
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    #[must_use]
    pub fn login_count(&self) -> usize {
        self.inner.logins.load(Ordering::SeqCst)
    }

    #[must_use]
    pub fn refresh_count(&self) -> usize {
        self.inner.refreshes.load(Ordering::SeqCst)
    }

    async fn issue(&self, user: &str, session: &str) -> Option<Tokens> {
        let mut jwt = self.inner.jwt.clone();
        jwt.access_token_ttl_secs = self.inner.access_ttl.load(Ordering::SeqCst);
        let access = jwt
            .issue_access_token(
                user.to_string(),
                "user".to_string(),
                Some(session.to_string()),
            )
            .await
            .ok()?;
        let refresh = quench_core::hex::encode(quench_core::random::bytes::<24>());
        self.inner
            .refresh
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(refresh.clone(), (user.to_string(), session.to_string()));
        Some(Tokens {
            access_token: access,
            refresh_token: refresh,
            token_type: "Bearer".to_string(),
            expires_in: self.inner.access_ttl.load(Ordering::SeqCst),
        })
    }

    /// The tokens as a body, plus the cookies a browser would keep.
    fn answer(tokens: &Tokens) -> Response {
        Response::json(StatusCode::OK, tokens)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
            .append_header(
                "set-cookie",
                realm::session_cookie(tokens.access_token.clone()).to_string(),
            )
            .append_header(
                "set-cookie",
                realm::refresh_cookie(tokens.refresh_token.clone()).to_string(),
            )
    }

    async fn login(&self, mut req: Request) -> Response {
        let Ok(body) = req.take_body().collect_limited(1 << 16).await else {
            return Response::new(StatusCode::BAD_REQUEST);
        };
        let Ok(login) = json::from_slice::<Login>(&body) else {
            return Response::new(StatusCode::BAD_REQUEST);
        };
        if !self
            .inner
            .users
            .iter()
            .any(|(u, p)| *u == login.username && *p == login.password)
        {
            return refusal(StatusCode::UNAUTHORIZED, "invalid_credentials");
        }
        self.inner.logins.fetch_add(1, Ordering::SeqCst);
        let session = format!("s{}", self.inner.sessions.fetch_add(1, Ordering::SeqCst));
        self.issue(&login.username, &session).await.map_or_else(
            || Response::new(StatusCode::INTERNAL_SERVER_ERROR),
            |tokens| Self::answer(&tokens),
        )
    }

    async fn refresh(&self, mut req: Request) -> Response {
        let Ok(body) = req.take_body().collect_limited(1 << 16).await else {
            return Response::new(StatusCode::BAD_REQUEST);
        };
        let Ok(request) = json::from_slice::<Refresh>(&body) else {
            return Response::new(StatusCode::BAD_REQUEST);
        };
        let taken = self
            .inner
            .refresh
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&request.refresh_token);
        let Some((user, session)) = taken else {
            return Response::new(StatusCode::UNAUTHORIZED);
        };
        self.inner.refreshes.fetch_add(1, Ordering::SeqCst);
        self.issue(&user, &session).await.map_or_else(
            || Response::new(StatusCode::INTERNAL_SERVER_ERROR),
            |tokens| Self::answer(&tokens),
        )
    }

    async fn logout(&self, mut req: Request) -> Response {
        if let Ok(body) = req.take_body().collect_limited(1 << 16).await
            && let Ok(request) = json::from_slice::<Refresh>(&body)
        {
            self.inner
                .refresh
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&request.refresh_token);
        }
        Response::new(StatusCode::NO_CONTENT)
            .append_header("set-cookie", clearing_cookie(&realm::session_cookie_name()))
            .append_header("set-cookie", clearing_cookie(&realm::refresh_cookie_name()))
    }
}

#[async_trait]
impl Endpoint for StubGatehouse {
    async fn call(&self, req: Request) -> Response {
        if req.method().as_str() != "POST" {
            return Response::new(StatusCode::METHOD_NOT_ALLOWED);
        }
        match req.uri().path() {
            "/api/v1/auth/login" => self.login(req).await,
            "/api/v1/auth/refresh" => self.refresh(req).await,
            "/api/v1/auth/logout" => self.logout(req).await,
            _ => Response::not_found(),
        }
    }
}

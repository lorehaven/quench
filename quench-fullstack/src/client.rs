//! Calling the server from the browser, and signing in.
//!
//! The `#[server]` stub calls [`call`]. A cookie session needs nothing more: the browser sends the
//! cookie with every same-origin request. A host without cookies (an Android or desktop shell)
//! signs in with [`Bearer`], which keeps the tokens in a [`TokenStore`] and refreshes them when the
//! server answers 401, once per expiry however many calls ran into it.

// The browser runs one thread, and the futures here hold `Rc`s: they are not meant to be `Send`.
#![allow(clippy::future_not_send)]

use std::cell::RefCell;
use std::rc::Rc;

use quench_serde::{Deserialize, json};
use quench_ui_web::{FetchError, Request, Response, sleep};

use crate::error::ServerError;
use crate::wire::{CONTENT_TYPE, RPC_HEADER, RPC_VALUE};

/// An access token and the refresh token that renews it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
}

/// Where a host keeps the tokens. The browser's own storage is not used: a shell keeps them in
/// its secure storage and implements this on top of it.
pub trait TokenStore {
    fn load(&self) -> Option<Tokens>;
    fn save(&self, tokens: Option<Tokens>);
}

/// Tokens held in memory only, gone when the page is.
#[derive(Debug, Default)]
pub struct MemoryStore(RefCell<Option<Tokens>>);

impl TokenStore for MemoryStore {
    fn load(&self) -> Option<Tokens> {
        self.0.borrow().clone()
    }

    fn save(&self, tokens: Option<Tokens>) {
        *self.0.borrow_mut() = tokens;
    }
}

/// What a gatehouse-compatible `login` and `refresh` answer with.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
}

/// What it answers with when it refuses.
#[derive(Default, Deserialize)]
struct AuthError {
    #[serde(default)]
    error: String,
}

/// Bearer-token sessions against a gatehouse-compatible endpoint.
///
/// `POST {base}/login` takes `{"username", "password"}` and `POST {base}/refresh` takes
/// `{"refresh_token"}`; each answers with `{"access_token", "refresh_token", ...}`. The refresh
/// token is rotated on every refresh, so the store always holds the newest pair.
#[derive(Clone)]
pub struct Bearer {
    base: String,
    store: Rc<dyn TokenStore>,
}

impl std::fmt::Debug for Bearer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bearer")
            .field("base", &self.base)
            .field("signed_in", &self.signed_in())
            .finish_non_exhaustive()
    }
}

impl Bearer {
    /// `base` is the auth API's path or URL, such as `/api/v1/auth`.
    #[must_use]
    pub fn new(base: &str, store: Rc<dyn TokenStore>) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            store,
        }
    }

    #[must_use]
    pub fn signed_in(&self) -> bool {
        self.store.load().is_some()
    }

    #[must_use]
    pub fn access_token(&self) -> Option<String> {
        self.store.load().map(|t| t.access)
    }

    /// Signs in with a username and password and stores the tokens.
    pub async fn login(&self, username: &str, password: &str) -> Result<(), ServerError> {
        let body =
            json::to_string(&quench_serde::json!({ "username": username, "password": password }));
        self.exchange("login", body).await
    }

    /// Trades the refresh token for a new pair. A refresh token the server no longer accepts ends
    /// the session: the store is cleared and the error is [`ServerError::Unauthorized`].
    pub async fn refresh(&self) -> Result<(), ServerError> {
        let Some(tokens) = self.store.load() else {
            return Err(ServerError::unauthorized("not signed in"));
        };
        let body = json::to_string(&quench_serde::json!({ "refresh_token": tokens.refresh }));
        let result = self.exchange("refresh", body).await;
        if matches!(result, Err(ServerError::Unauthorized(_))) {
            self.store.save(None);
        }
        result
    }

    /// Ends the session on the server (best effort) and forgets the tokens.
    pub async fn logout(&self) {
        if let Some(tokens) = self.store.load() {
            let body = json::to_string(&quench_serde::json!({ "refresh_token": tokens.refresh }));
            let _ = post(&format!("{}/logout", self.base), body, None).await;
        }
        self.store.save(None);
    }

    async fn exchange(&self, action: &str, body: String) -> Result<(), ServerError> {
        let response = post(&format!("{}/{action}", self.base), body, None).await?;
        if !response.ok() {
            return Err(auth_error(&response));
        }
        let tokens: TokenResponse = json::from_str(&response.text())
            .map_err(|e| ServerError::BadResponse(format!("unreadable tokens: {e}")))?;
        self.store.save(Some(Tokens {
            access: tokens.access_token,
            refresh: tokens.refresh_token,
        }));
        Ok(())
    }
}

fn auth_error(response: &Response) -> ServerError {
    let code = json::from_str::<AuthError>(&response.text())
        .map(|e| e.error)
        .unwrap_or_default();
    let message = if code.is_empty() {
        format!("sign-in refused ({})", response.status)
    } else {
        code.replace('_', " ")
    };
    match response.status {
        401 | 403 => ServerError::Unauthorized(message),
        s if s >= 500 => ServerError::Internal(message),
        _ => ServerError::BadResponse(message),
    }
}

async fn post(url: &str, body: String, bearer: Option<&str>) -> Result<Response, ServerError> {
    let mut request = Request::post(url)
        .header("content-type", CONTENT_TYPE)
        .header("accept", CONTENT_TYPE)
        .header(RPC_HEADER, RPC_VALUE)
        .body(body);
    if let Some(token) = bearer {
        request = request.header("authorization", &format!("Bearer {token}"));
    }
    request.send().await.map_err(|e| match e {
        FetchError::Network(m) => ServerError::Network(m),
        FetchError::Aborted => ServerError::Network("request aborted".to_string()),
    })
}

#[derive(Default)]
struct State {
    bearer: Option<Bearer>,
    on_unauthorized: Option<Rc<dyn Fn()>>,
    /// Bumped by every successful refresh.
    generation: u32,
    refreshing: bool,
    /// The generation a refresh failed in, and why, for the calls that were waiting for it.
    failed: Option<(u32, ServerError)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Makes every call carry `bearer`'s access token and refresh it on a 401.
pub fn use_bearer(bearer: Bearer) {
    STATE.with(|s| s.borrow_mut().bearer = Some(bearer));
}

/// Runs `handler` when a call ends as [`ServerError::Unauthorized`] and no refresh can help: the
/// place to show a sign-in page.
pub fn on_unauthorized(handler: impl Fn() + 'static) {
    STATE.with(|s| s.borrow_mut().on_unauthorized = Some(Rc::new(handler)));
}

/// Forgets the bearer session, the handler and any refresh in progress. For tests.
pub fn reset() {
    STATE.with(|s| *s.borrow_mut() = State::default());
}

fn bearer() -> Option<Bearer> {
    STATE.with(|s| s.borrow().bearer.clone())
}

fn generation() -> u32 {
    STATE.with(|s| s.borrow().generation)
}

fn notify_unauthorized() {
    let handler = STATE.with(|s| s.borrow().on_unauthorized.clone());
    if let Some(handler) = handler {
        handler();
    }
}

/// What a call that met a 401 does next.
enum Next {
    /// Someone else settled it.
    Settled(Result<(), ServerError>),
    /// A refresh is running; wait for it.
    Wait,
    /// Nobody is refreshing: this call does.
    Refresh,
}

/// Renews the tokens after a 401 for a request that started in generation `started_in`. If another
/// call already did, or is doing so, this one waits for that instead of using the (now rotated)
/// refresh token a second time.
async fn renew(bearer: &Bearer, started_in: u32) -> Result<(), ServerError> {
    loop {
        let next = STATE.with(|s| {
            let s = s.borrow();
            if s.generation != started_in {
                Next::Settled(Ok(()))
            } else if let Some((g, e)) = &s.failed
                && *g == started_in
            {
                Next::Settled(Err(e.clone()))
            } else if s.refreshing {
                Next::Wait
            } else {
                Next::Refresh
            }
        });
        match next {
            Next::Settled(result) => return result,
            Next::Wait => sleep(5).await,
            Next::Refresh => break,
        }
    }
    STATE.with(|s| s.borrow_mut().refreshing = true);
    let result = bearer.refresh().await;
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.refreshing = false;
        match &result {
            Ok(()) => s.generation += 1,
            Err(e) => s.failed = Some((started_in, e.clone())),
        }
    });
    result
}

/// Calls the server function at `path` with `args` and decodes its answer. What `#[server]`
/// expands to in the browser.
pub async fn call<A: quench_serde::Serialize, T: Deserialize>(
    path: &str,
    args: &A,
) -> Result<T, ServerError> {
    let body = json::to_string(args);
    let mut renewed = false;
    loop {
        let started_in = generation();
        let token = bearer().and_then(|b| b.access_token());
        let response = post(path, body.clone(), token.as_deref()).await?;
        if response.status == 401
            && !renewed
            && let Some(bearer) = bearer()
        {
            renewed = true;
            if let Err(e) = renew(&bearer, started_in).await {
                notify_unauthorized();
                return Err(e);
            }
            continue;
        }
        return finish(&response);
    }
}

fn finish<T: Deserialize>(response: &Response) -> Result<T, ServerError> {
    let text = response.text();
    if response.ok() {
        return json::from_str(&text)
            .map_err(|e| ServerError::BadResponse(format!("unreadable answer: {e}")));
    }
    let error = ServerError::from_response(response.status, &text);
    if matches!(error, ServerError::Unauthorized(_)) {
        notify_unauthorized();
    }
    Err(error)
}

/// Signs in for a cookie session: posts the credentials to `{base}/login` and lets the browser
/// keep the cookies the answer sets. For gatehouse-compatible endpoints that set them.
pub async fn sign_in(base: &str, username: &str, password: &str) -> Result<(), ServerError> {
    let body =
        json::to_string(&quench_serde::json!({ "username": username, "password": password }));
    let response = post(&format!("{}/login", base.trim_end_matches('/')), body, None).await?;
    if response.ok() {
        Ok(())
    } else {
        Err(auth_error(&response))
    }
}

/// Ends a cookie session: posts to `{base}/logout`, which clears the cookies.
pub async fn sign_out(base: &str) -> Result<(), ServerError> {
    let response = post(
        &format!("{}/logout", base.trim_end_matches('/')),
        "{}".to_string(),
        None,
    )
    .await?;
    if response.ok() {
        Ok(())
    } else {
        Err(auth_error(&response))
    }
}

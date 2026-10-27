//! What the browser and the server agree on for a server function call.

/// Every call is a `POST` of a JSON object (the arguments by name) with this content type.
pub const CONTENT_TYPE: &str = "application/json";

/// A header every call carries.
///
/// A page on another origin cannot add a custom header to a cross-origin request without the
/// server's say-so (a CORS preflight it will not get), so a request without it is refused: that is
/// the protection against cross-site request forgery for cookie sessions.
pub const RPC_HEADER: &str = "x-quench-rpc";

/// The value of [`RPC_HEADER`].
pub const RPC_VALUE: &str = "1";

/// The largest request body a server function accepts, in bytes.
pub const MAX_BODY: usize = 1 << 20;

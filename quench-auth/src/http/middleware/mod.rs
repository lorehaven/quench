//! Mounting order: `wrap(inner, m)` runs `m` *around* `inner` - the last
//! `wrap` call is outermost, the opposite of actix-web's `.wrap()` (last
//! registered runs first). `RequireWrite` reads the `Claims` `Auth` sets,
//! so `Auth` has to run first, which here means it's the *outer* layer:
//!
//! ```ignore
//! let app = wrap(routes, RequireWrite::new(config.clone()));
//! let app = wrap(app, Auth::new(config)); // outermost: runs first
//! ```

pub mod auth;
pub mod require_write;

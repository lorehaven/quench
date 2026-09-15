pub mod api_error;
pub mod error;

// `DbWrapper` lives in `crate::common::db` - it never touched actix, so
// there's nothing to port; reuse it directly.
pub use crate::common::db::DbWrapper;

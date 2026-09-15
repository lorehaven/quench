// `DbWrapper` doesn't touch actix at all - it moved to `common::db` so the
// quench-http bootstrap (`crate::http`) can reuse it verbatim instead of
// duplicating it. Re-exported here so `quench_starter::prelude::DbWrapper`
// (and anything else importing this path) keeps working unchanged.
pub use crate::common::db::DbWrapper;

pub mod actix;
pub mod common;
// Not glob re-exported at the crate root like `actix` is: `actix::serve`
// and `http::serve` (and `domain`/`middleware`/`routers`) would collide.
// Reach it as `quench_starter::http::serve` while both stacks coexist.
pub mod http;
pub mod logging;
pub mod metrics;
pub mod prelude;
pub mod resilience;

pub use actix::*;
pub use common::*;
pub use logging::*;
pub use metrics::*;
pub use resilience::*;

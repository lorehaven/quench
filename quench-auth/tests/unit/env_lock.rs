//! A process-wide guard for tests that read/write global environment
//! variables. `envmnt`/`std::env` are not scoped per-thread, so any test
//! across this binary that touches a variable name another file also touches
//! (`GATEHOUSE_URL`, `GATEHOUSE_CLIENT_ID`, `GATEHOUSE_CLIENT_SECRET`,
//! `GATEHOUSE_TLS_VERIFY`) must hold this lock for the duration.

use std::sync::Mutex;

pub static ENV_LOCK: Mutex<()> = Mutex::new(());

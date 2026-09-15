use actix_web::dev::HttpServiceFactory;
use actix_web::middleware::NormalizePath;
use actix_web::{HttpResponse, Responder, get, web};

// `HealthState` doesn't touch actix - it moved to `common::health` so the
// quench-http bootstrap (`crate::http`) can share it instead of duplicating
// it. Re-exported so `quench_starter::prelude::HealthState` keeps working.
pub use crate::common::health::HealthState;

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/health")
        .wrap(NormalizePath::trim())
        .service(health)
        .service(live)
        .service(ready)
}

#[get("")]
#[doc(hidden)]
pub async fn health() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

#[get("/live")]
#[doc(hidden)]
pub async fn live(state: web::Data<HealthState>) -> impl Responder {
    if state.is_live() {
        HttpResponse::Ok().json(serde_json::json!({ "status": "live" }))
    } else {
        HttpResponse::ServiceUnavailable().json(serde_json::json!({ "status": "not_live" }))
    }
}

#[get("/ready")]
#[doc(hidden)]
pub async fn ready(state: web::Data<HealthState>) -> impl Responder {
    if state.is_ready() {
        HttpResponse::Ok().json(serde_json::json!({ "status": "ready" }))
    } else {
        HttpResponse::ServiceUnavailable().json(serde_json::json!({ "status": "not_ready" }))
    }
}

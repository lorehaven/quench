//! Unit tests for `actix/routers/swagger.rs`.

use actix_web::{App, http::StatusCode, test};

#[actix_web::test]
async fn swagger_ui_redirects_to_the_trailing_slash_form() {
    let app = test::init_service(
        App::new().service(quench_starter::actix::routers::swagger::swagger_redirect),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::get().uri("/swagger-ui").to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    let location = response
        .headers()
        .get("location")
        .expect("a location")
        .to_str()
        .expect("a header that is text");
    assert!(location.ends_with("/swagger-ui/"), "got: {location}");
}

#[actix_web::test]
async fn swagger_ui_index_redirects_to_the_html_file() {
    let app = test::init_service(
        App::new().service(quench_starter::actix::routers::swagger::swagger_index_redirect),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::get().uri("/swagger-ui/").to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    let location = response
        .headers()
        .get("location")
        .expect("a location")
        .to_str()
        .expect("a header that is text");
    assert!(
        location.ends_with("/swagger-ui/index.html"),
        "got: {location}"
    );
}

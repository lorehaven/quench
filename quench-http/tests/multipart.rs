//! `Multipart` is the quench-http replacement for `actix-multipart`,
//! needed to migrate `sage-service`'s file upload endpoint (a `file` bytes
//! field plus optional `conversation_id`/`project_id` text fields - see
//! `docker/sage-service/src/routers/files.rs`'s `FileUploadForm`).

use bytes::Bytes as RawBytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::extract::{BodyLimit, FromRequest};
use quench_http::multipart::Multipart;
use quench_http::request::Request;
use std::sync::Arc;

const BOUNDARY: &str = "quench-test-boundary";

fn multipart_request(body: String, container: &Arc<quench_http::di::Container>) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        format!("multipart/form-data; boundary={BOUNDARY}")
            .parse()
            .unwrap(),
    );
    Request::new(
        Method::POST,
        "/upload".parse::<Uri>().unwrap(),
        headers,
        InboundBody::from_bytes(RawBytes::from(body)),
        container.clone(),
    )
}

fn sample_body() -> String {
    format!(
        "--{BOUNDARY}\r\n\
         Content-Disposition: form-data; name=\"conversation_id\"\r\n\
         \r\n\
         conv-123\r\n\
         --{BOUNDARY}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"hello.txt\"\r\n\
         Content-Type: text/plain\r\n\
         \r\n\
         hello world\r\n\
         --{BOUNDARY}--\r\n"
    )
}

#[tokio::test]
async fn reads_a_text_field_and_a_file_field_by_name() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let mut req = multipart_request(sample_body(), &container);
    let mut form = Multipart::from_request(&mut req)
        .await
        .expect("valid multipart body");

    let mut conversation_id = None;
    let mut file_name = None;
    let mut file_bytes = None;
    let mut file_content_type = None;

    while let Some(field) = form.next_field().await.expect("field reads cleanly") {
        match field.name() {
            Some("conversation_id") => conversation_id = Some(field.text().await.unwrap()),
            Some("file") => {
                file_name = field.file_name().map(str::to_string);
                file_content_type = field.content_type().map(str::to_string);
                file_bytes = Some(field.bytes().await.unwrap());
            }
            _ => {}
        }
    }

    assert_eq!(conversation_id.as_deref(), Some("conv-123"));
    assert_eq!(file_name.as_deref(), Some("hello.txt"));
    assert_eq!(file_content_type.as_deref(), Some("text/plain"));
    assert_eq!(file_bytes.unwrap(), RawBytes::from_static(b"hello world"));
}

#[tokio::test]
async fn missing_boundary_is_rejected_before_reading_any_field() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    let mut req = Request::new(
        Method::POST,
        "/upload".parse::<Uri>().unwrap(),
        headers,
        InboundBody::from_bytes(RawBytes::new()),
        container.clone(),
    );

    let err = match Multipart::from_request(&mut req).await {
        Err(e) => e,
        Ok(_) => panic!("a non-multipart content-type should be rejected"),
    };
    assert_eq!(err.into_response().status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn whole_stream_body_limit_is_respected() {
    // A limit smaller than the sample body's file field.
    let container = Arc::new(
        ContainerBuilder::new()
            .provide(BodyLimit(5))
            .build()
            .await
            .unwrap(),
    );
    let mut req = multipart_request(sample_body(), &container);
    let mut form = Multipart::from_request(&mut req)
        .await
        .expect("boundary parses fine, the body itself is what's too big");

    // Whichever field is read first, the stream as a whole exceeds the
    // 5-byte cap before both fields can be fully read.
    let mut saw_error = false;
    loop {
        match form.next_field().await {
            Ok(Some(field)) => {
                if field.bytes().await.is_err() {
                    saw_error = true;
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => {
                saw_error = true;
                break;
            }
        }
    }
    assert!(
        saw_error,
        "reading past the 5-byte whole-stream limit should fail somewhere"
    );
}

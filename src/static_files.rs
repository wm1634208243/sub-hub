use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;
use std::path::Path;

#[derive(RustEmbed)]
#[folder = "public/"]
pub struct EmbeddedAssets;

pub async fn static_handler(uri: axum::http::Uri) -> Response {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if path.is_empty() {
        path = "index.html".to_string();
    }

    // 1. Try embedded binary assets (single binary deployment takes priority)
    if let Some(file) = EmbeddedAssets::get(&path) {
        let mime = mime_guess::from_path(&path).first_or_octet_stream();
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, HeaderValue::from_str(mime.as_ref()).unwrap())
            .body(Body::from(file.data))
            .unwrap();
    }

    // 2. Fallback to local filesystem
    let local_file = Path::new("public").join(&path);
    if local_file.exists() && local_file.is_file() {
        if let Ok(bytes) = tokio::fs::read(&local_file).await {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, HeaderValue::from_str(mime.as_ref()).unwrap())
                .body(Body::from(bytes))
                .unwrap();
        }
    }
    if let Some(index) = EmbeddedAssets::get("index.html") {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"))
            .body(Body::from(index.data))
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("404 Not Found"))
            .unwrap()
    }
}

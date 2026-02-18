use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use mime_guess::from_path;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "ui/dist"]
struct UiAssets;

pub async fn serve_embedded_asset(uri: Uri) -> Response {
    let requested_path = uri.path().trim_start_matches('/');

    let asset_path = if requested_path.is_empty() {
        "index.html"
    } else {
        requested_path
    };

    if let Some(response) = asset_response(asset_path) {
        return response;
    }

    if let Some(response) = asset_response("index.html") {
        return response;
    }

    StatusCode::NOT_FOUND.into_response()
}

fn asset_response(path: &str) -> Option<Response> {
    let content = UiAssets::get(path)?;

    let mime = from_path(path).first_or_octet_stream();
    let mut response = Response::new(Body::from(content.data.into_owned()));

    if let Ok(value) = HeaderValue::from_str(mime.as_ref()) {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }

    Some(response)
}

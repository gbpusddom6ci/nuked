use axum::{
    extract::Multipart,
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use std::net::SocketAddr;

mod analysis;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/analyze", post(analyze));

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Listening on http://{}", addr);

    axum::serve(
        tokio::net::TcpListener::bind(addr).await?,
        app.into_make_service(),
    )
    .await?;

    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn analyze(mut multipart: Multipart) -> Result<Json<analysis::Report>, (StatusCode, String)> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;
    let mut timeframe: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(to_bad_request)? {
        if field.name() == Some("file") {
            file_name = field.file_name().map(|name| name.to_string());
            let data = field.bytes().await.map_err(to_bad_request)?;
            file_bytes = Some(data.to_vec());
        } else if field.name() == Some("timeframe") {
            timeframe = Some(field.text().await.map_err(to_bad_request)?);
        }
    }

    let bytes = file_bytes.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Missing file field named 'file'.".to_string(),
        )
    })?;

    let report =
        analysis::analyze_input(&bytes, file_name.as_deref(), timeframe.as_deref())
            .map_err(to_bad_request)?;
    Ok(Json(report))
}

fn to_bad_request<E: std::fmt::Display>(err: E) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, err.to_string())
}

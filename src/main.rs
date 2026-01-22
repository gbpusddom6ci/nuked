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

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
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

    while let Some(field) = multipart.next_field().await.map_err(to_bad_request)? {
        if field.name() == Some("file") {
            let data = field.bytes().await.map_err(to_bad_request)?;
            file_bytes = Some(data.to_vec());
        }
    }

    let bytes = file_bytes.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Missing file field named 'file'.".to_string(),
        )
    })?;

    let report = analysis::analyze_csv(&bytes).map_err(to_bad_request)?;
    Ok(Json(report))
}

fn to_bad_request<E: std::fmt::Display>(err: E) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, err.to_string())
}

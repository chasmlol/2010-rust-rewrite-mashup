use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum_server::tls_rustls::RustlsConfig;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::{Result, load_certificates, load_private_key};

pub async fn serve(bind: SocketAddr, cert: PathBuf, key: PathBuf, root: PathBuf) -> Result<()> {
    let mut tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(load_certificates(&cert)?, load_private_key(&key)?)?;
    tls.alpn_protocols = vec![b"http/1.1".to_vec()];
    let listener = TcpListener::bind(bind)?;
    let app = Router::new()
        .route("/health", get(|| async { "ok\n" }))
        .route("/updates/{name}", get(file))
        .with_state(root);
    axum_server::from_tcp_rustls(listener, RustlsConfig::from_config(Arc::new(tls)))
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

async fn file(
    State(root): State<PathBuf>,
    Path(name): Path<String>,
    request: Request<axum::body::Body>,
) -> Response {
    let blob = name
        .strip_prefix("iw4l-")
        .and_then(|s| s.strip_suffix(".exe.zst"))
        .is_some_and(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        });
    if name != "manifest.toml" && !blob {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = root.join(&name);
    if !tokio::fs::symlink_metadata(&path)
        .await
        .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut response = ServeFile::new(path)
        .oneshot(request)
        .await
        .expect("ServeFile is infallible")
        .into_response();
    let cache = if blob {
        format!("public, max-age={}, immutable", 365 * 24 * 60 * 60)
    } else {
        "no-store".into()
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, cache.parse().expect("cache header"));
    response
}

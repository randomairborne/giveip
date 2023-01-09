#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::net::SocketAddr;

use axum::{extract::ConnectInfo, http::HeaderMap};
use axum_server::{tls_rustls::RustlsConfig, Handle};

#[allow(clippy::unused_async)]
async fn home(
    ConnectInfo(sock_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> ([(&'static str, &'static str); 1], String) {
    let real_ip = headers.get("X-Real-IP").map_or_else(
        || sock_addr.ip().to_string(),
        |ip| {
            ip.to_str().map_or_else(
                |_| sock_addr.ip().to_string(),
                std::string::ToString::to_string,
            )
        },
    );
    let accept = headers
        .get("Accept")
        .map_or("*/*", |x| x.to_str().unwrap_or("invalid header value"));
    if accept.contains("text/html") {
        (
            [("Content-Type", "text/html; charset=utf-8")],
            include_str!("index.html").to_string(),
        )
    } else {
        (
            [("Content-Type", "text/plain; charset=utf-8")],
            format!("{real_ip}\n"),
        )
    }
}
#[allow(clippy::unused_async)]
async fn raw(ConnectInfo(sock_addr): ConnectInfo<SocketAddr>, headers: HeaderMap) -> String {
    let real_ip = headers.get("X-Real-IP").map_or_else(
        || sock_addr.ip().to_string(),
        |ip| {
            ip.to_str().map_or_else(
                |_| sock_addr.ip().to_string(),
                std::string::ToString::to_string,
            )
        },
    );
    format!("{real_ip}\n")
}

#[tokio::main]
async fn main() {
    let addr = std::net::SocketAddr::from((
        [0, 0, 0, 0],
        std::env::var("PORT")
            .unwrap_or_else(|_| 8080.to_string())
            .parse::<u16>()
            .unwrap_or(8080),
    ));
    let tls_addr = std::net::SocketAddr::from((
        [0, 0, 0, 0],
        std::env::var("HTTPS_PORT")
            .unwrap_or_else(|_| 4443.to_string())
            .parse::<u16>()
            .unwrap_or(4443),
    ));
    let app = axum::Router::new()
        .route("/", axum::routing::get(home))
        .route("/raw", axum::routing::get(raw))
        .with_state(());
    let handle = Handle::new();
    let perhaps_cert = std::env::var("CERTIFICATE");
    let perhaps_key = std::env::var("PRIVATE_KEY");
    let sd_handle = handle.clone();
    tokio::task::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        println!("Server shutting down...");
        sd_handle.shutdown();
    });
    if perhaps_cert.is_ok() || perhaps_key.is_ok() {
        if let (Ok(cert), Ok(key)) = (perhaps_cert, perhaps_key) {
            let tls_cfg = RustlsConfig::from_pem(cert.into(), key.into())
                .await
                .expect("Error loading certificate or privkey!");
            let app = app.clone();
            let handle = handle.clone();
            tokio::task::spawn(async move {
                println!("Listening on https://{tls_addr}");
                axum_server::bind_rustls(tls_addr, tls_cfg)
                    .handle(handle)
                    .serve(app.into_make_service_with_connect_info::<std::net::SocketAddr>())
                    .await
                    .expect("Failed to run https server");
            });
        } else {
            panic!("Both CERTIFICATE and PRIVATE_KEY must be set to run https")
        }
    }
    println!("Listening on http://{addr}");
    axum_server::bind(addr)
        .handle(handle)
        .serve(app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .await
        .expect("Failed to run http server");
}

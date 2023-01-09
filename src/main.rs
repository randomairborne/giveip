#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, State},
    http::HeaderMap,
};
use axum_server::Handle;

#[allow(clippy::unused_async)]
async fn home(
    ConnectInfo(sock_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(header_name): State<String>,
) -> ([(&'static str, &'static str); 1], String) {
    let real_ip = headers.get(&header_name).map_or_else(
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
async fn raw(
    ConnectInfo(sock_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(header_name): State<String>,
) -> ([(&'static str, &'static str); 1], String) {
    let real_ip = headers.get(&header_name).map_or_else(
        || sock_addr.ip().to_string(),
        |ip| {
            ip.to_str().map_or_else(
                |_| sock_addr.ip().to_string(),
                std::string::ToString::to_string,
            )
        },
    );
    (
        [("Access-Control-Allow-Origin", "*")],
        format!("{real_ip}\n"),
    )
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
    let client_ip_var =
        std::env::var("CLIENT_IP_HEADER").unwrap_or_else(|_| "X-Real-IP".to_string());
    let app = axum::Router::new()
        .route("/", axum::routing::get(home))
        .route("/raw", axum::routing::get(raw))
        .with_state(client_ip_var);
    let handle = Handle::new();
    let sd_handle = handle.clone();
    tokio::task::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        println!("Server shutting down...");
        sd_handle.shutdown();
    });
    println!("Listening on http://{addr}");
    axum_server::bind(addr)
        .handle(handle)
        .serve(app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .await
        .expect("Failed to run http server");
}

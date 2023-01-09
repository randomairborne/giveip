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
    State(maybe_header_name): State<Option<String>>,
) -> ([(&'static str, &'static str); 1], String) {
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
            format!("{}\n", get_ip(sock_addr, headers, maybe_header_name)),
        )
    }
}

#[allow(clippy::unused_async)]
async fn raw(
    ConnectInfo(sock_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(maybe_header_name): State<Option<String>>,
) -> ([(&'static str, &'static str); 1], String) {
    (
        [("Access-Control-Allow-Origin", "*")],
        format!("{}\n", get_ip(sock_addr, headers, maybe_header_name)),
    )
}

fn get_ip(addr: SocketAddr, headers: HeaderMap, maybe_header_name: Option<String>) -> String {
    if let Some(header_name) = maybe_header_name {
        headers.get(&header_name).map_or_else(
            || format!("No {header_name} header found"),
            |ip| {
                ip.to_str()
                    .map_or_else(|_| addr.ip().to_string(), std::string::ToString::to_string)
            },
        )
    } else {
        addr.ip().to_string()
    }
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
    let client_ip_var = std::env::var("CLIENT_IP_HEADER").ok();
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

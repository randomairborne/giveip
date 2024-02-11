#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{any, get},
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let addr = SocketAddr::from((
        [0, 0, 0, 0],
        std::env::var("PORT")
            .unwrap_or_else(|_| 8080.to_string())
            .parse::<u16>()
            .unwrap_or(8080),
    ));
    let client_ip_var = std::env::var("CLIENT_IP_HEADER").ok();
    let state = AppState::new(client_ip_var);
    let app = axum::Router::new()
        .route("/raw", any(raw))
        .layer(axum::middleware::from_fn(noindex))
        .route("/", get(home))
        .layer(axum::middleware::from_fn(nocors))
        .with_state(state.clone());
    println!("Listening on http://{addr} for ip requests");
    let tcp = TcpListener::bind(addr).await.unwrap();
    axum::serve(tcp, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(vss::shutdown_signal())
        .await
        .unwrap();
}

#[allow(clippy::unused_async)]
async fn home(
    ConnectInfo(sock_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<HtmlOrRaw, Error> {
    let accept = headers
        .get("Accept")
        .map_or("*/*", |x| x.to_str().unwrap_or("invalid header value"));
    let ip = get_ip(sock_addr, &headers, state.clone())?;
    if accept.contains("text/html") {
        Ok(HtmlOrRaw::Html(include_str!("index.html")))
    } else {
        Ok(HtmlOrRaw::Raw(format!("{ip}\n")))
    }
}

#[allow(clippy::unused_async)]
async fn raw(
    ConnectInfo(sock_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<String, Error> {
    let ip = get_ip(sock_addr, &headers, state.clone())?;
    Ok(format!("{ip}\n"))
}

fn get_ip(addr: SocketAddr, headers: &HeaderMap, state: AppState) -> Result<IpAddr, Error> {
    if let Some(header_name) = state.header {
        if let Some(header) = headers.get(&*header_name) {
            let sock_str = header.to_str()?;
            Ok(IpAddr::from_str(sock_str)?)
        } else {
            Err(Error::NoHeader)
        }
    } else {
        Ok(addr.ip())
    }
}

static CORS_STAR: HeaderValue = HeaderValue::from_static("*");

async fn nocors(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
        CORS_STAR.clone(),
    );
    response
}

static ROBOTS_NAME: HeaderName = HeaderName::from_static("x-robots-tag");
static ROBOTS_VALUE: HeaderValue = HeaderValue::from_static("noindex");

async fn noindex(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    resp.headers_mut()
        .insert(ROBOTS_NAME.clone(), ROBOTS_VALUE.clone());
    resp
}

#[derive(Clone)]
pub struct AppState {
    header: Option<Arc<HeaderName>>,
}

impl AppState {
    #[must_use]
    /// # Panics
    /// This function can panic when its hardcoded values are invalid
    /// or the passed `client_ip_name` is not a valid header name
    pub fn new(client_ip_name: Option<String>) -> Self {
        Self {
            header: client_ip_name.map(|v| Arc::new(HeaderName::try_from(v).unwrap())),
        }
    }
}

pub enum HtmlOrRaw {
    Html(&'static str),
    Raw(String),
}

static CACHE_CONTROL_VALUE: HeaderValue = HeaderValue::from_static("no-store");

impl IntoResponse for HtmlOrRaw {
    fn into_response(self) -> Response {
        match self {
            Self::Html(v) => axum::response::Html(v).into_response(),
            Self::Raw(v) => {
                let mut resp = v.into_response();
                resp.headers_mut().insert(
                    axum::http::header::CACHE_CONTROL,
                    CACHE_CONTROL_VALUE.clone(),
                );
                resp
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("No header found")]
    NoHeader,
    #[error("Could not convert supplied header to string")]
    ToStr(#[from] axum::http::header::ToStrError),
    #[error("Could not convert supplied header to IP address")]
    ToAddr(#[from] std::net::AddrParseError),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}

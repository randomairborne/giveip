#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderName, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{any, get},
};
use prometheus::{Encoder, IntCounterVec, Opts, Registry, TextEncoder};

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
    let v4 = ip.is_ipv4();
    if accept.contains("text/html") {
        if v4 {
            state.requests.browser.v4.inc();
        } else {
            state.requests.browser.v6.inc();
        }
        Ok(HtmlOrRaw::Html(include_str!("index.html")))
    } else {
        if v4 {
            state.requests.cmdline.v4.inc();
        } else {
            state.requests.cmdline.v6.inc();
        }
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
    if ip.is_ipv4() {
        state.requests.raw.v4.inc();
    } else {
        state.requests.raw.v6.inc();
    }
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

#[allow(clippy::unused_async)]
async fn metrics(State(state): State<AppState>) -> Result<Vec<u8>, Error> {
    let mut buffer = Vec::with_capacity(8192);
    let encoder = TextEncoder::new();
    let metrics = state.reg.gather();
    encoder.encode(&metrics, &mut buffer)?;
    Ok(buffer)
}

async fn nocors<B: Send>(request: Request<B>, next: Next<B>) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    response
}

#[tokio::main]
async fn main() {
    let addr = SocketAddr::from((
        [0, 0, 0, 0],
        std::env::var("PORT")
            .unwrap_or_else(|_| 8080.to_string())
            .parse::<u16>()
            .unwrap_or(8080),
    ));
    let metrics_addr = SocketAddr::from((
        [0, 0, 0, 0],
        std::env::var("METRICS_PORT")
            .unwrap_or_else(|_| 9090.to_string())
            .parse::<u16>()
            .unwrap_or(9090),
    ));
    let client_ip_var = std::env::var("CLIENT_IP_HEADER").ok();
    let state = AppState::new(client_ip_var);
    let metrics_app = axum::Router::new()
        .route("/metrics", get(metrics))
        .with_state(state.clone());
    let app = axum::Router::new()
        .route("/", get(home))
        .route("/raw", any(raw))
        .layer(axum::middleware::from_fn(nocors))
        .with_state(state.clone());
    let sd = tokio_shutdown::Shutdown::new().unwrap();
    println!(
        "Listening on http://{addr} for ip requests and http://{metrics_addr} for metrics requests"
    );
    let sd_s = sd.clone();
    tokio::spawn(async move {
        axum::Server::bind(&metrics_addr)
            .serve(metrics_app.into_make_service())
            .with_graceful_shutdown(async {
                sd.handle().await;
            })
            .await
            .unwrap();
    });
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            sd_s.handle().await;
        })
        .await
        .unwrap();
}

#[derive(Clone)]
pub struct AppState {
    requests: Arc<IpRequests>,
    reg: Registry,
    header: Option<Arc<HeaderName>>,
}

impl AppState {
    #[must_use]
    /// # Panics
    /// This function can panic when its hardcoded values are invalid
    /// or the passed `client_ip_name` is not a valid header name
    pub fn new(client_ip_name: Option<String>) -> Self {
        const NAMESPACE: &str = env!("CARGO_PKG_NAME");
        let requests_opts =
            Opts::new("requests", "Number of IP requests handled").namespace(NAMESPACE);
        let requests_untyped =
            IntCounterVec::new(requests_opts, &["ip_version", "request_kind"]).unwrap();
        let requests = Arc::new(IpRequests::from(&requests_untyped));
        let reg = Registry::new();
        reg.register(Box::new(requests_untyped)).unwrap();
        Self {
            requests,
            reg,
            header: client_ip_name.map(|v| Arc::new(HeaderName::try_from(v).unwrap())),
        }
    }
}

prometheus_static_metric::make_static_metric! {
    pub struct IpRequests: IntCounter {
        "request_kind" => {
            cmdline,
            browser,
            raw,
        },
        "ip_version" => {
            v4,
            v6
        },
    }
}

pub enum HtmlOrRaw {
    Html(&'static str),
    Raw(String),
}

impl IntoResponse for HtmlOrRaw {
    fn into_response(self) -> Response {
        match self {
            Self::Html(v) => axum::response::Html(v).into_response(),
            Self::Raw(v) => v.into_response(),
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
    #[error("Prometheus failed to serialize: {0}")]
    Prometheus(#[from] prometheus::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}

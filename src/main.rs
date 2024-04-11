#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(clippy::useless_let_if_seq)]

use std::{
    fmt::{Display, Formatter},
    net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6},
    str::FromStr,
    sync::Arc,
};

use askama::Template;
use axum::{
    extract::{ConnectInfo, FromRequestParts, Request, State},
    handler::Handler,
    http::{
        header::{ACCESS_CONTROL_ALLOW_ORIGIN, CACHE_CONTROL},
        request::Parts,
        HeaderName, HeaderValue, StatusCode,
    },
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{any, get},
    Router,
};
use bustdir::BustDir;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;

mod filters {
    pub use bustdir::askama::bust_dir;
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT").map_or(8080, |v| v.parse().expect("Invalid PORT"));
    let v6_addr = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0));

    let state = AppState::new();

    let serve_dir = ServeDir::new("assets").fallback(not_found.with_state(state.clone()));
    let assets = ServiceBuilder::new()
        .layer(axum::middleware::from_fn(noindex))
        .layer(axum::middleware::from_fn(infinicache))
        .service(serve_dir);

    let app = Router::new()
        .route("/", get(home))
        .route(
            "/raw",
            any(raw).layer(
                ServiceBuilder::new()
                    .layer(axum::middleware::from_fn(noindex))
                    .layer(axum::middleware::from_fn(nocors)),
            ),
        )
        .layer(axum::middleware::from_fn(nocache))
        .fallback_service(assets)
        .with_state(state.clone());

    println!("Listening on http://localhost:{port} and http://{v6_addr} for ip requests");
    let tcp6 = TcpListener::bind(v6_addr).await.unwrap();
    svc(tcp6, app).await;
}

async fn svc(tcp: TcpListener, app: Router) {
    axum::serve(tcp, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(vss::shutdown_signal())
        .await
        .unwrap();
}

#[derive(Template)]
#[template(path = "index.hbs", escape = "html", ext = "html")]
pub struct IndexPage {
    root_dns_name: Arc<str>,
    cb: Arc<BustDir>,
    ip: IpAddr,
    proto: String,
}

#[derive(Template)]
#[template(path = "404.hbs", escape = "html", ext = "html")]
pub struct NotFoundPage {
    cb: Arc<BustDir>,
}

#[allow(clippy::unused_async)]
async fn home(
    IpAddress(ip): IpAddress,
    XForwardedProto(proto): XForwardedProto,
    Accept(accept): Accept,
    State(state): State<AppState>,
) -> Result<Result<IndexPage, String>, Error> {
    if accept.contains("text/html") {
        let page = IndexPage {
            root_dns_name: state.root_dns_name,
            cb: state.cb,
            ip,
            proto,
        };
        Ok(Ok(page))
    } else {
        Ok(Err(format!("{ip}\n")))
    }
}

#[allow(clippy::unused_async)]
async fn raw(IpAddress(ip): IpAddress) -> Result<String, Error> {
    Ok(format!("{ip}\n"))
}

#[allow(clippy::unused_async)]
async fn not_found(State(state): State<AppState>) -> NotFoundPage {
    NotFoundPage { cb: state.cb }
}

async fn nocache(request: Request, next: Next) -> Response {
    static CACHE_CONTROL_PRIVATE: HeaderValue = HeaderValue::from_static("no-store, private");

    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, CACHE_CONTROL_PRIVATE.clone());
    response
}

async fn noindex(req: Request, next: Next) -> Response {
    static ROBOTS_NAME: HeaderName = HeaderName::from_static("x-robots-tag");
    static ROBOTS_VALUE: HeaderValue = HeaderValue::from_static("noindex");

    let mut resp = next.run(req).await;
    resp.headers_mut()
        .insert(ROBOTS_NAME.clone(), ROBOTS_VALUE.clone());
    resp
}

async fn nocors(req: Request, next: Next) -> Response {
    static CORS_STAR: HeaderValue = HeaderValue::from_static("*");

    let mut resp = next.run(req).await;

    resp.headers_mut()
        .insert(ACCESS_CONTROL_ALLOW_ORIGIN, CORS_STAR.clone());
    resp
}

async fn infinicache(request: Request, next: Next) -> Response {
    static CACHE_CONTROL_1_YEAR: HeaderValue =
        HeaderValue::from_static("immutable, public, max-age=3153600");

    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, CACHE_CONTROL_1_YEAR.clone());
    response
}

#[derive(Clone)]
pub struct AppState {
    header: Option<Arc<HeaderName>>,
    root_dns_name: Arc<str>,
    cb: Arc<BustDir>,
}

impl AppState {
    #[must_use]
    /// # Panics
    /// This function can panic when its hardcoded values are invalid
    /// or the passed `client_ip_name` is not a valid header name
    pub fn new() -> Self {
        let client_ip = std::env::var("CLIENT_IP_HEADER").ok();
        let root_dns_name: Arc<str> = std::env::var("ROOT_DNS_NAME")
            .expect("No ROOT_DNS_NAME in env")
            .into();
        let bust = BustDir::new("assets").expect("Failed to create cache-bustin hashes");
        let cb = Arc::new(bust);
        Self {
            header: client_ip.map(|v| Arc::new(HeaderName::try_from(v).unwrap())),
            root_dns_name,
            cb,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct IpAddress(IpAddr);

impl Display for IpAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[axum::async_trait]
impl FromRequestParts<AppState> for IpAddress {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(header_name) = state.header.clone() {
            if let Some(header) = parts.headers.get(&*header_name) {
                let sock_str = header.to_str()?;
                Ok(Self(IpAddr::from_str(sock_str)?))
            } else {
                Err(Error::NoHeader)
            }
        } else {
            let conn_info: ConnectInfo<SocketAddr> = ConnectInfo::from_request_parts(parts, state)
                .await
                .map_err(|_| Error::ConnectInfo)?;
            Ok(Self(conn_info.0.ip()))
        }
    }
}

#[derive(Clone, Debug)]
pub struct XForwardedProto(pub String);

#[axum::async_trait]
impl<S> FromRequestParts<S> for XForwardedProto {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let proto = parts
            .headers
            .get("X-Forwarded-Proto")
            .map(HeaderValue::to_str)
            .and_then(Result::ok)
            .unwrap_or("http")
            .to_owned();
        Ok(Self(proto))
    }
}

#[derive(Clone, Debug)]
pub struct Accept(String);

#[axum::async_trait]
impl<S> FromRequestParts<S> for Accept {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.headers.get("Accept").map_or_else(
            || Ok(Self(String::new())),
            |v| Ok(Self(v.to_str().unwrap_or("").to_string())),
        )
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("No header found")]
    NoHeader,
    #[error("Could not extract connection info")]
    ConnectInfo,
    #[error("Could not convert supplied header to string (this is a configuration issue)")]
    ToStr(#[from] axum::http::header::ToStrError),
    #[error("Could not convert supplied header to IP address (this is a configuration issue)")]
    ToAddr(#[from] std::net::AddrParseError),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}

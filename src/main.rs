#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use std::{
    fmt::{Display, Formatter},
    net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6},
    str::FromStr,
    sync::Arc,
};

use askama::Template;
use axum::{
    extract::{ConnectInfo, FromRequestParts, Request, State},
    http::{
        header::{ACCESS_CONTROL_ALLOW_ORIGIN, CACHE_CONTROL, CONTENT_TYPE},
        request::Parts,
        HeaderMap, HeaderName, HeaderValue, StatusCode,
    },
    middleware::Next,
    response::{Html, IntoResponse, Response},
    routing::{any, get},
    Router,
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT").map_or(8080, |v| v.parse().expect("Invalid PORT"));
    let v6_addr = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0));

    let state = AppState::new();
    let statics = Router::new()
        .route("/main.css", get(css))
        .route("/main.js", get(js))
        .layer(axum::middleware::from_fn(noindex));
    let app = Router::new()
        .route("/raw", any(raw))
        .layer(axum::middleware::from_fn(noindex))
        .route("/", get(home))
        .layer(axum::middleware::from_fn(nocors_nocache))
        .merge(statics)
        .fallback(not_found)
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
#[template(
    path = "index.hbs",
    escape = "html",
    whitespace = "suppress",
    ext = "html"
)]
pub struct IndexPage {
    root_dns_name: Arc<str>,
    ip: IpAddr,
    https: bool,
}

#[allow(clippy::unused_async)]
async fn home(
    IpAddress(ip): IpAddress,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Result<IndexPage, String>, Error> {
    let accept = headers
        .get("Accept")
        .map_or("*/*", |x| x.to_str().unwrap_or("invalid header value"));
    if accept.contains("text/html") {
        let page = IndexPage {
            root_dns_name: state.root_dns_name,
            ip,
            https: state.https,
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
async fn css() -> ([(HeaderName, HeaderValue); 1], &'static str) {
    static CSS_CONTENT_TYPE: HeaderValue = HeaderValue::from_static("text/css;charset=UTF-8");
    (
        [(CONTENT_TYPE, CSS_CONTENT_TYPE.clone())],
        include_str!("main.css"),
    )
}

#[allow(clippy::unused_async)]
async fn js() -> ([(HeaderName, HeaderValue); 1], &'static str) {
    static JS_CONTENT_TYPE: HeaderValue = HeaderValue::from_static("text/javascript;charset=UTF-8");
    (
        [(CONTENT_TYPE, JS_CONTENT_TYPE.clone())],
        include_str!("main.js"),
    )
}

#[allow(clippy::unused_async)]
async fn not_found() -> Html<&'static str> {
    Html(include_str!("404.html"))
}

async fn nocors_nocache(request: Request, next: Next) -> Response {
    static CORS_STAR: HeaderValue = HeaderValue::from_static("*");
    static CACHE_CONTROL_VALUE: HeaderValue = HeaderValue::from_static("no-store");

    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(ACCESS_CONTROL_ALLOW_ORIGIN, CORS_STAR.clone());
    response
        .headers_mut()
        .insert(CACHE_CONTROL, CACHE_CONTROL_VALUE.clone());
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

#[derive(Clone)]
pub struct AppState {
    header: Option<Arc<HeaderName>>,
    root_dns_name: Arc<str>,
    https: bool,
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
        let https = std::env::var("NO_HTTPS").is_err();
        Self {
            header: client_ip.map(|v| Arc::new(HeaderName::try_from(v).unwrap())),
            root_dns_name,
            https,
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

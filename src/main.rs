#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use std::{
    fmt::{Debug, Display, Formatter},
    net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6},
    str::FromStr,
    sync::Arc,
};

use askama::Template;
use axum::{
    extract::{ConnectInfo, FromRequestParts, Request, State},
    http::{
        header::{ACCESS_CONTROL_ALLOW_ORIGIN, CACHE_CONTROL, CONTENT_SECURITY_POLICY},
        request::Parts,
        HeaderName, HeaderValue, StatusCode,
    },
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{any, get},
    Router,
};
use rand::{distributions::Alphanumeric, Rng};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::set_header::SetResponseHeaderLayer;

static ROBOTS_NAME: HeaderName = HeaderName::from_static("x-robots-tag");
static ROBOTS_VALUE: HeaderValue = HeaderValue::from_static("noindex");
static CORS_STAR: HeaderValue = HeaderValue::from_static("*");
static CACHE_CONTROL_PRIVATE: HeaderValue = HeaderValue::from_static("no-store, private");

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT").map_or(8080, |v| v.parse().expect("Invalid PORT"));
    let v6_addr = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0));

    let state = AppState::new();

    let noindex = SetResponseHeaderLayer::overriding(ROBOTS_NAME.clone(), ROBOTS_VALUE.clone());
    let permissive_cors =
        SetResponseHeaderLayer::overriding(ACCESS_CONTROL_ALLOW_ORIGIN.clone(), CORS_STAR.clone());
    let no_cache =
        SetResponseHeaderLayer::overriding(CACHE_CONTROL.clone(), CACHE_CONTROL_PRIVATE.clone());
    let nonce_generator = axum::middleware::from_fn_with_state(state.clone(), nonce_layer);

    let app = Router::new()
        .route("/", get(home))
        .route(
            "/raw",
            any(raw).layer(ServiceBuilder::new().layer(noindex).layer(permissive_cors)),
        )
        .route("/robots.txt", get(robots))
        .fallback(not_found)
        .layer(ServiceBuilder::new().layer(no_cache).layer(nonce_generator))
        .with_state(state);

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
    ip: IpAddr,
    proto: String,
    nonce: Nonce,
}

#[derive(Template)]
#[template(path = "404.hbs", escape = "html", ext = "html")]
pub struct NotFoundPage {
    nonce: Nonce,
}

#[allow(clippy::unused_async)]
async fn home(
    IpAddress(ip): IpAddress,
    XForwardedProto(proto): XForwardedProto,
    nonce: Nonce,
    Accept(accept): Accept,
    State(state): State<AppState>,
) -> Result<Result<IndexPage, String>, Error> {
    if accept.contains("text/html") {
        let page = IndexPage {
            root_dns_name: state.root_dns_name,
            ip,
            proto,
            nonce,
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
async fn not_found(nonce: Nonce) -> NotFoundPage {
    NotFoundPage { nonce }
}

#[allow(clippy::unused_async)]
async fn robots() -> &'static str {
    concat!("User-Agent: *", "\n", "Allow: /")
}

#[derive(Clone)]
pub struct AppState {
    header: Option<Arc<HeaderName>>,
    root_dns_name: Arc<str>,
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
        Self {
            header: client_ip.map(|v| Arc::new(HeaderName::try_from(v).unwrap())),
            root_dns_name,
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
        Display::fmt(&self.0, f)
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

#[derive(Clone, Debug)]
pub struct Nonce(pub String);

#[axum::async_trait]
impl<S> FromRequestParts<S> for Nonce {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts
            .extensions
            .get()
            .cloned()
            .unwrap_or_else(|| Self("no-noncense".to_string())))
    }
}

impl Display for Nonce {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

async fn nonce_layer(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let nonce_string = random_string(32);
    req.extensions_mut().insert(Nonce(nonce_string.clone()));
    let mut resp = next.run(req).await;
    let base_dns_name = state.root_dns_name;
    let csp_str = format!(
        "default-src 'none'; object-src 'none'; img-src 'self'; \
        connect-src v4.{base_dns_name} v6.{base_dns_name} cloudflareinsights.com; \
        style-src 'nonce-{nonce_string}'; \
        script-src 'nonce-{nonce_string}' 'unsafe-inline' 'strict-dynamic' http: https:; \
        base-uri 'none';"
    );
    match HeaderValue::from_str(&csp_str) {
        Ok(csp) => {
            resp.headers_mut().insert(CONTENT_SECURITY_POLICY, csp);
        }
        Err(source) => eprintln!("ERROR: {source:?}"),
    }
    resp
}

fn random_string(length: usize) -> String {
    let rng = rand::thread_rng();
    rng.sample_iter(Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("No header found")]
    NoHeader,
    #[error("Could not extract connection info")]
    ConnectInfo,
    #[error("Could not get CSP nonce")]
    NoNonce,
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

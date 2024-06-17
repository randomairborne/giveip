#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use std::{
    fmt::{Debug, Display, Formatter},
    net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6},
    str::FromStr,
    sync::Arc,
};

use askama::Template;
use axum::{
    extract::{ConnectInfo, FromRequestParts, State},
    http::{
        header::{ACCESS_CONTROL_ALLOW_ORIGIN, CACHE_CONTROL},
        request::Parts,
        HeaderName, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
    routing::{any, get},
    Router,
};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_sombrero::{
    csp::CspNonce,
    headers::{ContentSecurityPolicy, CspSchemeSource, CspSource, XFrameOptions},
    Sombrero,
};

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

    let csp = ContentSecurityPolicy::new_empty()
        .default_src([CspSource::None])
        .base_uri([CspSource::None])
        .img_src([CspSource::SelfOrigin])
        .style_src([CspSource::Nonce])
        .connect_src([
            CspSource::Host(format!("v4.{}", state.root_dns_name)),
            CspSource::Host(format!("v6.{}", state.root_dns_name)),
            CspSource::Host("cloudflareinsights.com".to_string()),
        ])
        .script_src([
            CspSource::Nonce,
            CspSource::UnsafeInline,
            CspSource::StrictDynamic,
            CspSource::Scheme(CspSchemeSource::Https),
            CspSource::Scheme(CspSchemeSource::Https),
        ]);
    let sombrero = Sombrero::default()
        .content_security_policy(csp)
        .x_frame_options(XFrameOptions::Deny)
        .remove_strict_transport_security();

    let app = Router::new()
        .route("/", get(home))
        .route(
            "/raw",
            any(raw).layer(ServiceBuilder::new().layer(noindex).layer(permissive_cors)),
        )
        .route("/robots.txt", get(robots))
        .route("/humans.txt", get(humans))
        .fallback(not_found)
        .layer(ServiceBuilder::new().layer(no_cache).layer(sombrero))
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
    description: Arc<str>,
    proto: String,
    nonce: String,
}

#[derive(Template)]
#[template(path = "404.hbs", escape = "html", ext = "html")]
pub struct NotFoundPage {
    nonce: String,
}

#[allow(clippy::unused_async)]
async fn home(
    IpAddress(ip): IpAddress,
    XForwardedProto(proto): XForwardedProto,
    CspNonce(nonce): CspNonce,
    Accept(accept): Accept,
    State(state): State<AppState>,
) -> Result<Result<IndexPage, String>, Error> {
    if accept.contains("text/html") {
        let page = IndexPage {
            root_dns_name: state.root_dns_name,
            ip,
            description: state.description,
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
async fn not_found(nonce: String) -> NotFoundPage {
    NotFoundPage { nonce }
}

#[allow(clippy::unused_async)]
async fn robots() -> &'static str {
    include_str!("robots.txt")
}

#[allow(clippy::unused_async)]
async fn humans() -> &'static str {
    include_str!("humans.txt")
}

#[derive(Clone)]
pub struct AppState {
    header: Option<Arc<HeaderName>>,
    root_dns_name: Arc<str>,
    description: Arc<str>,
}

impl AppState {
    const DEFAULT_DESCRIPTION: &'static str = "IP address return API and site";

    #[must_use]
    /// # Panics
    /// This function can panic when its hardcoded values are invalid
    /// or the passed `client_ip_name` is not a valid header name
    pub fn new() -> Self {
        let client_ip = std::env::var("CLIENT_IP_HEADER").ok();
        let root_dns_name: Arc<str> = std::env::var("ROOT_DNS_NAME")
            .expect("No ROOT_DNS_NAME in env")
            .into();
        let description: Arc<str> = std::env::var("DESCRIPTION")
            .unwrap_or_else(|_| Self::DEFAULT_DESCRIPTION.to_string())
            .into();
        Self {
            header: client_ip.map(|v| Arc::new(HeaderName::try_from(v).unwrap())),
            root_dns_name,
            description,
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

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("No header found")]
    NoHeader,
    #[error("Could not extract connection info")]
    ConnectInfo,
    #[error("Could not get CSP nonce")]
    NoNonce(#[from] tower_sombrero::Error),
    #[error("Could not convert supplied header to string (this is a configuration issue)")]
    ToStr(#[from] axum::http::header::ToStrError),
    #[error("Could not convert supplied header to IP address (this is a configuration issue)")]
    ToAddr(#[from] std::net::AddrParseError),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let msg = match self {
            Self::NoHeader => "No header found",
            Self::ConnectInfo => "Could not extract connection info",
            Self::NoNonce(_) => "Could not get CSP nonce",
            Self::ToStr(_) => {
                "Could not convert supplied header to string (this is a configuration issue)"
            }
            Self::ToAddr(_) => {
                "Could not convert supplied header to IP address (this is a configuration issue)"
            }
        };
        (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
    }
}

#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use std::{
    fmt::{Debug, Display, Formatter},
    net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6},
    str::FromStr,
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::{ConnectInfo, FromRequestParts, State},
    http::{
        HeaderName, HeaderValue, StatusCode,
        header::{ACCESS_CONTROL_ALLOW_ORIGIN, CACHE_CONTROL},
        request::Parts,
    },
    response::{Html, IntoResponse, Response},
    routing::{any, get},
};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_sombrero::{
    Sombrero,
    csp::CspNonce,
    headers::{ContentSecurityPolicy, CspSchemeSource, CspSource, XFrameOptions},
};

mod pages;

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
        ])
        .script_src([
            CspSource::Nonce,
            CspSource::UnsafeInline,
            CspSource::StrictDynamic,
            CspSource::Scheme(CspSchemeSource::Https),
        ]);
    let sombrero = Sombrero::default()
        .content_security_policy(csp)
        .x_frame_options(XFrameOptions::Deny)
        .remove_strict_transport_security();

    let app = Router::new()
        .route("/", get(home))
        .route("/json", get(json))
        .route("/raw", any(raw).layer(noindex))
        .layer(permissive_cors)
        .route("/robots.txt", get(robots))
        .route("/humans.txt", get(humans))
        .fallback(pages::not_found)
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

pub struct IndexPage {
    root_dns_name: Arc<str>,
    ip: IpAddr,
    description: Arc<str>,
    proto: String,
    nonce: String,
}

#[derive(serde::Serialize)]
pub struct JsonIpInfo {
    pub version: u8,
    pub address: String,
}

async fn home(
    ip: IpAddress,
    XForwardedProto(proto): XForwardedProto,
    CspNonce(nonce): CspNonce,
    Accept(accept): Accept,
    State(state): State<AppState>,
) -> Result<Response, Error> {
    if accept.contains("text/html") {
        let page = IndexPage {
            root_dns_name: state.root_dns_name,
            ip: ip.0,
            description: state.description,
            proto,
            nonce,
        };
        Ok(Html(pages::index(&page).into_string()).into_response())
    } else if accept.contains("application/json") {
        Ok(json(ip).await.into_response())
    } else {
        Ok(raw(ip).await.into_response())
    }
}

async fn raw(IpAddress(ip): IpAddress) -> Result<String, Error> {
    Ok(format!("{ip}\n"))
}

async fn json(IpAddress(ip): IpAddress) -> Result<Json<JsonIpInfo>, Error> {
    let version = match ip {
        IpAddr::V4(_) => 4,
        IpAddr::V6(_) => 6,
    };
    let info = JsonIpInfo {
        version,
        address: ip.to_string(),
    };
    Ok(Json(info))
}

async fn robots() -> &'static str {
    "User-Agent: *\
     Allow: /"
}

async fn humans() -> &'static str {
    "This site was created by valkyrie_pilot. Thank you for checking it out.\n"
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

impl<S: Sync> FromRequestParts<S> for XForwardedProto {
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

impl<S: Sync> FromRequestParts<S> for Accept {
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
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}

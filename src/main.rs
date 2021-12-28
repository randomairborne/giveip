use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use std::convert::Infallible;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};
#[cfg(debug_assertions)]
use std::time::Instant;

async fn handle(addr: SocketAddr, req: Request<Body>) -> Result<Response<Body>, Infallible> {
    #[cfg(debug_assertions)]
    let st = Instant::now();

    let headers = req.headers();
    let pretty_addr = if let Some(ip) = headers.get("HTTP_X_Real_IP") {
        ip.to_str()
            .expect("invalid ip addr string header set by proxy")
            .to_string()
    } else {
        match addr {
            SocketAddr::V4(v4) => {
                format!("{}", v4.ip())
            }
            SocketAddr::V6(v6) => {
                format!("{}", v6.ip())
            }
        }
    };
    let accept = headers
        .get("Accept")
        .map_or("*/*", |x| x.to_str().expect("invalid header value"));
    let body = match accept.find("text/html") {
        Some(_) => {
            format!(
                r#"<title>Your IP is {0}</title><h1 style="font-size: 32px;font-size: 3vw;height: 100%;width: 100%;display: flex;position: fixed;align-items: center;justify-content: center; padding: 50px;">Your public IP is {0}</h1>"#,
                pretty_addr
            )
        }
        None => format!("{}\n", pretty_addr),
    };

    let r = Ok(Response::new(Body::from(body)));

    #[cfg(debug_assertions)]
    let et = Instant::now();
    #[cfg(debug_assertions)]
    let tt = et.duration_since(st);
    #[cfg(debug_assertions)]
    println!("{}ns to handle request (pt2)", tt.as_nanos());
    r
}

#[tokio::main]
async fn main() {
    let make_service = make_service_fn(move |conn: &AddrStream| {
        #[cfg(debug_assertions)]
        let st = Instant::now();

        let addr = conn.remote_addr();

        let service = service_fn(move |req| handle(addr, req));

        let r = async move { Ok::<_, Infallible>(service) };
        
        #[cfg(debug_assertions)]
        let et = Instant::now();
        #[cfg(debug_assertions)]
        let tt = et.duration_since(st);
        #[cfg(debug_assertions)]
        println!("{}ns to handle request", tt.as_nanos());
        r
    });

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let addr_v6 = SocketAddr::from(SocketAddrV6::new(
        Ipv6Addr::from([0, 0, 0, 0, 0, 0, 0, 1]),
        3000,
        0,
        0,
    ));

    let server = Server::bind(&addr)
        .serve(make_service)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for ctrl+c");
        });
    let server_v6 = Server::bind(&addr_v6)
        .serve(make_service)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for ctrl+c");
        });

    println!("Listening on http://{} and http://{}", addr, addr_v6);

    let err = match futures_util::future::join(server, server_v6).await {
        (Err(e), _) => Some(e),
        (_, Err(e)) => Some(e),
        _ => None,
    };
    if let Some(e) = err {
        eprintln!("server error: {}", e);
    }
}

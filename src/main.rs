#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use std::convert::Infallible;
use std::net::SocketAddr;
#[cfg(debug_assertions)]
use std::time::Instant;

#[allow(clippy::unused_async)]
async fn handle(addr: SocketAddr, req: Request<Body>) -> Result<Response<Body>, Infallible> {
    #[cfg(debug_assertions)]
    let st = Instant::now();

    let headers = req.headers();
    let pretty_addr = headers.get("X-Real-IP").map_or_else(
        || "(failed to get)".to_string(),
        |ip| {
            ip.to_str()
                .unwrap_or("invalid ip address string header set by proxy")
                .to_string()
        },
    );
    if req.uri() == "/raw" {
        return Ok(Response::new(Body::from(pretty_addr)));
    }
    let accept = headers
        .get("Accept")
        .map_or("*/*", |x| x.to_str().unwrap_or("invalid header value"));
    let body = match accept.find("text/html") {
        Some(_) => include_str!("index.html").to_string(),
        None => format!("{}\n", pretty_addr),
    };

    let r = Ok(Response::new(Body::from(body)));
    #[cfg(debug_assertions)]
    {
        let et = Instant::now();
        let tt = et.duration_since(st);
        println!("{}ns to handle request (pt2)", tt.as_nanos());
    }
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
        {
            let et = Instant::now();
            let tt = et.duration_since(st);
            println!("{}ns to handle request", tt.as_nanos());
        }
        r
    });

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));

    let server = Server::bind(&addr)
        .serve(make_service)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for ctrl+c");
        });

    println!("Listening on http://{}", addr);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

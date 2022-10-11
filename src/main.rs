#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use std::convert::Infallible;

#[allow(clippy::unused_async)]
async fn handle(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let headers = req.headers();
    let addr = headers.get("X-Real-IP").map_or_else(
        || "(failed to get)",
        |ip| {
            ip.to_str()
                .unwrap_or("(invalid ip address string header set by proxy)")
        },
    );
    let pretty_addr = format!("{}\n", addr);
    match req.uri().path() {
        "/raw" => return Ok(Response::new(Body::from(pretty_addr))),
        "/robots.txt" => return Ok(Response::new(Body::from("User-Agent: *\nAllow: /"))),
        _ => {}
    } 
    let accept = headers
        .get("Accept")
        .map_or("*/*", |x| x.to_str().unwrap_or("invalid header value"));
    let body = if accept.contains("text/html") {
        include_str!("index.html").to_string()
    } else {
        format!("{}\n", pretty_addr)
    };

    Ok(Response::new(Body::from(body)))
}

#[tokio::main]
async fn main() {
    let make_service = make_service_fn(move |_| {
        let service = service_fn(handle);
        async move { Ok::<_, Infallible>(service) }
    });

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 8080));

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

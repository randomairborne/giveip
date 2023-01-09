#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use std::convert::Infallible;
#[cfg(debug_assertions)]
use std::time::Instant;

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
    let pretty_addr = format!("{addr}\n");
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
        format!("{pretty_addr}\n")
    };

    Ok(Response::new(Body::from(body)))
}

#[tokio::main]
async fn main() {
    let make_service = make_service_fn(move |_| {
        #[cfg(debug_assertions)]
        let st = Instant::now();

        let service = service_fn(handle);

        let r = async move { Ok::<_, Infallible>(service) };

        #[cfg(debug_assertions)]
        {
            let et = Instant::now();
            let tt = et.duration_since(st);
            println!("{}ns to handle request", tt.as_nanos());
        }
        r
    });

    let addr = std::net::SocketAddr::from((
        [0, 0, 0, 0],
        std::env::var("PORT")
            .unwrap_or_else(|_| 8080.to_string())
            .parse::<u16>()
            .unwrap_or(8080),
    ));

    let server = Server::bind(&addr)
        .serve(make_service)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for ctrl+c");
        });

    println!("Listening on http://{addr}");

    if let Err(e) = server.await {
        eprintln!("server error: {e}");
    }
}

use axum::response::Html;
use maud::{html, PreEscaped, DOCTYPE};
use tower_sombrero::csp::CspNonce;

pub async fn not_found(CspNonce(nonce): CspNonce) -> Html<String> {
    let page = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="robots" content="noindex";
                meta name="description" content="How did we get here? This website only has a root page.";
                title { "404 Not Found" }
                style nonce=(nonce) { (PreEscaped(include_str!("style.css"))) }
            }
            body {
                .stack {
                    span { "404 not found" }
                    a href="/" .home { "Go home?" }
                }
            }
        }
    };
    Html(page.into_string())
}

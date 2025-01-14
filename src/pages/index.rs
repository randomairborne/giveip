use std::{borrow::Cow, net::IpAddr};

use maud::{html, Markup, PreEscaped, DOCTYPE};

use crate::IndexPage;

pub fn index(page: &IndexPage) -> Markup {
    let raw_opposite_dns_ext = match page.ip {
        IpAddr::V4(_) => "v6",
        IpAddr::V6(_) => "v4",
    };
    let raw_opposite_endpoint = format!(
        "{}://{}.{}/raw",
        page.proto, raw_opposite_dns_ext, page.root_dns_name
    );
    let current_url = format!("{}://{}/", page.proto, page.root_dns_name);
    let canonical = format!("https://{}", page.root_dns_name);

    let title = match page.ip {
        IpAddr::V4(ip) => Cow::Owned(format!("Your public IP is {ip}")),
        IpAddr::V6(_) => Cow::Borrowed("What's your IP?"),
    };

    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="robots" content="noindex";
                meta name="description" content=(page.description);
                link rel="preload" href=(raw_opposite_endpoint) as="fetch" crossorigin="anonymous" nonce=(page.nonce);
                meta name="description" content=(page.description);
                meta property="og:title" content=(title);
                meta property="og:url" content=(current_url);
                meta property="og:description" content=(page.description);
                link rel="canonical" href=(canonical);
                title { (title) };
                style nonce=(page.nonce) { (PreEscaped(include_str!("style.css"))) }
                meta id="js-params" data-set-title=(page.ip.is_ipv6()) data-endpoint=(raw_opposite_endpoint);
            }
            body {
                .stack {
                    @match page.ip {
                        IpAddr::V4(v4) => {
                            span { "Your public IPv4 is " code { (v4) } }
                            span #unfilled-ip-container hidden {
                                "Your public IPv6 is " code #unfilled-ip-code { "(fetching)" }
                            }
                        }
                        IpAddr::V6(v6) => {
                            span #unfilled-ip-container hidden {
                              "Your public IPv4 is " code #unfilled-ip-code { "(fetching)" }
                            }
                            span {
                                "Your public IPv6 is " code { (v6) }
                            }
                        }
                    }
                }
            }
            script nonce=(page.nonce) { (PreEscaped(include_str!("load_ip.js"))) }
        }
    }
}

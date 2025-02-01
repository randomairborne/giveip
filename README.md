# [giveip](https://giveip.io)

[![CI](https://github.com/randomairborne/giveip/actions/workflows/build.yml/badge.svg)](https://github.com/randomairborne/giveip/actions/workflows/build.yml)
[![CI](https://github.com/randomairborne/giveip/actions/workflows/check.yml/badge.svg)](https://github.com/randomairborne/giveip/actions/workflows/check.yml)
[![Website](https://img.shields.io/website?url=https%3A//giveip.io/raw)](https://giveip.io)

GiveIP is an IP address return server with no ads and no BS. \
You can also use cURL or wget giveip.io, it'll return a plaintext output as long as text/html is not in the Accept header. You can force this behavior by visiting [/raw](https://giveip.io/raw), which has permissive CORS and is free to use for all applications. There is no SLA, but if you want to use it in production, [get in touch](mailto:valk@randomairborne.dev).
To force a specific kind of IP, you can visit [v4.giveip.io/raw](https://v4.giveip.io/raw) or [v6.giveip.io/raw](https://v6.giveip.io/raw), which have DNS records for only IPv4 and IPv6 respectively.
Remember: This is at the DNS level! If you can't connect over IPv6, you WILL get an error very early on in the request process for the v6 only endpoint.
You need to make sure that you catch these errors properly, and don't fatally handle them.
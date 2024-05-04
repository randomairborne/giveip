FROM ghcr.io/randomairborne/asset-squisher:latest AS compressor

COPY /assets/ /uncompressed/

RUN asset-squisher /uncompressed/ /assets/

FROM rust:alpine AS builder

WORKDIR /build
COPY . .

RUN apk add musl-dev
RUN cargo build --release

FROM alpine:latest

WORKDIR /giveip

COPY --from=builder /build/target/release/giveip /usr/bin/giveip
COPY --from=compressor /assets/ /giveip/assets/

EXPOSE 8080

CMD ["/usr/bin/giveip"]

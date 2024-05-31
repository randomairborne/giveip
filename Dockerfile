FROM rust:alpine AS builder

WORKDIR /build
COPY . .

RUN apk add musl-dev
RUN cargo build --release

FROM alpine:latest

COPY --from=builder /build/target/release/giveip /usr/bin/giveip
EXPOSE 8080

CMD ["/usr/bin/giveip"]

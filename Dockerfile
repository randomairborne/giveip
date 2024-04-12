FROM rust:alpine AS builder

WORKDIR /build
COPY . .

RUN apk add musl-dev
RUN cargo build --release

FROM alpine

WORKDIR /giveip

COPY --from=builder /build/target/release/giveip /usr/bin/giveip
COPY /assets/ /giveip/assets/

EXPOSE 8080

ENTRYPOINT "/usr/bin/giveip"

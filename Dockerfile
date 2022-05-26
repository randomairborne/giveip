FROM rust:alpine AS builder

WORKDIR /build
COPY . .

RUN cargo build --release

FROM alpine

COPY --from=builder /build/target/release/giveip /usr/bin/giveip

EXPOSE 3000

CMD [ "/usr/bin/giveip" ]
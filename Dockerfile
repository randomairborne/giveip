FROM rust AS builder

WORKDIR /build
COPY . .

RUN apt install openssl-dev
RUN cargo build --release

FROM debian:slim

COPY --from=builder /build/target/release/giveip /usr/bin/giveip

EXPOSE 8080

CMD [ "/usr/bin/giveip" ]

ARG LLVMTARGETARCH
FROM --platform=${BUILDPLATFORM} ghcr.io/randomairborne/cross-cargo-${LLVMTARGETARCH}:latest AS builder

ARG LLVMTARGETARCH

WORKDIR /build

COPY . .

RUN cargo build --release --target ${LLVMTARGETARCH}-unknown-linux-musl

FROM scratch

COPY --from=builder /build/target/release/giveip /usr/bin/giveip
EXPOSE 8080

ENTRYPOINT ["/usr/bin/giveip"]

FROM alpine:3.23 AS certs
RUN apk add --no-cache ca-certificates

FROM scratch
COPY --from=certs /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY target/x86_64-unknown-linux-musl/release/lattice /lattice

ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV LATTICE_PORT=7400
ENV LATTICE_DB_URL=sqlite:///data/lattice.db
ENV LATTICE_STORAGE_DIR=/data/storage

EXPOSE 7400
VOLUME ["/data"]

ENTRYPOINT ["/lattice"]

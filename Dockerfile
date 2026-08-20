# syntax=docker/dockerfile:1.7
FROM rust:1.97.1-bookworm@sha256:0e2bcaef56d041a486784e54104a81aebe0da44bd03019bd70bc0401e42e4a97 AS builder

ARG TRUNK_VERSION=0.21.14
ARG GEONAMES_CITIES_SHA256=9c144cdd80cd9ae454bc2bae034d52b84da5d612b77148c9a97a213cf68479e4
ARG GEONAMES_ADMIN1_SHA256=590651498043f674accda2b7f46d21286cda0e290b02f8561c5005eee9a5448c
ARG GEONAMES_ADMIN2_SHA256=dfb1b884b7094070b3539be18076080aa75f66fd82e19e78acaf584b190856c4

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl python3 \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add wasm32-unknown-unknown \
    && cargo install trunk --locked --version "${TRUNK_VERSION}"

WORKDIR /source
COPY . .
RUN cargo fetch --locked \
    && cd crates/oracle-studio-ui \
    && trunk build index.html --release --locked=true --dist /product \
    && python3 /source/scripts/csp-hashes.py /product/index.html /product/oracle-csp.conf

RUN install -d /product/catalog/geonames \
    && curl --fail --location --proto '=https' --tlsv1.2 \
        --output /product/catalog/geonames/cities500.zip \
        https://download.geonames.org/export/dump/cities500.zip \
    && curl --fail --location --proto '=https' --tlsv1.2 \
        --output /product/catalog/geonames/admin1CodesASCII.txt \
        https://download.geonames.org/export/dump/admin1CodesASCII.txt \
    && curl --fail --location --proto '=https' --tlsv1.2 \
        --output /product/catalog/geonames/admin2Codes.txt \
        https://download.geonames.org/export/dump/admin2Codes.txt \
    && printf '%s  %s\n' \
        "${GEONAMES_CITIES_SHA256}" /product/catalog/geonames/cities500.zip \
        "${GEONAMES_ADMIN1_SHA256}" /product/catalog/geonames/admin1CodesASCII.txt \
        "${GEONAMES_ADMIN2_SHA256}" /product/catalog/geonames/admin2Codes.txt \
        | sha256sum --check --strict \
    && printf '{"retrieved_at":"2026-08-20T02:02:47Z","cities500_sha256":"%s","admin1_sha256":"%s","admin2_sha256":"%s"}\n' \
        "${GEONAMES_CITIES_SHA256}" "${GEONAMES_ADMIN1_SHA256}" "${GEONAMES_ADMIN2_SHA256}" \
        > /product/catalog/geonames/manifest.json

FROM nginxinc/nginx-unprivileged:1.29.1-alpine3.22@sha256:27985295bdb22a1ef8f712863210bd5877c0f3006494a593e86b3fe0fa55467e AS runtime

COPY --from=builder /product/ /usr/share/nginx/html/
COPY --from=builder /product/oracle-csp.conf /etc/nginx/conf.d/oracle-csp.conf
COPY docker/nginx.conf /etc/nginx/nginx.conf
USER root
RUN rm /usr/share/nginx/html/oracle-csp.conf

USER 101:101
EXPOSE 8080
STOPSIGNAL SIGQUIT

# syntax=docker/dockerfile:1.7
FROM rust:1.97.1-bookworm@sha256:0e2bcaef56d041a486784e54104a81aebe0da44bd03019bd70bc0401e42e4a97 AS builder

ARG TRUNK_VERSION=0.21.14

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add wasm32-unknown-unknown \
    && cargo install trunk --locked --version "${TRUNK_VERSION}"

WORKDIR /source
COPY . .
RUN cargo fetch --locked \
    && cd crates/oracle-studio-ui \
    && trunk build index.html --release --locked=true --dist /product \
    && python3 /source/scripts/csp-hashes.py /product/index.html /product/oracle-csp.conf

FROM builder AS demo-builder

RUN cargo run --locked --quiet -p oracle-studio-demo -- generate /demo-generated \
    && cd crates/oracle-studio-ui \
    && trunk build demo.html --release --locked=true --dist /demo-product \
    && install -d /demo-product/demo \
    && install -m 0644 \
        /demo-generated/oracle-studio-demo.oracle-vault \
        /demo-product/demo/oracle-studio-demo.oracle-vault \
    && python3 /source/scripts/csp-hashes.py \
        /demo-product/index.html /demo-product/oracle-csp.conf

FROM builder AS builder-with-catalog

RUN python3 /source/scripts/geonames.py download --source-dir /tmp/geonames-source \
    && python3 /source/scripts/geonames.py stage \
        --source-dir /tmp/geonames-source --output /product

FROM nginxinc/nginx-unprivileged:1.29.1-alpine3.22@sha256:27985295bdb22a1ef8f712863210bd5877c0f3006494a593e86b3fe0fa55467e AS acceptance-runtime

COPY --from=builder /product/ /usr/share/nginx/html/
COPY --from=builder /product/oracle-csp.conf /etc/nginx/conf.d/oracle-csp.conf
COPY docker/nginx.conf /etc/nginx/nginx.conf
USER root
RUN rm /usr/share/nginx/html/oracle-csp.conf

USER 101:101
EXPOSE 8080
STOPSIGNAL SIGQUIT

FROM nginxinc/nginx-unprivileged:1.29.1-alpine3.22@sha256:27985295bdb22a1ef8f712863210bd5877c0f3006494a593e86b3fe0fa55467e AS demo-runtime

COPY --from=demo-builder /demo-product/ /usr/share/nginx/html/
COPY --from=demo-builder /demo-product/oracle-csp.conf /etc/nginx/conf.d/oracle-csp.conf
COPY docker/nginx.conf /etc/nginx/nginx.conf
USER root
RUN rm /usr/share/nginx/html/oracle-csp.conf

USER 101:101
EXPOSE 8080
STOPSIGNAL SIGQUIT

FROM nginxinc/nginx-unprivileged:1.29.1-alpine3.22@sha256:27985295bdb22a1ef8f712863210bd5877c0f3006494a593e86b3fe0fa55467e AS runtime

COPY --from=builder-with-catalog /product/ /usr/share/nginx/html/
COPY --from=builder-with-catalog /product/oracle-csp.conf /etc/nginx/conf.d/oracle-csp.conf
COPY docker/nginx.conf /etc/nginx/nginx.conf
USER root
RUN rm /usr/share/nginx/html/oracle-csp.conf

USER 101:101
EXPOSE 8080
STOPSIGNAL SIGQUIT

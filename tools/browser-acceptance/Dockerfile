# syntax=docker/dockerfile:1.7

ARG DEBIAN_IMAGE=debian:trixie-slim@sha256:38a76d01668772e381ad2826d876627c89e7133e2f8a0f5d567306798b0f2a16

FROM ${DEBIAN_IMAGE} AS browser-download

ARG CHROME_VERSION=152.0.7977.54
ARG CHROME_SHA256=88af83664e1e5f79dc1c1378d0699b98dddd69690a748addf4ccbe322bfacedf
ARG CHROMEDRIVER_SHA256=66f3984d666de8455706de8968bd7afb6ad3fa70c1a08084bba8235e146617a6

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl unzip \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /download
RUN curl --fail --location --proto '=https' --tlsv1.2 \
        --output chrome-linux64.zip \
        "https://storage.googleapis.com/chrome-for-testing-public/${CHROME_VERSION}/linux64/chrome-linux64.zip" \
    && echo "${CHROME_SHA256}  chrome-linux64.zip" | sha256sum --check --strict \
    && curl --fail --location --proto '=https' --tlsv1.2 \
        --output chromedriver-linux64.zip \
        "https://storage.googleapis.com/chrome-for-testing-public/${CHROME_VERSION}/linux64/chromedriver-linux64.zip" \
    && echo "${CHROMEDRIVER_SHA256}  chromedriver-linux64.zip" | sha256sum --check --strict \
    && unzip -q chrome-linux64.zip -d /opt/chrome-for-testing \
    && unzip -q chromedriver-linux64.zip -d /opt/chrome-for-testing

FROM ${DEBIAN_IMAGE}

ARG CHROME_VERSION=152.0.7977.54

LABEL org.opencontainers.image.title="Oracle Studio browser acceptance" \
      org.opencontainers.image.description="Pinned Chrome for Testing for the static browser-local product" \
      org.opencontainers.image.version="${CHROME_VERSION}" \
      org.opencontainers.image.source="https://github.com/gracee3/oracle-studio"

# Chrome and Python are deliberately isolated from the product runtime image.
RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        ca-certificates \
        fonts-liberation \
        gsettings-desktop-schemas \
        libasound2 \
        libatk-bridge2.0-0 \
        libatk1.0-0 \
        libatspi2.0-0 \
        libcairo2 \
        libcups2 \
        libdbus-1-3 \
        libdrm2 \
        libexpat1 \
        libgbm1 \
        libglib2.0-0 \
        libnspr4 \
        libnss3 \
        libpango-1.0-0 \
        libudev1 \
        libx11-6 \
        libxcb1 \
        libxcomposite1 \
        libxdamage1 \
        libxext6 \
        libxfixes3 \
        libxkbcommon0 \
        libxrandr2 \
        python3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 studio-browser \
    && useradd --uid 10001 --gid 10001 --create-home --shell /usr/sbin/nologin studio-browser

COPY --from=browser-download /opt/chrome-for-testing /opt/chrome-for-testing
COPY acceptance.py /opt/oracle-studio/acceptance.py
COPY demo_acceptance.py /opt/oracle-studio/demo_acceptance.py

ENV CHROME_BIN=/opt/chrome-for-testing/chrome-linux64/chrome \
    CHROMEDRIVER_BIN=/opt/chrome-for-testing/chromedriver-linux64/chromedriver \
    PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

USER 10001:10001
ENTRYPOINT ["python3", "/opt/oracle-studio/acceptance.py"]

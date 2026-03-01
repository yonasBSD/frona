# ============================
# PROD BUILD STAGES
# ============================

# Stage 1: frontend
FROM node:22-slim AS frontend-builder
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json* ./
RUN npm ci
COPY frontend/ .
RUN npm run build

# Stage 2: cargo-chef planner
FROM rust:1.89-bookworm AS planner
RUN --mount=type=bind,source=build/pkgs/builder-rust-cargo.txt,target=/tmp/cargo.txt \
  for line in $(grep -vE '^\s*(#|$)' /tmp/cargo.txt); do \
    crate=${line%%=*}; version=${line#*=}; \
    cargo install "$crate" --version "$version"; \
  done
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY engine/ engine/
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: backend builder
FROM rust:1.89-bookworm AS backend-builder
RUN --mount=type=bind,source=build/pkgs/builder-rust-apt.txt,target=/tmp/packages.txt \
  apt-get update && apt-get install -y --no-install-recommends \
  $(grep -vE '^\s*(#|$)' /tmp/packages.txt | tr '\n' ' ') \
  && rm -rf /var/lib/apt/lists/*
RUN --mount=type=bind,source=build/pkgs/builder-rust-cargo.txt,target=/tmp/cargo.txt \
  for line in $(grep -vE '^\s*(#|$)' /tmp/cargo.txt); do \
    crate=${line%%=*}; version=${line#*=}; \
    cargo install "$crate" --version "$version"; \
  done
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY engine/ engine/
RUN cargo build --release -p frona

# Stage 4: python packages
FROM python:3.12-slim-bookworm AS python-builder
RUN --mount=type=bind,source=build/pkgs/builder-python-apt.txt,target=/tmp/packages.txt \
  apt-get update && apt-get install -y --no-install-recommends \
  $(grep -vE '^\s*(#|$)' /tmp/packages.txt | tr '\n' ' ') \
  && rm -rf /var/lib/apt/lists/*
COPY build/pkgs/builder-python-pip.txt .
RUN pip install --no-cache-dir --prefix=/install -r builder-python-pip.txt

# ============================
# DEV TARGET
# ============================

FROM rust:1.89-bookworm AS dev
RUN --mount=type=bind,source=build/pkgs/dev-apt.txt,target=/tmp/packages.txt \
  apt-get update \
  && apt-get install -y --no-install-recommends \
     $(grep -vE '^\s*(#|$)|^nodejs=' /tmp/packages.txt | tr '\n' ' ') \
  && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
  && apt-get install -y --no-install-recommends \
     $(grep '^nodejs=' /tmp/packages.txt) \
  && rm -rf /var/lib/apt/lists/*
COPY build/pkgs/builder-python-pip.txt /tmp/requirements.txt
RUN pip install --no-cache-dir --break-system-packages -r /tmp/requirements.txt
RUN --mount=type=bind,source=build/pkgs/dev-rust-cargo.txt,target=/tmp/cargo.txt \
  for line in $(grep -vE '^\s*(#|$)' /tmp/cargo.txt); do \
    crate=${line%%=*}; version=${line#*=}; \
    cargo install "$crate" --version "$version"; \
  done

RUN groupadd -g 1000 frona && useradd -u 1000 -g frona -m -s /bin/bash frona
RUN mkdir -p /app /app/target /app/frontend/node_modules /app/data \
    && chown -R frona:frona /app /usr/local/cargo
USER frona
WORKDIR /app
COPY --chown=frona:frona frontend/package.json frontend/package-lock.json* frontend/
RUN cd frontend && npm install
CMD ["bash", "-c", "cargo watch -w engine/ -w Cargo.toml -w Cargo.lock --delay 3 -x 'run -p frona' & cd frontend && npm run dev & wait"]

# ============================
# PROD TARGET (default)
# ============================

FROM python:3.12-slim-bookworm AS prod
RUN --mount=type=bind,source=build/pkgs/prod-apt.txt,target=/tmp/packages.txt \
  apt-get update && apt-get install -y --no-install-recommends \
  $(grep -vE '^\s*(#|$)' /tmp/packages.txt | tr '\n' ' ') \
  && rm -rf /var/lib/apt/lists/*

RUN groupadd -g 1000 frona && useradd -u 1000 -g frona -d /app -s /bin/bash frona
RUN mkdir -p /app /data && chown -R frona:frona /app /data
WORKDIR /app

COPY --from=python-builder /install /usr/local
COPY --chown=frona:frona --from=backend-builder /app/target/release/frona /app/frona
COPY --chown=frona:frona --from=frontend-builder /app/frontend/out /app/static
COPY --chown=frona:frona resources /app/share

ENV FRONA_SERVER_STATIC_DIR=/app/static
ENV FRONA_SERVER_DATA_DIR=/app/data
ENV FRONA_SERVER_PORT=3001
ENV FRONA_STORAGE_SHARED_CONFIG_DIR=/app/share
ENV FRONA_LOG_LEVEL=info

VOLUME /app/data
EXPOSE 3001
USER frona
CMD ["/app/frona"]

# syntax=docker/dockerfile:1.7
#
# One image for the whole service: the `api`, `worker` and `migrate` binaries
# and the built web app, which the API serves from the same origin
# (DESIGN.md §13.5). The default command is the API; `worker` and `migrate`
# are run by naming them (see docker-compose.yml).
#
# Nothing secret is built in. Every setting, the database connection and
# APP_SECRET first of all, comes from the environment at run time
# (.env.example lists them).

# ---- The web app ------------------------------------------------------------
FROM node:26-bookworm-slim AS web
WORKDIR /src
# The workspace manifests first, so the dependency layer is reused until one
# of them changes.
COPY package.json package-lock.json tsconfig.base.json ./
COPY apps/web/package.json apps/web/
COPY apps/mobile/package.json apps/mobile/
COPY packages/shared/package.json packages/shared/
COPY packages/api-client/package.json packages/api-client/
RUN --mount=type=cache,target=/root/.npm npm ci --no-audit --no-fund
COPY apps/web apps/web
COPY packages packages
RUN npm run build:web

# ---- The service ------------------------------------------------------------
FROM rust:1.97-bookworm AS backend
WORKDIR /src
COPY backend backend
# The build embeds the wording and the list of languages (backend/build.rs).
COPY packages/shared/wording packages/shared/wording
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/backend/target \
    cd backend \
    && cargo build --release --locked --bin api --bin worker --bin migrate \
    && mkdir -p /out \
    && cp target/release/api target/release/worker target/release/migrate /out/

# ---- The image --------------------------------------------------------------
# A libc and CA certificates, no shell and no package manager. The binaries
# bring their own TLS (rustls), so nothing else is needed.
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=backend /out/api /out/worker /out/migrate /usr/local/bin/
COPY --from=web /src/apps/web/dist /srv/web
# Listen on every interface, since the container's own address is what the
# host maps; serve the web app built above.
ENV BIND_ADDR=0.0.0.0:8080 \
    WEB_DIR=/srv/web \
    RUST_LOG=info
EXPOSE 8080
USER nonroot
CMD ["/usr/local/bin/api"]

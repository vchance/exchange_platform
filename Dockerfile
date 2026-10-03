# syntax=docker/dockerfile:1.7@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e
#
# One image for the whole service: the `api`, `worker` and `migrate` binaries
# and the built web app, which the API serves from the same origin
# (DESIGN.md §13.5). The default command is the API; `worker` and `migrate`
# are run by naming them (see docker-compose.yml), as is `replay-deletions`
# after a restore (docs/operations.md, "Replaying deletions").
#
# Nothing secret is built in. Every setting, the database connection and
# APP_SECRET first of all, comes from the environment at run time
# (.env.example lists them).
#
# Every image is pinned by digest, with its tag kept beside it, so a build
# uses exactly the bytes that were reviewed. Dependabot proposes new digests
# weekly (.github/dependabot.yml).

# ---- The web app ------------------------------------------------------------
FROM node:26-bookworm-slim@sha256:662933cf47f013bc8e4beb31a6116448427a82057ba7c42c97e4c5ba766504c2 AS web
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
FROM rust:1.98-bookworm@sha256:93ce27a88655056a51dbdd8f5f2d7ddc071c7b0070fb288a37b5a285fc83971e AS backend
WORKDIR /src
COPY backend backend
# The build embeds the wording and the list of languages (backend/build.rs).
COPY packages/shared/wording packages/shared/wording
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/backend/target \
    cd backend \
    && cargo build --release --locked --bin api --bin worker --bin migrate --bin replay-deletions \
    && mkdir -p /out \
    && cp target/release/api target/release/worker target/release/migrate target/release/replay-deletions /out/

# ---- The image --------------------------------------------------------------
# A libc and CA certificates, no shell and no package manager. The binaries
# bring their own TLS (rustls), so nothing else is needed.
FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f
COPY --from=backend /out/api /out/worker /out/migrate /out/replay-deletions /usr/local/bin/
COPY --from=web /src/apps/web/dist /srv/web
# Listen on every interface, since the container's own address is what the
# host maps; serve the web app built above.
ENV BIND_ADDR=0.0.0.0:8080 \
    WEB_DIR=/srv/web \
    RUST_LOG=info
EXPOSE 8080
USER nonroot
CMD ["/usr/local/bin/api"]

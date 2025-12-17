# Aetheras Rust Template

Starter kit for building Rust services at Aetheras. It scaffolds an HTTP API plus Temporal worker, wires in PostgreSQL, and ships a ready-to-run local stack via Docker Compose. Common tooling (Justfile tasks, formatting, linting, and cargo-generate hooks) is preconfigured so you can start shipping features immediately.

## What you get

- HTTP server and worker binaries sharing a core crate
- Temporal + UI, PostgreSQL, and Adminer for local development
- Dockerfile and `compose.yaml` for build/test stacks
- `just` commands for build, run, worker, and dev stack orchestration

## Generate a new project

Install [cargo-generate](https://github.com/cargo-generate/cargo-generate)

```sh
cargo install cargo-generate
```

```sh
cargo generate --git git@github.com:aetheras-io/rust-template.git --name my-service
```

If you prefer HTTPS instead of SSH, use a personal access token (with `repo` scope) and supply it in the URL:

```sh
cargo generate --git https://github.com/aetheras-io/rust-template.git --name my-service
```

ORG := "{{org}}"
PROJECT := "{{ project-name | kebab_case }}"
REPO := "https://github.com" / ORG / PROJECT
ROOT_DIR := justfile_directory()
OUTPUT_DIR := ROOT_DIR / "target"
SEM_VER := `awk -F' = ' '$1=="version"{print $2;exit;}' ./Cargo.toml`
DOCKER_REGISTRY := "{{docker_repo}}/{{org}}"

# Paths to built binaries (macOS/Linux)
BIN := OUTPUT_DIR / "debug" / PROJECT
R_BIN := OUTPUT_DIR / "release" / PROJECT

set dotenv-load
alias b := build
alias l := local
alias ld := local-down

default:
    @just --choose

semver:
    @echo {{ "{{" }}SEM_VER{{ "}}" }}

###########################################################
### Build
###

build:
    cargo build --manifest-path bin/{{project-name}}/Cargo.toml --bin {{ project-name | kebab_case }}

build-release:
    cargo build --release --manifest-path bin/{{project-name}}/Cargo.toml --bin {{ project-name | kebab_case }}

build-docker:
    docker build --platform=linux/amd64 \
        --progress plain \
        -f ./Dockerfile \
        -t {{ "{{" }}DOCKER_REGISTRY{{ "}}" }}/{{ project-name | kebab_case }}:latest \
        -t {{ "{{" }}DOCKER_REGISTRY{{ "}}" }}/{{ project-name | kebab_case }}:v{{ "{{" }}SEM_VER{{ "}}" }} .

docker-push:
    docker push {{ "{{" }}DOCKER_REGISTRY{{ "}}" }}/{{ project-name | kebab_case }}:latest
    docker push {{ "{{" }}DOCKER_REGISTRY{{ "}}" }}/{{ project-name | kebab_case }}:v{{ "{{" }}SEM_VER{{ "}}" }}
    docker image prune -f

###########################################################
### Platform Development
###
export COMPOSE_PROJECT_NAME := PROJECT

schema: build 
    {{ "{{" }}BIN{{ "}}" }} generate-schema > target/schema.graphql

dev: build
    RUST_BACKTRACE=1 {{ "{{" }}BIN{{ "}}" }} mono

dev-http: build
    RUST_BACKTRACE=1 {{ "{{" }}BIN{{ "}}" }} http

dev-worker: build
    RUST_BACKTRACE=1 {{ "{{" }}BIN{{ "}}" }} worker

local:
    docker compose -f compose.yaml up -d {{ project-name | kebab_case }}

local-down:
    docker compose -f compose.yaml down -v

local-pg:
    #!/usr/bin/env bash
    set -euo pipefail

    docker compose -f compose.yaml up -d postgres adminer

    echo "Waiting for postgres"
    while ! docker exec postgres pg_isready >/dev/null 2>&1; do sleep 0.1; done
    # #HACK for some reason even if pg_isready, postgis container isn't accepting connections
    sleep 3

local-temporal: local-pg
	docker compose -f {{ "{{" }}ROOT_DIR{{ "}}" }}/compose.yaml up -d temporal temporal-ui temporal-admin-tools; \
	sleep 3

local-dev: local-temporal dev

###########################################################
### Testing

BASE_URL := "http://host.docker.internal:3030"

test-core-all:
    # Runs all core tests, including ignored ones (sqlx and temporal). Temporal test uses ephemeral server feature.
    # Requires: Postgres on localhost:5432. Temporal will download/start via SDK core when feature enabled.
    cargo test -p {{ project-name | snake_case }}-core --features temporal-tests -- --include-ignored

stress-test:
    #!/usr/bin/env bash
    set -euo pipefail
    export BASE_URL={{ "{{" }}BASE_URL{{ "}}" }}
    docker compose -f {{ "{{" }}ROOT_DIR{{ "}}" }}/compose.yaml --profile stresstest run --rm k6-stress

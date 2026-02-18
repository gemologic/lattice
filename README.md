# lattice

[![uses nix](https://img.shields.io/badge/uses-nix-%237EBAE4)](https://nixos.org/)
![rust](https://img.shields.io/badge/Rust-1.95%2B-orange.svg)

Lattice is a local-first project board for human + agent execution:

- Vue UI for board/spec/questions/webhooks
- REST API under `/api/v1`
- MCP server under `/mcp`
- SSE stream for live updates

The server is a single Rust binary that embeds the built UI assets.

## Quick Start

Run:

```bash
lattice
```

Open `http://127.0.0.1:7400`.

## First 5 Minutes

### 1. Create a project

```bash
API=http://127.0.0.1:7400/api/v1

curl -sS -X POST "$API/projects" \
  -H 'content-type: application/json' \
  -d '{"name":"Lattice Demo","slug":"LATTICE-DEMO","goal":"Ship the first workflow"}' | jq
```

Project slugs must use uppercase letters, digits, and `-`.

### 2. Create a task

```bash
curl -sS -X POST "$API/projects/LATTICE-DEMO/tasks" \
  -H 'content-type: application/json' \
  -d '{"title":"Build docs","description":"Write README and usage docs"}' | jq
```

### 3. Open the board

Go to `http://127.0.0.1:7400/LATTICE-DEMO`.

## Auth

If `LATTICE_TOKEN` is set, every request must include:

```bash
-H "Authorization: Bearer $LATTICE_TOKEN"
```

If `LATTICE_TOKEN` is unset, auth is disabled and the server logs a startup warning.

Rate limiting runs before auth checks, so repeated invalid auth attempts are throttled.

## Configuration

All options are available as CLI flags or env vars:

| Env Var                          | Default                 | Description                   |
| -------------------------------- | ----------------------- | ----------------------------- |
| `LATTICE_PORT`                   | `7400`                  | HTTP port                     |
| `LATTICE_DB_URL`                 | `sqlite://./lattice.db` | Database DSN                  |
| `LATTICE_TOKEN`                  | unset                   | Bearer auth token             |
| `LATTICE_LOG_LEVEL`              | `info`                  | Tracing filter level          |
| `LATTICE_STORAGE_DIR`            | `./storage`             | Attachment storage directory  |
| `LATTICE_MAX_FILE_SIZE`          | `10485760`              | Max upload bytes              |
| `LATTICE_MAX_REQUEST_BODY_BYTES` | `12582912`              | Global max request body bytes |

### Rate Limiting Env Vars

| Env Var                                   | Default |
| ----------------------------------------- | ------- |
| `LATTICE_RATE_LIMIT_READ_PER_MIN`         | `240`   |
| `LATTICE_RATE_LIMIT_READ_BURST`           | `60`    |
| `LATTICE_RATE_LIMIT_WRITE_PER_MIN`        | `120`   |
| `LATTICE_RATE_LIMIT_WRITE_BURST`          | `30`    |
| `LATTICE_RATE_LIMIT_ATTACHMENT_PER_MIN`   | `30`    |
| `LATTICE_RATE_LIMIT_ATTACHMENT_BURST`     | `10`    |
| `LATTICE_RATE_LIMIT_WEBHOOK_TEST_PER_MIN` | `20`    |
| `LATTICE_RATE_LIMIT_WEBHOOK_TEST_BURST`   | `5`     |
| `LATTICE_RATE_LIMIT_MCP_PER_MIN`          | `80`    |
| `LATTICE_RATE_LIMIT_MCP_BURST`            | `20`    |
| `LATTICE_RATE_LIMIT_SSE_CONNECT_PER_MIN`  | `40`    |
| `LATTICE_RATE_LIMIT_SSE_CONNECT_BURST`    | `10`    |
| `LATTICE_RATE_LIMIT_SSE_MAX_PER_IDENTITY` | `10`    |
| `LATTICE_RATE_LIMIT_SSE_MAX_GLOBAL`       | `400`   |

Recommended prebuilt profiles (`dev`, `small-team`, `strict`) are in `docs/usage.md`.

See exact CLI help:

```bash
lattice --help
```

## Interfaces

- UI: `/`
- REST: `/api/v1/...`
- MCP (streamable HTTP): `/mcp`
- SSE:
  - `/api/v1/events`
  - `/api/v1/events?project=SLUG&project=OTHER`
  - `/api/v1/projects/:slug/events`

## Docs

- VitePress source: `docs/`
- Getting started page: `docs/getting-started.md`
- Usage guide: `docs/usage.md`
- MCP guide: `docs/mcp.md`

Run docs locally:

```bash
direnv exec . bunx vitepress dev docs
```

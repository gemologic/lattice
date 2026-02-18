# Usage Guide

This is the shortest path to productive usage.

## Run

```bash
lattice
```

Server defaults to `http://127.0.0.1:7400`.

## Core URLs

- Project list: `http://127.0.0.1:7400/`
- Board: `http://127.0.0.1:7400/<PROJECT_SLUG>`
- Spec: `http://127.0.0.1:7400/<PROJECT_SLUG>/spec`
- Questions: `http://127.0.0.1:7400/<PROJECT_SLUG>/questions`
- Webhooks: `http://127.0.0.1:7400/<PROJECT_SLUG>/settings/webhooks`

## API Workflow (curl)

Set base URL:

```bash
API=http://127.0.0.1:7400/api/v1
```

If auth is enabled:

```bash
AUTH=(-H "Authorization: Bearer $LATTICE_TOKEN")
```

If auth is disabled:

```bash
AUTH=()
```

## Rate Limits

Rate limiting is evaluated before auth checks. This means repeated invalid or missing bearer tokens are throttled before `401` handling.

Default env vars:

| Env Var                                   | Default    |
| ----------------------------------------- | ---------- |
| `LATTICE_RATE_LIMIT_READ_PER_MIN`         | `240`      |
| `LATTICE_RATE_LIMIT_READ_BURST`           | `60`       |
| `LATTICE_RATE_LIMIT_WRITE_PER_MIN`        | `120`      |
| `LATTICE_RATE_LIMIT_WRITE_BURST`          | `30`       |
| `LATTICE_RATE_LIMIT_ATTACHMENT_PER_MIN`   | `30`       |
| `LATTICE_RATE_LIMIT_ATTACHMENT_BURST`     | `10`       |
| `LATTICE_RATE_LIMIT_WEBHOOK_TEST_PER_MIN` | `20`       |
| `LATTICE_RATE_LIMIT_WEBHOOK_TEST_BURST`   | `5`        |
| `LATTICE_RATE_LIMIT_MCP_PER_MIN`          | `80`       |
| `LATTICE_RATE_LIMIT_MCP_BURST`            | `20`       |
| `LATTICE_RATE_LIMIT_SSE_CONNECT_PER_MIN`  | `40`       |
| `LATTICE_RATE_LIMIT_SSE_CONNECT_BURST`    | `10`       |
| `LATTICE_RATE_LIMIT_SSE_MAX_PER_IDENTITY` | `10`       |
| `LATTICE_RATE_LIMIT_SSE_MAX_GLOBAL`       | `400`      |
| `LATTICE_MAX_REQUEST_BODY_BYTES`          | `12582912` |

### Recommended Profiles

`dev` profile, single user/local experimentation:

```bash
export LATTICE_RATE_LIMIT_READ_PER_MIN=240
export LATTICE_RATE_LIMIT_READ_BURST=60
export LATTICE_RATE_LIMIT_WRITE_PER_MIN=120
export LATTICE_RATE_LIMIT_WRITE_BURST=30
export LATTICE_RATE_LIMIT_ATTACHMENT_PER_MIN=30
export LATTICE_RATE_LIMIT_ATTACHMENT_BURST=10
export LATTICE_RATE_LIMIT_WEBHOOK_TEST_PER_MIN=20
export LATTICE_RATE_LIMIT_WEBHOOK_TEST_BURST=5
export LATTICE_RATE_LIMIT_MCP_PER_MIN=80
export LATTICE_RATE_LIMIT_MCP_BURST=20
export LATTICE_RATE_LIMIT_SSE_CONNECT_PER_MIN=40
export LATTICE_RATE_LIMIT_SSE_CONNECT_BURST=10
export LATTICE_RATE_LIMIT_SSE_MAX_PER_IDENTITY=10
export LATTICE_RATE_LIMIT_SSE_MAX_GLOBAL=400
export LATTICE_MAX_REQUEST_BODY_BYTES=25165824
```

`small-team` profile, shared internal instance:

```bash
export LATTICE_RATE_LIMIT_READ_PER_MIN=180
export LATTICE_RATE_LIMIT_READ_BURST=45
export LATTICE_RATE_LIMIT_WRITE_PER_MIN=60
export LATTICE_RATE_LIMIT_WRITE_BURST=20
export LATTICE_RATE_LIMIT_ATTACHMENT_PER_MIN=20
export LATTICE_RATE_LIMIT_ATTACHMENT_BURST=8
export LATTICE_RATE_LIMIT_WEBHOOK_TEST_PER_MIN=10
export LATTICE_RATE_LIMIT_WEBHOOK_TEST_BURST=4
export LATTICE_RATE_LIMIT_MCP_PER_MIN=40
export LATTICE_RATE_LIMIT_MCP_BURST=12
export LATTICE_RATE_LIMIT_SSE_CONNECT_PER_MIN=30
export LATTICE_RATE_LIMIT_SSE_CONNECT_BURST=8
export LATTICE_RATE_LIMIT_SSE_MAX_PER_IDENTITY=6
export LATTICE_RATE_LIMIT_SSE_MAX_GLOBAL=250
export LATTICE_MAX_REQUEST_BODY_BYTES=12582912
```

`strict` profile, exposed or noisy environments:

```bash
export LATTICE_RATE_LIMIT_READ_PER_MIN=90
export LATTICE_RATE_LIMIT_READ_BURST=20
export LATTICE_RATE_LIMIT_WRITE_PER_MIN=20
export LATTICE_RATE_LIMIT_WRITE_BURST=6
export LATTICE_RATE_LIMIT_ATTACHMENT_PER_MIN=6
export LATTICE_RATE_LIMIT_ATTACHMENT_BURST=3
export LATTICE_RATE_LIMIT_WEBHOOK_TEST_PER_MIN=3
export LATTICE_RATE_LIMIT_WEBHOOK_TEST_BURST=1
export LATTICE_RATE_LIMIT_MCP_PER_MIN=12
export LATTICE_RATE_LIMIT_MCP_BURST=4
export LATTICE_RATE_LIMIT_SSE_CONNECT_PER_MIN=10
export LATTICE_RATE_LIMIT_SSE_CONNECT_BURST=3
export LATTICE_RATE_LIMIT_SSE_MAX_PER_IDENTITY=3
export LATTICE_RATE_LIMIT_SSE_MAX_GLOBAL=120
export LATTICE_MAX_REQUEST_BODY_BYTES=8388608
```

When throttled, the API returns `429` plus `Retry-After` and `x-ratelimit-*` headers.

### Create a project

```bash
curl -sS -X POST "$API/projects" "${AUTH[@]}" \
  -H 'content-type: application/json' \
  -d '{"name":"Roadmap","slug":"ROADMAP","goal":"Deliver v1"}' | jq
```

### Create a task

```bash
curl -sS -X POST "$API/projects/ROADMAP/tasks" "${AUTH[@]}" \
  -H 'content-type: application/json' \
  -d '{"title":"Set up board","status":"backlog","priority":"medium"}' | jq
```

### Move a task

Task refs accept UUID or display key (`ROADMAP-1`):

```bash
curl -sS -X POST "$API/projects/ROADMAP/tasks/ROADMAP-1/move" "${AUTH[@]}" \
  -H 'content-type: application/json' \
  -d '{"status":"in_progress"}' | jq
```

### Ask and resolve open questions

```bash
curl -sS -X POST "$API/projects/ROADMAP/tasks/ROADMAP-1/questions" "${AUTH[@]}" \
  -H 'content-type: application/json' \
  -d '{"question":"Use SSE or polling?","context":"Realtime sync"}' | jq
```

Use the returned `question.id`:

```bash
curl -sS -X PATCH "$API/projects/ROADMAP/tasks/ROADMAP-1/questions/<QUESTION_ID>" "${AUTH[@]}" \
  -H 'content-type: application/json' \
  -d '{"answer":"Use SSE for browser sync"}' | jq
```

### Upload and download attachments

Upload:

```bash
curl -sS -X POST "$API/projects/ROADMAP/tasks/ROADMAP-1/attachments" "${AUTH[@]}" \
  -F "file=@./screenshot.png" | jq
```

Download via canonical file route:

```bash
curl -L -o download.bin "$API/files/<ATTACHMENT_ID>" "${AUTH[@]}"
```

### Webhooks

Create:

```bash
curl -sS -X POST "$API/projects/ROADMAP/webhooks" "${AUTH[@]}" \
  -H 'content-type: application/json' \
  -d '{
    "name":"team alerts",
    "url":"https://example.com/webhook",
    "platform":"generic",
    "events":["task.created","task.moved","question.created","question.resolved"],
    "secret":"replace-me"
  }' | jq
```

Test:

```bash
curl -sS -X POST "$API/projects/ROADMAP/webhooks/<WEBHOOK_ID>/test" "${AUTH[@]}" -i
```

## Live Events (SSE)

All projects:

```bash
curl -N "$API/events" "${AUTH[@]}"
```

Single project:

```bash
curl -N "$API/projects/ROADMAP/events" "${AUTH[@]}"
```

## MCP

MCP endpoint is `/mcp` (streamable HTTP). Tools include:

- `lattice_list_projects`
- `lattice_create_project`
- `lattice_list_tasks`
- `lattice_create_task`
- `lattice_move_task`
- `lattice_update_spec_section`
- `lattice_ask_question`
- `lattice_answer_question`
- `lattice_board_summary`

Agent identity is taken from `MCP-Client` and used in audit fields.

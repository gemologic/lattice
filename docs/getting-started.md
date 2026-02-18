# Getting Started

## 1. Configure the database (required)

Set `LATTICE_DB_URL` for your backend:

```bash
export LATTICE_DB_URL='sqlite://./lattice.db'
# or
export LATTICE_DB_URL='postgres://lattice:secret@127.0.0.1:5432/lattice'
```

Notes:

- SQLite is the default if `LATTICE_DB_URL` is unset.
- SQLite gets `?mode=rwc` automatically when no query params are provided.
- Migrations run automatically on startup for both SQLite and PostgreSQL.

## 2. Start Lattice

```bash
lattice
```

Open `http://127.0.0.1:7400`.

## 3. Create your first project

```bash
API=http://127.0.0.1:7400/api/v1

curl -sS -X POST "$API/projects" \
  -H 'content-type: application/json' \
  -d '{"name":"Lattice Demo","slug":"LATTICE-DEMO","goal":"Ship first workflow"}' | jq
```

Slugs must use uppercase letters, digits, and `-`.

## 4. Create a task

```bash
curl -sS -X POST "$API/projects/LATTICE-DEMO/tasks" \
  -H 'content-type: application/json' \
  -d '{"title":"Create docs","description":"Set up VitePress docs"}' | jq
```

## 5. Open the board

Visit `http://127.0.0.1:7400/LATTICE-DEMO`.

## Auth

If `LATTICE_TOKEN` is set, include:

```bash
-H "Authorization: Bearer $LATTICE_TOKEN"
```

If `LATTICE_TOKEN` is unset, auth is disabled and the server logs a warning.

## Related Runtime Settings

- `LATTICE_STORAGE_DIR` controls attachment file storage.
- `LATTICE_PORT` controls the HTTP listen port.

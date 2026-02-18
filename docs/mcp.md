# MCP

Lattice exposes an MCP Streamable HTTP service at:

```text
/mcp
```

Agent identity is read from the `MCP-Client` header and written into audit fields.

## Tool Set

- `lattice_list_projects`
- `lattice_create_project`
- `lattice_list_tasks`
- `lattice_create_task`
- `lattice_move_task`
- `lattice_update_spec_section`
- `lattice_ask_question`
- `lattice_answer_question`
- `lattice_board_summary`

## Runtime Notes

- If `LATTICE_TOKEN` is enabled, MCP callers must send `Authorization: Bearer <token>`.
- Mutating tools emit SSE events and webhook events just like REST mutations.
- `lattice_board_summary` is tuned for low token orientation context.

## Client Setup

Base URL used in all examples:

```text
http://127.0.0.1:7400/mcp
```

### Codex CLI

Add to `~/.codex/config.toml`:

```toml
[mcp_servers.lattice]
url = "http://127.0.0.1:7400/mcp"
```

If auth is enabled, include:

```toml
[mcp_servers.lattice]
url = "http://127.0.0.1:7400/mcp"
bearer_token_env_var = "LATTICE_TOKEN"
```

### Cursor

Add to `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "lattice": {
      "url": "http://127.0.0.1:7400/mcp"
    }
  }
}
```

If auth is enabled, add `Authorization` in the server headers:

```json
{
  "mcpServers": {
    "lattice": {
      "url": "http://127.0.0.1:7400/mcp",
      "headers": {
        "Authorization": "Bearer <LATTICE_TOKEN>"
      }
    }
  }
}
```

### VS Code

Add to `.vscode/mcp.json`:

```json
{
  "servers": {
    "lattice": {
      "type": "http",
      "url": "http://127.0.0.1:7400/mcp"
    }
  }
}
```

## Quick Verify

1. Start Lattice with `lattice`.
2. Reload your MCP client.
3. Ask the client to run `lattice_list_projects`.

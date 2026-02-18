# Lattice UI

This Vue app is built and embedded into the Rust server binary.

## Local UI-only Dev

```bash
direnv exec . bun install --cwd ui
direnv exec . bun run --cwd ui dev
```

## Production Build

```bash
direnv exec . bun run --cwd ui build
```

Output goes to `ui/dist`.

During Rust builds, `build.rs` will run `bun install` + `bun run build` automatically unless `LATTICE_SKIP_UI_BUILD=1` is set.

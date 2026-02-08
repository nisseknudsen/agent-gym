# Games

Each game lives under `games/<name>/`.

## Structure

```
games/<name>/
  sim/          # Required: game logic crate (lib + optional MCP server bin)
  viewer/       # Optional: visual client (e.g., Bevy app)
  web/          # Optional: web server for viewer
```

## Adding a new game

1. Create `games/<name>/sim/` — implement the `Game` trait from `sim_core`
2. Add it to root `Cargo.toml` workspace members
3. (Optional) Add `viewer/` and `web/` crates

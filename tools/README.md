# tools

Local developer tools. The Context7 CLI is dev-only; the Alpaca CLI fetched
into `binaries/` is the app's sidecar.

## `context7/ctx7`

Query [Context7](https://context7.com) docs from a terminal instead of guessing
an API from memory.

```bash
tools/context7/ctx7 libs rusqlite
tools/context7/ctx7 docs /rusqlite/rusqlite --topic "WAL mode" --tokens 3000
tools/context7/ctx7 "How do I stream a response with Axum?"
```

- `search <query>` / bare `<query>` — find library ids.
- `libs <name>` — shortlist matching libraries.
- `docs <library-id>` — fetch a topic-specific docs slice.
- `--json` prints the raw response for scripts.
- `CONTEXT7_API_KEY` overrides the built-in key.

The API key is hardcoded on purpose for this single-user workspace. Rotate it
before making the file public.

## `fetch-alpaca-cli.sh`

Downloads the pinned Alpaca CLI (`v0.0.15`) into `binaries/`, verifies the
release checksum, and stages it for the bundle.

```bash
sh tools/fetch-alpaca-cli.sh
```

Runs automatically from `npm run build` and `npm run dev`. Idempotent: an
existing binary is left alone, so delete it to force a re-fetch. `binaries/` is
gitignored — the artifact is fetched, never committed.

It ships as a **resource** (`bundle.resources`), not an `externalBin`: Tauri
installs `externalBin` to `/usr/bin`, which would put `alpaca` on the system
`PATH` and shadow any copy the user installed. As a resource it lands in
`usr/lib/traiding/bin/alpaca` and is invisible outside the app.

Only `src/cli.rs` runs it, and only from the agent's read-only tools. Trading,
streaming and backtests use the in-process `apca` client, not a subprocess.

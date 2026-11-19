# traiding

Single-binary Alpaca trading terminal. Rust (Tauri 2) + React 19 + Astryx +
Lightweight Charts. No HTTP server.

## Read first

| If you are… | Read |
| --- | --- |
| new to the repo | [`memory/index.md`](.agents/memory/index.md) |
| touching Rust | [`.agents/skills/rust/SKILL.md`](.agents/skills/rust/SKILL.md) |
| touching the UI | [`.agents/skills/ui/SKILL.md`](.agents/skills/ui/SKILL.md) |
| using Astryx | [`.agents/skills/astryx/SKILL.md`](.agents/skills/astryx/SKILL.md) |
| writing any user-facing text | [`.agents/skills/copywriting/SKILL.md`](.agents/skills/copywriting/SKILL.md) |
| using the repo tools | [`tools/README.md`](tools/README.md) |
| verifying the UI | [`.agents/memory/verification.md`](.agents/memory/verification.md) |

## Non-negotiables

1. **One process.** No axum, no listening port. UI ⇄ Rust is Tauri IPC only.
2. **Credentials never reach the webview and the app never writes them to disk.**
   Only a masked key (`PKTE…3456`) is returned. Preferences and market data do
   go to SQLite; no secret ever does.
3. **Wire names are camelCase.** Rust request structs use
   `#[serde(rename_all = "camelCase")]`; `ui/src/api.ts` must send exactly that.
4. **Read Astryx props from `node_modules/.../dist/*.d.ts`**, never from memory.
5. **Zero warnings.** `cargo check --all-targets` and `npm run typecheck` both clean.
6. **Short copy.** Labels ≤ 2 words, helper text ≤ 12 words.
7. **Local data is SQLite.** One database in Tauri's app-local data directory,
   bundled `rusqlite`, WAL. Storage failures warn; they never block live data.

## Commands

```bash
npm run dev            # tauri dev, hot reload
npm run build          # release binary + bundles
npm run typecheck      # tsc --noEmit
cargo test             # includes IPC contract tests
sh install.sh --from-source --prefix /tmp/t
```

## Verify after any change

```bash
cargo check --all-targets && cargo test && npm run typecheck
```

A green build does **not** prove rendering. See
[`memory/gotchas.md`](.agents/memory/gotchas.md) before declaring UI work done.

## Layout

```
src/            Rust: main, lib, commands (IPC), stream (websockets), backtest,
                agent (OpenRouter chat), cli (bundled Alpaca CLI), state, storage
ui/src/         React: App, api.ts (IPC boundary), components/, agent/, styles.css
specs/          Realtime, backtesting and desktop UI implementation specs
tools/          Context7 CLI + UI IPC mock + Alpaca CLI fetch
.agents/        skills + memory wiki
icons/          app icon set
install.sh      curl installer
```

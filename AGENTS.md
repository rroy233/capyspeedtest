# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## Project Overview

CapySpeedtest is a Tauri desktop application (React + Rust) for batch speed testing proxy nodes. It manages Mihomo kernels, GeoIP databases, parses subscription links, and measures node performance (TCP ping, site ping, download/upload throughput, NAT type detection).

## Common Commands

```bash
# Frontend development (Vite dev server)
bun run dev

# Build frontend for production
bun run build

# Run frontend tests
bun run test                # run all tests once
bun run test:watch          # watch mode

# Tauri commands
bun run tauri dev           # run full Tauri app in dev mode
bun run tauri build         # build production Tauri app
```

## Architecture

### Frontend (React + TypeScript)

- **Framework**: React 19, React Router 6, TailwindCSS v4, HeroUI
- **Routing**: Defined in `src/App.tsx` via `AppRoutes()` — routes: `/` (Home), `/results/table`, `/results/chart`, `/settings`, `/about`
- **API Layer**: `src/api/settings.ts` wraps all Tauri IPC calls. **Mock mode**: When not running inside Tauri runtime (`hasTauriRuntime()` check), all functions return mock data so development works outside the desktop shell.
- **State Types**: `src/types/settings.ts` defines all shared TypeScript interfaces
- **History**: `src/utils/history.ts` persists speed test results to localStorage with query/filter/export (CSV) capabilities

### Backend (Rust + Tauri)

- **`src-tauri/src/main.rs`**: Entry point; registers all Tauri commands and `AppState`
- **`src-tauri/src/models.rs`**: All `serde` serializable types (kernel/IP update status, node info, speed test results, etc.)
- **`src-tauri/src/commands.rs`**: Tauri command handlers (`#[tauri::command]`). Manages `AppState` (kernel version, installed versions, IP database version). Handles: settings snapshot, kernel version listing/selection, IP database refresh, subscription parsing, client update check/download, batch speedtest execution with progress events
- **`src-tauri/src/services.rs`**: Business logic — GitHub API calls, kernel/IP database download with retry, subscription parsing, batch speedtest orchestration, file persistence to app data dir. **Tests are inline** (`#[cfg(test)]` mod)

### Tauri IPC Pattern

Frontend calls `invokeTauri<T>("command_name", args)` which routes to Rust `#[tauri::command]` handlers. Speedtest progress is streamed back via `window.emit("speedtest://progress", event)`. Frontend subscribes via `listen<SpeedTestProgressEvent>("speedtest://progress", callback)`.

## Key File Locations

- `src-tauri/src/main.rs` — Rust entry and command registration
- `src-tauri/src/commands.rs` — Tauri command handlers and `AppState`
- `src-tauri/src/services.rs` — Core business logic and tests
- `src-tauri/src/models.rs` — Shared Rust types
- `src/api/settings.ts` — Frontend API wrapper with mock fallback
- `src/types/settings.ts` — Frontend TypeScript types
- `src/pages/HomePage.tsx` — Speedtest task configuration and execution UI

## Testing

- Frontend tests: Vitest with jsdom environment (`vitest.config.ts`). Setup file: `src/setupTests.ts`. Test files match `*.test.ts` or `*.test.tsx` pattern.
- Backend tests: Inline in `src-tauri/src/services.rs` with `#[cfg(test)] mod tests`
- Mock mode in frontend API means page-level tests can run without Tauri runtime

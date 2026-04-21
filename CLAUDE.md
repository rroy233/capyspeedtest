# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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
- **Routing**: Defined in `src/App.tsx` via `AppRoutes()` — routes: `/` (Home), `/results`, `/settings`, `/about`
- **API Layer**: `src/api/` contains modular IPC wrappers (`settings.ts`, `speedtest.ts`, `subscription.ts`, `updates.ts`, `database.ts`). **Mock mode**: When not running inside Tauri runtime (`hasTauriRuntime()` check), all functions return mock data so development works outside the desktop shell.
- **State Types**: `src/types/settings.ts` defines all shared TypeScript interfaces
- **History**: `src/utils/history.ts` persists speed test results to localStorage with query/filter/export (CSV) capabilities

### Backend (Rust + Tauri)

The Rust backend is organized into focused modules:

**`src-tauri/src/commands/`** — Tauri command handlers (submodules):
| Submodule | Responsibility |
|---|---|
| `settings` | Kernel version listing/selection, IP database refresh |
| `subscription` | Subscription parsing commands |
| `updates` | Client update check/download, scheduled update checks |
| `speedtest` | Batch speedtest execution with progress events |
| `database_cmd` | Batch CRUD, scatter chart data |
| `data_mgmt` | Data directory info, export, clear |
| `fs_utils` | File system utilities (directory stats, zip packaging) |

**`src-tauri/src/services/`** — Core business logic (submodules):
| Submodule | Responsibility |
|---|---|
| `state` | Runtime state persistence (JSON file read/write) |
| `subscription` | Subscription parsing (Base64 decode, URI parse, node filter) |
| `kernel` | Mihomo kernel download, spawn, config generation, lifecycle |
| `geoip` | MMDB database read, real IP→geo location lookup |
| `speedtest` | Real network speedtest (TCP Ping, HTTP download/upload, NAT detection) |
| `updater` | GitHub version check, client update package download |
| `system_proxy` | Windows system proxy auto-detection |

**`src-tauri/src/database/`** — SQLite operations:
- `batch.rs` — Batch metadata operations
- `result.rs` — Speedtest result CRUD

**`src-tauri/src/models.rs`** — All `serde` serializable types (kernel/IP update status, node info, speed test results, etc.)

**`src-tauri/src/main.rs`** — Entry point; sets up logging and registers all Tauri commands.

### Tauri IPC Pattern

Frontend calls `invokeTauri<T>("command_name", args)` which routes to Rust `#[tauri::command]` handlers. Speedtest progress is streamed back via `window.emit("speedtest://progress", event)`. Frontend subscribes via `listen<SpeedTestProgressEvent>("speedtest://progress", callback)`.

## Key File Locations

### Frontend
- `src/App.tsx` — App entry, routing, navigation tabs, theme/alert providers
- `src/api/settings.ts` — Settings-related Tauri IPC wrappers
- `src/pages/HomePage.tsx` — Speedtest task configuration and execution UI
- `src/pages/ResultsPage.tsx` — Results display with chart and table views
- `src/components/speedtest/` — Speedtest UI components (NodeListItem, RegionCard, SpeedTestConfigForm, etc.)

### Backend
- `src-tauri/src/main.rs` — Entry point, logging setup
- `src-tauri/src/commands/mod.rs` — Command module registry and AppState definition
- `src-tauri/src/services/mod.rs` — Service module registry and re-exports
- `src-tauri/src/models.rs` — Shared Rust types
- `src-tauri/src/database/mod.rs` — Database module registry

## Testing

- Frontend tests: Vitest with jsdom environment (`vitest.config.ts`). Setup file: `src/setupTests.ts`. Test files match `*.test.ts` or `*.test.tsx` pattern.
- Backend tests: Inline in service modules with `#[cfg(test)]` blocks
- Mock mode in frontend API means page-level tests can run without Tauri runtime

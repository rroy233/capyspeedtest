# Contributing to CapySpeedtest

> [中文版本](#贡献指南)

Thank you for your interest in contributing to CapySpeedtest! Please read our Code of Conduct before participating.

## How to Contribute

There are many ways to contribute:

- **Report bugs** — Found something broken? [Open a bug report](https://github.com/rroy233/capyspeedtest/issues/new?template=bug_report.yml).
- **Suggest features** — Have an idea? [Submit a feature request](https://github.com/rroy233/capyspeedtest/issues/new?template=feature_request.yml).
- **Improve docs** — Spot a typo or missing info? [Report a doc issue](https://github.com/rroy233/capyspeedtest/issues/new?template=doc_issue.yml).
- **Contribute code** — Fix bugs or implement features via pull requests.

## Development Setup

### Prerequisites

- Node.js 18+ and Bun
- Rust 1.85+ and Cargo
- [Tauri 2.0 prerequisites](https://v2.tauri.app/start/prerequisites/)

### Quick Start

```bash
# Install dependencies
bun install

# Start development server with hot reload
bun run dev
```

### Useful Commands

| Command | Description |
|---------|-------------|
| `bun run dev` | Start dev server (hot reload) |
| `bun run build` | Production build |
| `bun run test` | Run unit tests |
| `bun run test:watch` | Run unit tests in watch mode |
| `bun run tauri dev` | Run full Tauri app in dev mode |
| `bun run tauri build` | Build production Tauri app |

For Rust backend:

```bash
cd src-tauri
cargo fmt        # Format Rust code
cargo clippy     # Run linter
cargo test       # Run tests
```

## Code Style

- **Frontend**: TypeScript strict mode, Vitest for unit tests
- **Backend**: `cargo fmt` for formatting, `cargo clippy` for linting
- **Tauri 2.0**: Command names must use camelCase

Run all checks before submitting:

```bash
bun run build
bun run test
cd src-tauri && cargo fmt --check && cargo clippy && cargo test
```

## Pull Request Guidelines

1. **Open an issue first** for new features — PRs for features that are not a good fit may be closed.
2. **Fork and branch** — Create a feature branch from `main` (e.g., `feat/my-feature` or `fix/issue-123`).
3. **Keep PRs focused** — One feature or fix per PR. Avoid unrelated changes.
4. **Follow the PR template** — Fill in the summary, related issue, and checklist.

### PR Checklist

- [ ] `bun run build` passes
- [ ] `bun run test` passes
- [ ] `cargo clippy` passes (if Rust code changed)

### Commit Convention

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(speedtest): add batch speedtest with progress streaming
fix(results): resolve results disappearing on page refresh
docs(readme): update installation instructions
ci: add stale issue workflow
chore(deps): update dependencies
```

## Questions?

- [Open a question](https://github.com/rroy233/capyspeedtest/issues/new?template=question.yml)
- [GitHub Discussions](https://github.com/rroy233/capyspeedtest/discussions)

---

# 贡献指南

> [English Version](#contributing-to-capyspeedtest)

感谢你对 CapySpeedtest 的贡献兴趣！参与之前请阅读我们的行为准则。

## 如何贡献

你可以通过多种方式参与贡献：

- **报告 Bug** — 发现问题？[提交 Bug 报告](https://github.com/rroy233/capyspeedtest/issues/new?template=bug_report.yml)。
- **建议功能** — 有想法？[提交功能请求](https://github.com/rroy233/capyspeedtest/issues/new?template=feature_request.yml)。
- **改进文档** — 发现错误或缺失？[报告文档问题](https://github.com/rroy233/capyspeedtest/issues/new?template=doc_issue.yml)。
- **贡献代码** — 通过 Pull Request 修复 Bug 或实现新功能。

## 开发环境搭建

### 前提条件

- Node.js 18+ 和 Bun
- Rust 1.85+ 和 Cargo
- [Tauri 2.0 开发环境](https://v2.tauri.app/start/prerequisites/)

### 快速开始

```bash
# 安装依赖
bun install

# 启动开发服务器（热重载）
bun run dev
```

### 常用命令

| 命令 | 说明 |
|------|------|
| `bun run dev` | 启动开发服务器（热重载） |
| `bun run build` | 构建生产版本 |
| `bun run test` | 运行单元测试 |
| `bun run test:watch` | 运行单元测试（监视模式） |
| `bun run tauri dev` | 以开发模式运行完整 Tauri 应用 |
| `bun run tauri build` | 构建生产版 Tauri 应用 |

Rust 后端命令：

```bash
cd src-tauri
cargo fmt        # 格式化 Rust 代码
cargo clippy     # 运行 Clippy 检查
cargo test       # 运行测试
```

## 代码规范

- **前端**：TypeScript 严格模式、Vitest 单元测试
- **后端**：使用 `cargo fmt` 格式化、`cargo clippy` 检查
- **Tauri 2.0**：命令名必须使用 camelCase

提交前运行所有检查：

```bash
bun run build
bun run test
cd src-tauri && cargo fmt --check && cargo clippy && cargo test
```

## Pull Request 指南

1. **先开 Issue 讨论** — 新功能请先开 Issue，不适合项目方向的 PR 可能会被关闭。
2. **Fork 并创建分支** — 从 `main` 创建功能分支（如 `feat/my-feature` 或 `fix/issue-123`）。
3. **保持 PR 专注** — 每个 PR 只做一件事，避免无关改动。
4. **遵循 PR 模板** — 填写概述、关联 Issue 和检查清单。

### PR 检查清单

- [ ] `bun run build` 通过
- [ ] `bun run test` 通过
- [ ] `cargo clippy` 通过（如修改了 Rust 代码）

### 提交信息规范

我们使用 [Conventional Commits](https://www.conventionalcommits.org/)：

```
feat(speedtest): add batch speedtest with progress streaming
fix(results): resolve results disappearing on page refresh
docs(readme): update installation instructions
ci: add stale issue workflow
chore(deps): update dependencies
```

## 有疑问？

- [提问](https://github.com/rroy233/capyspeedtest/issues/new?template=question.yml)
- [GitHub 讨论区](https://github.com/rroy233/capyspeedtest/discussions)

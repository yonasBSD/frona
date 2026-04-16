# Release Process

## Quick Start

```bash
mise run release patch                    # 0.1.0 → 0.1.1
mise run release minor                    # 0.1.0 → 0.2.0
mise run release major                    # 0.1.0 → 1.0.0
mise run release minor alpha              # 0.1.0 → 0.2.0-ALPHA1
mise run release alpha                    # 0.2.0-ALPHA1 → 0.2.0-ALPHA2
mise run release beta                     # 0.2.0-ALPHA2 → 0.2.0-BETA1
mise run release stable                   # 0.2.0-BETA1 → 0.2.0
mise run release 1.0.0-RC1               # (any) → 1.0.0-RC1
```

## CLI Usage

```
build/release.sh <command> [pre-release] [--dry-run] [--skip-docker] [--skip-tests]
```

### Commands

| Command | Description |
|---------|-------------|
| `patch` / `minor` / `major` | Bump to a stable release |
| `minor alpha` / `major beta` | Bump + start a pre-release series |
| `alpha` / `beta` / `rc` | Increment existing pre-release number |
| `stable` | Promote current pre-release to stable |
| `<version>` | Set an explicit version (e.g., `1.0.0-RC1`) |

### Flags

| Flag | Description |
|------|-------------|
| `--dry-run` | Preview changes without modifying anything |
| `--skip-docker` | Version bump + git tag only, no Docker build |
| `--skip-tests` | Skip `cargo test` before releasing |

## Pre-release Format

Pre-release versions use FreeBSD-style uppercase tags without dot separators:

- `0.2.0-ALPHA1`, `0.2.0-BETA2`, `0.2.0-RC1`

Commands are lowercase for ergonomics (`mise run release alpha`).

## Pre-release Workflow

```bash
# Start a pre-release series
mise run release minor alpha              # 0.1.0 → 0.2.0-ALPHA1

# Iterate within a pre-release tag
mise run release alpha                    # 0.2.0-ALPHA1 → 0.2.0-ALPHA2

# Advance to the next stage
mise run release beta                     # 0.2.0-ALPHA2 → 0.2.0-BETA1
mise run release rc                       # 0.2.0-BETA1 → 0.2.0-RC1

# Promote to stable
mise run release stable                   # 0.2.0-RC1 → 0.2.0
```

## Docker Tagging

- **Stable** `0.2.0` → `ghcr.io/fronalabs/frona:v0.2.0` + `:latest`
- **Pre-release** `0.2.0-ALPHA1` → `ghcr.io/fronalabs/frona:v0.2.0-ALPHA1` only (no `:latest`)

## Version Sources

The script updates these files in sync:

- `Cargo.toml` — `version` under `[workspace.package]`
- `web/package.json` — `"version"` field
- `web/package-lock.json` — root `"version"` + `packages[""]` version

## Safety Checks

1. Working tree must be clean (no uncommitted changes)
2. Stable releases must be from the `main` branch
3. Git tag must not already exist
4. Tests must pass (unless `--skip-tests`)

## Git Operations

- Commit message: `release: v{version}`
- Annotated tag: `v{version}`
- Auto-pushes commit and tag to `origin`

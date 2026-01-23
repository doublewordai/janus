# sqlx-pool-router Development Guide

This document provides guidance for working on the `sqlx-pool-router` crate.

## Repository Overview

**sqlx-pool-router** is a lightweight Rust library for routing database operations to different SQLx PostgreSQL connection pools based on read/write operations.

**Key files:**
- `src/lib.rs` - Main library with `PoolProvider` trait, `DbPools`, and `TestDbPools`
- `examples/` - Example usage (basic.rs for production, testing.rs for tests)
- `.github/workflows/` - CI/CD pipelines
- `Cargo.toml` - Package metadata and dependencies

## Development Workflow

### Running Tests Locally

Tests require PostgreSQL:

```bash
# Start PostgreSQL via Docker
docker run -d \
  -p 5432:5432 \
  -e POSTGRES_PASSWORD=password \
  --name sqlx-pool-router-test-db \
  postgres:16

# Set DATABASE_URL (use 'postgres' database for sqlx::test)
export DATABASE_URL=postgresql://postgres:password@localhost:5432/postgres

# Run tests
cargo test --all-features

# Cleanup
docker stop sqlx-pool-router-test-db && docker rm sqlx-pool-router-test-db
```

**Important:** Use `DATABASE_URL` pointing to `postgres` database (not `test`), as `#[sqlx::test]` creates isolated test databases dynamically.

### Code Quality Checks

```bash
# Format code
cargo fmt

# Run linter
cargo clippy --all-features -- -D warnings

# Build documentation
cargo doc --no-deps --open
```

### Testing Examples

```bash
# Test basic example
export DATABASE_URL=postgresql://postgres:password@localhost:5432/postgres
cargo run --example basic

# Test testing example
cargo run --example testing
```

## Architecture

### Core Types

**`PoolProvider` trait:**
- `.read()` - Returns pool for read operations (may route to replica)
- `.write()` - Returns pool for write operations (always primary)
- Implemented by `PgPool`, `DbPools`, and `TestDbPools`

**`DbPools` struct:**
- Production type with optional replica pool
- `DbPools::new(primary)` - Single pool configuration
- `DbPools::with_replica(primary, replica)` - Dual pool configuration

**`TestDbPools` struct:**
- Test helper that enforces read/write separation
- Creates read-only replica pool from same database
- Use with `#[sqlx::test]` to catch routing bugs

### Testing Strategy

**Unit tests (`src/lib.rs`):**
- Test basic functionality with `#[sqlx::test]`
- Verify read/write routing with separate databases
- Test `TestDbPools` read-only enforcement

**Example tests:**
- `examples/basic.rs` - Production usage patterns
- `examples/testing.rs` - Test patterns with `TestDbPools`

**Important testing note:** PostgreSQL TEMP tables are per-connection, not per-pool. When testing with pools, avoid relying on TEMP table visibility across different connections.

## Making Changes

### Adding Features

1. Add implementation to `src/lib.rs`
2. Write unit tests with `#[sqlx::test]`
3. Update documentation comments
4. Add examples if needed
5. Run all checks: `cargo test --all-features && cargo fmt && cargo clippy --all-features`

### Updating Documentation

Documentation lives in three places:
- **Doc comments** in `src/lib.rs` (published to docs.rs)
- **README.md** (shown on crates.io and GitHub)
- **Examples** in `examples/` (runnable code)

When updating docs:
1. Update doc comments with correct crate name (`sqlx_pool_router`)
2. Keep README.md in sync
3. Verify examples compile: `cargo build --examples`
4. Build docs locally: `cargo doc --no-deps --open`

### Commit Conventions

Use [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` - New features
- `fix:` - Bug fixes
- `docs:` - Documentation changes
- `test:` - Test additions/modifications
- `refactor:` - Code refactoring
- `perf:` - Performance improvements
- `chore:` - Build/tooling changes

Example: `feat: add connection timeout configuration`

Always include co-author line:
```
feat: add new feature

Description of the feature.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>
```

## CI/CD Pipeline

### GitHub Actions Workflows

**CI Workflow** (`.github/workflows/ci.yml`):
- Runs on push to main and PRs
- Test job: Runs `cargo test --all-features` with PostgreSQL 16
- Lint job: Runs `cargo fmt --check` and `cargo clippy`

**Release Workflow** (`.github/workflows/release.yml`):
- Runs on published GitHub releases
- Tests before publishing
- Publishes to crates.io using `CARGO_REGISTRY_TOKEN`

**Release Please** (`.github/workflows/release-please.yaml`):
- Automatically creates release PRs based on conventional commits
- Updates CHANGELOG.md
- Bumps version in Cargo.toml

### Publishing Process

Managed by Release Please:

1. Merge PRs to main (using conventional commits)
2. Release Please creates/updates a release PR
3. Review and merge the release PR
4. Release Please creates a GitHub release
5. Release workflow publishes to crates.io
6. docs.rs automatically builds documentation

**Manual publishing** (if needed):
```bash
cargo publish
```

Requires `CARGO_REGISTRY_TOKEN` environment variable or `cargo login`.

## Common Tasks

### Renaming the Crate

If renaming the crate:

1. Update `Cargo.toml` name and repository URL
2. Update all imports in `src/lib.rs` and `examples/`
3. Update README.md references
4. Run `cargo fmt` (fixes import ordering)
5. Run tests to verify
6. Rename GitHub repository to match
7. Update git remote: `git remote set-url origin <new-url>`

### Fixing Test Failures

**Formatting errors:**
```bash
cargo fmt
```

**Connection issues:**
- Verify PostgreSQL is running: `docker ps`
- Check DATABASE_URL points to `postgres` database
- Ensure port 5432 is available

**TEMP table issues:**
- Remember: TEMP tables are per-connection, not per-pool
- Avoid testing cross-connection TEMP table visibility
- Use regular tables or transactions for multi-connection tests

## Troubleshooting

### "database test does not exist"
- DATABASE_URL should point to `postgres` database, not `test`
- `#[sqlx::test]` creates isolated databases automatically

### "relation does not exist" in tests
- TEMP tables are per-connection
- Use `TestDbPools.write()` for both creation and queries of TEMP tables
- Or use transactions to keep queries on same connection

### CI fails but local tests pass
- Check PostgreSQL version matches (postgres:16)
- Verify DATABASE_URL format
- Run `cargo fmt` before committing

### docs.rs build fails
- Verify `[package.metadata.docs.rs]` in Cargo.toml
- Check all doc comment code examples compile
- Test locally: `cargo doc --all-features --no-deps`

## Real-World Usage

This library is used in production by:
- [outlet-postgres](https://github.com/doublewordai/outlet-postgres) - HTTP logging middleware
- [fusillade](https://github.com/doublewordai/fusillade) - LLM batching daemon
- [dwctl](https://github.com/doublewordai/control-layer) - Observability platform

When making changes, consider impact on these downstream consumers.

## Getting Help

- **Issues**: https://github.com/doublewordai/sqlx-pool-router/issues
- **Discussions**: https://github.com/doublewordai/sqlx-pool-router/discussions
- **Crate docs**: https://docs.rs/sqlx-pool-router

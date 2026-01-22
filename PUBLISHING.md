# Publishing Guide for janus

This document explains how to publish `janus` to crates.io.

## Prerequisites

1. **Crates.io Account**: Create an account at https://crates.io
2. **API Token**: Generate a token at https://crates.io/me
3. **GitHub Repository**: Create a repository at https://github.com/doublewordai/janus
4. **GitHub Secret**: Add your crates.io token as `CARGO_REGISTRY_TOKEN` in GitHub repository secrets

## Setup GitHub Repository

```bash
cd /Users/peter/titan/janus

# Initialize git if not already done
git init

# Add remote
git remote add origin git@github.com:doublewordai/janus.git

# Add all files
git add .

# Commit
git commit -m "Initial commit: janus v0.1.0"

# Push to GitHub
git push -u origin main
```

## Publishing Workflow

### Automatic Publishing (Recommended)

The crate is configured to publish automatically when you create a release tag:

```bash
# Update version in Cargo.toml if needed
# Make sure all tests pass
cargo test --all-features

# Commit any changes
git add Cargo.toml
git commit -m "Bump version to 0.1.0"

# Create and push a tag
git tag v0.1.0
git push origin v0.1.0
```

The GitHub Action will:
1. Run tests
2. Run clippy
3. Check formatting
4. Verify the tag matches Cargo.toml version
5. Publish to crates.io

### Manual Publishing

If you prefer to publish manually:

```bash
# Login to crates.io (one time)
cargo login

# Ensure tests pass
cargo test --all-features

# Publish
cargo publish
```

## Version Bumping

When releasing a new version:

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md` (create if doesn't exist)
3. Commit changes
4. Create git tag matching the version
5. Push tag to trigger release

## Updating Dependent Crates

After publishing, update dependent crates:

### outlet-postgres

```toml
[dependencies]
janus = "0.1"
```

### fusillade

```toml
[dependencies]
janus = "0.1"
```

### control-layer (dwctl)

```toml
[dependencies]
janus = "0.1"
```

## Post-Publication Checklist

- [ ] Verify package appears on crates.io
- [ ] Check documentation renders correctly on docs.rs
- [ ] Update dependent crates to use published version
- [ ] Create GitHub release with changelog
- [ ] Announce on relevant channels if appropriate

## Troubleshooting

### "Tag version does not match Cargo.toml version"

Make sure the git tag matches the version in Cargo.toml:
- Tag: `v0.1.0`
- Cargo.toml: `version = "0.1.0"`

### "CARGO_REGISTRY_TOKEN not set"

Add the secret in GitHub repository settings:
1. Go to Settings > Secrets and variables > Actions
2. Click "New repository secret"
3. Name: `CARGO_REGISTRY_TOKEN`
4. Value: Your crates.io API token

### Tests failing in CI

Run tests locally with the same PostgreSQL version:
```bash
docker run -d -p 5432:5432 -e POSTGRES_PASSWORD=password postgres:16
export DATABASE_URL=postgresql://postgres:password@localhost:5432/test
cargo test --all-features
```

## Semantic Versioning

Follow semantic versioning (semver):
- **MAJOR**: Breaking API changes
- **MINOR**: New features, backward compatible
- **PATCH**: Bug fixes, backward compatible

For a 0.x release, MINOR changes may include breaking changes.

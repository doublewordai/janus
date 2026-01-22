# janus

[![Crates.io](https://img.shields.io/crates/v/janus.svg)](https://crates.io/crates/janus)
[![Documentation](https://docs.rs/janus/badge.svg)](https://docs.rs/janus)
[![License](https://img.shields.io/crates/l/janus.svg)](https://github.com/doublewordai/janus#license)

A lightweight Rust library for routing database operations to different SQLx PostgreSQL connection pools based on whether they're read or write operations.

This enables load distribution by routing read-heavy operations to read replicas while ensuring write operations always go to the primary database.

## Features

- **Zero-cost abstraction**: Trait-based design with no runtime overhead
- **Type-safe routing**: Compile-time guarantees for read/write pool separation
- **Backward compatible**: `PgPool` implements `PoolProvider` for seamless integration
- **Flexible**: Use single pool or separate primary/replica pools
- **Well-tested**: Comprehensive test suite with replica routing verification

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
janus = "0.1"
sqlx = { version = "0.8", features = ["postgres", "runtime-tokio"] }
```

## Quick Start

### Single Pool (Development)

```rust
use sqlx::PgPool;
use janus::PoolProvider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPool::connect("postgresql://localhost/mydb").await?;

    // PgPool implements PoolProvider automatically
    let result: (i32,) = sqlx::query_as("SELECT 1")
        .fetch_one(pool.read())
        .await?;

    Ok(())
}
```

### Read/Write Separation (Production)

```rust
use sqlx::postgres::PgPoolOptions;
use janus::{DbPools, PoolProvider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let primary = PgPoolOptions::new()
        .max_connections(5)
        .connect("postgresql://primary-host/mydb")
        .await?;

    let replica = PgPoolOptions::new()
        .max_connections(10)  // More connections for read-heavy workload
        .connect("postgresql://replica-host/mydb")
        .await?;

    let pools = DbPools::with_replica(primary, replica);

    // Reads go to replica
    let users: Vec<(i32, String)> = sqlx::query_as("SELECT id, name FROM users")
        .fetch_all(pools.read())
        .await?;

    // Writes go to primary
    sqlx::query("INSERT INTO users (name) VALUES ($1)")
        .bind("Alice")
        .execute(pools.write())
        .await?;

    Ok(())
}
```

## Testing with `TestDbPools`

The crate includes a `TestDbPools` helper for use with `#[sqlx::test]` that enforces read/write separation in your tests:

```rust
use sqlx::PgPool;
use janus::{TestDbPools, PoolProvider};

#[sqlx::test]
async fn test_repository(pool: PgPool) {
    // TestDbPools creates a read-only replica from the same database
    let pools = TestDbPools::new(pool).await.unwrap();

    // Writes through .read() will FAIL - catches bugs immediately!
    let result = sqlx::query("INSERT INTO users (name) VALUES ('Alice')")
        .execute(pools.read())
        .await;
    assert!(result.is_err());

    // Writes through .write() work fine
    sqlx::query("CREATE TEMP TABLE users (id INT, name TEXT)")
        .execute(pools.write())
        .await
        .unwrap();
}
```

**Why use `TestDbPools`?**

- Catches routing bugs immediately in tests
- No need for an actual replica database in test environment
- Enforces `default_transaction_read_only = on` on the read pool
- PostgreSQL will reject any write operations on `.read()`

## Generic Programming

Make your types generic over `PoolProvider` to support both single and multi-pool configurations:

```rust
use janus::PoolProvider;

struct Repository<P: PoolProvider> {
    pools: P,
}

impl<P: PoolProvider> Repository<P> {
    async fn get_user(&self, id: i64) -> Result<String, sqlx::Error> {
        // Read from replica
        sqlx::query_scalar("SELECT name FROM users WHERE id = $1")
            .bind(id)
            .fetch_one(self.pools.read())
            .await
    }

    async fn create_user(&self, name: &str) -> Result<i64, sqlx::Error> {
        // Write to primary
        sqlx::query_scalar("INSERT INTO users (name) VALUES ($1) RETURNING id")
            .bind(name)
            .fetch_one(self.pools.write())
            .await
    }
}

// Works with both PgPool and DbPools!
let repo_single = Repository { pools: single_pool };
let repo_multi = Repository { pools: db_pools };
```

## When to Use Each Method

### `.read()` - For Read Operations

Use for queries that:
- Don't modify data (SELECT without FOR UPDATE)
- Can tolerate slight staleness (eventual consistency)
- Benefit from load distribution

Examples: user listings, analytics, dashboards, search

### `.write()` - For Write Operations

Use for operations that:
- Modify data (INSERT, UPDATE, DELETE)
- Require transactions
- Need locking reads (SELECT FOR UPDATE)
- Require read-after-write consistency

Examples: creating records, updates, deletes, transactions

## Architecture

```text
┌─────────────┐
│   DbPools   │
└──────┬──────┘
       │
  ┌────┴────┐
  ↓         ↓
┌─────┐  ┌─────────┐
│Primary│  │ Replica │ (optional)
└─────┘  └─────────┘
```

## Real-World Use Cases

Named after Janus, the Roman god of transitions, gates, and doorways. Depicted with two faces looking in opposite directions - one for reads, one for writes - perfectly representing this crate's dual-pool routing capability.

This library is used in production by:
- [outlet-postgres](https://github.com/doublewordai/outlet-postgres) - HTTP request/response logging middleware
- [fusillade](https://github.com/doublewordai/fusillade) - LLM request batching daemon
- [dwctl](https://github.com/doublewordai/control-layer) - Observability and analytics platform

## License

This project is licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

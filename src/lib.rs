//! # janus
//!
//! A lightweight library for routing database operations to different SQLx PostgreSQL connection pools
//! based on whether they're read or write operations.
//!
//! Named after Janus, the Roman god of transitions and doorways, depicted with two faces
//! looking in opposite directions - perfect for read/write pool routing.
//!
//! This enables load distribution by routing read-heavy operations to read replicas while ensuring
//! write operations always go to the primary database.
//!
//! ## Features
//!
//! - **Zero-cost abstraction**: Trait-based design with no runtime overhead
//! - **Type-safe routing**: Compile-time guarantees for read/write pool separation
//! - **Backward compatible**: `PgPool` implements `PoolProvider` for seamless integration
//! - **Flexible**: Use single pool or separate primary/replica pools
//! - **Test helpers**: [`TestDbPools`] for testing with `#[sqlx::test]`
//! - **Well-tested**: Comprehensive test suite with replica routing verification
//!
//! ## Quick Start
//!
//! ### Single Pool (Development)
//!
//! ```rust,no_run
//! use sqlx::PgPool;
//! use janus::PoolProvider;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let pool = PgPool::connect("postgresql://localhost/mydb").await?;
//!
//! // PgPool implements PoolProvider automatically
//! let result: (i32,) = sqlx::query_as("SELECT 1")
//!     .fetch_one(pool.read())
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Read/Write Separation (Production)
//!
//! ```rust,no_run
//! use sqlx::postgres::PgPoolOptions;
//! use janus::{DbPools, PoolProvider};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let primary = PgPoolOptions::new()
//!     .max_connections(5)
//!     .connect("postgresql://primary-host/mydb")
//!     .await?;
//!
//! let replica = PgPoolOptions::new()
//!     .max_connections(10)
//!     .connect("postgresql://replica-host/mydb")
//!     .await?;
//!
//! let pools = DbPools::with_replica(primary, replica);
//!
//! // Reads go to replica
//! let users: Vec<(i32, String)> = sqlx::query_as("SELECT id, name FROM users")
//!     .fetch_all(pools.read())
//!     .await?;
//!
//! // Writes go to primary
//! sqlx::query("INSERT INTO users (name) VALUES ($1)")
//!     .bind("Alice")
//!     .execute(pools.write())
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐
//! │   DbPools   │
//! └──────┬──────┘
//!        │
//!   ┌────┴────┐
//!   ↓         ↓
//! ┌─────┐  ┌─────────┐
//! │Primary│  │ Replica │ (optional)
//! └─────┘  └─────────┘
//! ```
//!
//! ## Generic Programming
//!
//! Make your types generic over `PoolProvider` to support both single and multi-pool configurations:
//!
//! ```rust
//! use janus::PoolProvider;
//!
//! struct Repository<P: PoolProvider> {
//!     pools: P,
//! }
//!
//! impl<P: PoolProvider> Repository<P> {
//!     async fn get_user(&self, id: i64) -> Result<String, sqlx::Error> {
//!         // Read from replica
//!         sqlx::query_scalar("SELECT name FROM users WHERE id = $1")
//!             .bind(id)
//!             .fetch_one(self.pools.read())
//!             .await
//!     }
//!
//!     async fn create_user(&self, name: &str) -> Result<i64, sqlx::Error> {
//!         // Write to primary
//!         sqlx::query_scalar("INSERT INTO users (name) VALUES ($1) RETURNING id")
//!             .bind(name)
//!             .fetch_one(self.pools.write())
//!             .await
//!     }
//! }
//! ```
//!
//! ## Testing
//!
//! Use [`TestDbPools`] with `#[sqlx::test]` to enforce read/write separation in tests:
//!
//! ```rust,no_run
//! use sqlx::PgPool;
//! use janus::{TestDbPools, PoolProvider};
//!
//! #[sqlx::test]
//! async fn test_repository(pool: PgPool) {
//!     let pools = TestDbPools::new(pool).await.unwrap();
//!
//!     // Write operations through .read() will FAIL
//!     let result = sqlx::query("INSERT INTO users VALUES (1)")
//!         .execute(pools.read())
//!         .await;
//!     assert!(result.is_err());
//! }
//! ```
//!
//! This catches routing bugs immediately without needing a real replica database.

use sqlx::PgPool;
use std::ops::Deref;

/// Trait for providing database pools with read/write routing.
///
/// Implementations can provide separate read and write pools for load distribution,
/// or use a single pool for both operations.
///
/// # Thread Safety
///
/// Implementations must be `Clone`, `Send`, and `Sync` to work with async Rust
/// and be shared across tasks.
///
/// # When to Use Each Method
///
/// ## `.read()` - For Read Operations
///
/// Use for queries that:
/// - Don't modify data (SELECT without FOR UPDATE)
/// - Can tolerate slight staleness (eventual consistency)
/// - Benefit from load distribution
///
/// Examples: user listings, analytics, dashboards, search
///
/// ## `.write()` - For Write Operations
///
/// Use for operations that:
/// - Modify data (INSERT, UPDATE, DELETE)
/// - Require transactions
/// - Need locking reads (SELECT FOR UPDATE)
/// - Require read-after-write consistency
///
/// Examples: creating records, updates, deletes, transactions
///
/// # Example Implementation
///
/// ```
/// use sqlx::PgPool;
/// use janus::PoolProvider;
///
/// struct MyPools {
///     primary: PgPool,
///     replica: Option<PgPool>,
/// }
///
/// impl PoolProvider for MyPools {
///     fn read(&self) -> &PgPool {
///         self.replica.as_ref().unwrap_or(&self.primary)
///     }
///
///     fn write(&self) -> &PgPool {
///         &self.primary
///     }
/// }
/// ```
pub trait PoolProvider: Clone + Send + Sync + 'static {
    /// Get a pool for read operations.
    ///
    /// May return a read replica for load distribution, or fall back to
    /// the primary pool if no replica is configured.
    fn read(&self) -> &PgPool;

    /// Get a pool for write operations.
    ///
    /// Should always return the primary pool to ensure ACID guarantees
    /// and read-after-write consistency.
    fn write(&self) -> &PgPool;
}

/// Database pool abstraction supporting read replicas.
///
/// Wraps primary and optional replica pools, providing methods for
/// explicit read/write routing while maintaining backwards compatibility
/// through `Deref<Target = PgPool>`.
///
/// # Examples
///
/// ## Single Pool Configuration
///
/// ```rust,no_run
/// use sqlx::PgPool;
/// use janus::DbPools;
///
/// # async fn example() -> Result<(), sqlx::Error> {
/// let pool = PgPool::connect("postgresql://localhost/db").await?;
/// let pools = DbPools::new(pool);
///
/// // Both read() and write() return the same pool
/// assert!(!pools.has_replica());
/// # Ok(())
/// # }
/// ```
///
/// ## Primary/Replica Configuration
///
/// ```rust,no_run
/// use sqlx::postgres::PgPoolOptions;
/// use janus::DbPools;
///
/// # async fn example() -> Result<(), sqlx::Error> {
/// let primary = PgPoolOptions::new()
///     .max_connections(5)
///     .connect("postgresql://primary/db")
///     .await?;
///
/// let replica = PgPoolOptions::new()
///     .max_connections(10)
///     .connect("postgresql://replica/db")
///     .await?;
///
/// let pools = DbPools::with_replica(primary, replica);
/// assert!(pools.has_replica());
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct DbPools {
    primary: PgPool,
    replica: Option<PgPool>,
}

impl DbPools {
    /// Create a new DbPools with only a primary pool.
    ///
    /// This is useful for development or when you don't have a read replica configured.
    /// All read and write operations will route to the primary pool.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use sqlx::PgPool;
    /// use sqlx_pool_router::DbPools;
    ///
    /// # async fn example() -> Result<(), sqlx::Error> {
    /// let pool = PgPool::connect("postgresql://localhost/db").await?;
    /// let pools = DbPools::new(pool);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(primary: PgPool) -> Self {
        Self {
            primary,
            replica: None,
        }
    }

    /// Create a new DbPools with primary and replica pools.
    ///
    /// Read operations will route to the replica pool for load distribution,
    /// while write operations always use the primary pool.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use sqlx::postgres::PgPoolOptions;
    /// use sqlx_pool_router::DbPools;
    ///
    /// # async fn example() -> Result<(), sqlx::Error> {
    /// let primary = PgPoolOptions::new()
    ///     .max_connections(5)
    ///     .connect("postgresql://primary/db")
    ///     .await?;
    ///
    /// let replica = PgPoolOptions::new()
    ///     .max_connections(10)
    ///     .connect("postgresql://replica/db")
    ///     .await?;
    ///
    /// let pools = DbPools::with_replica(primary, replica);
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_replica(primary: PgPool, replica: PgPool) -> Self {
        Self {
            primary,
            replica: Some(replica),
        }
    }

    /// Check if a replica pool is configured.
    ///
    /// Returns `true` if a replica pool was provided via [`with_replica`](Self::with_replica).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use sqlx::PgPool;
    /// use sqlx_pool_router::DbPools;
    ///
    /// # async fn example() -> Result<(), sqlx::Error> {
    /// let pool = PgPool::connect("postgresql://localhost/db").await?;
    /// let pools = DbPools::new(pool);
    /// assert!(!pools.has_replica());
    /// # Ok(())
    /// # }
    /// ```
    pub fn has_replica(&self) -> bool {
        self.replica.is_some()
    }

    /// Close all database connections.
    ///
    /// Closes both primary and replica pools (if configured).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use sqlx::PgPool;
    /// use sqlx_pool_router::DbPools;
    ///
    /// # async fn example() -> Result<(), sqlx::Error> {
    /// let pool = PgPool::connect("postgresql://localhost/db").await?;
    /// let pools = DbPools::new(pool);
    /// pools.close().await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn close(&self) {
        self.primary.close().await;
        if let Some(replica) = &self.replica {
            replica.close().await;
        }
    }
}

impl PoolProvider for DbPools {
    fn read(&self) -> &PgPool {
        self.replica.as_ref().unwrap_or(&self.primary)
    }

    fn write(&self) -> &PgPool {
        &self.primary
    }
}

/// Dereferences to the primary pool.
///
/// This allows natural usage like `&*pools` when you need a `&PgPool`.
/// For explicit routing, use `.read()` or `.write()` methods.
impl Deref for DbPools {
    type Target = PgPool;

    fn deref(&self) -> &Self::Target {
        &self.primary
    }
}

/// Implement PoolProvider for PgPool for backward compatibility.
///
/// This allows existing code using `PgPool` directly to work with generic
/// code that accepts `impl PoolProvider` without any changes.
///
/// # Example
///
/// ```rust,no_run
/// use sqlx::PgPool;
/// use janus::PoolProvider;
///
/// async fn query_user<P: PoolProvider>(pools: &P, id: i64) -> Result<String, sqlx::Error> {
///     sqlx::query_scalar("SELECT name FROM users WHERE id = $1")
///         .bind(id)
///         .fetch_one(pools.read())
///         .await
/// }
///
/// # async fn example() -> Result<(), sqlx::Error> {
/// let pool = PgPool::connect("postgresql://localhost/db").await?;
///
/// // Works with PgPool directly
/// let name = query_user(&pool, 1).await?;
/// # Ok(())
/// # }
/// ```
impl PoolProvider for PgPool {
    fn read(&self) -> &PgPool {
        self
    }

    fn write(&self) -> &PgPool {
        self
    }
}

/// Test pool provider with read-only replica enforcement.
///
/// This creates two separate connection pools from the same database:
/// - Primary pool for writes (normal permissions)
/// - Replica pool for reads (enforces `default_transaction_read_only = on`)
///
/// This ensures tests catch bugs where write operations are incorrectly
/// routed through `.read()`. PostgreSQL will reject writes with:
/// "cannot execute INSERT/UPDATE/DELETE in a read-only transaction"
///
/// # Usage with `#[sqlx::test]`
///
/// ```rust,no_run
/// use sqlx::PgPool;
/// use sqlx_pool_router::{TestDbPools, PoolProvider};
///
/// #[sqlx::test]
/// async fn test_read_write_routing(pool: PgPool) {
///     let pools = TestDbPools::new(pool).await.unwrap();
///
///     // Write operations work on .write()
///     sqlx::query("CREATE TEMP TABLE users (id INT)")
///         .execute(pools.write())
///         .await
///         .expect("Write pool should allow writes");
///
///     // Write operations FAIL on .read()
///     let result = sqlx::query("INSERT INTO users VALUES (1)")
///         .execute(pools.read())
///         .await;
///     assert!(result.is_err(), "Read pool should reject writes");
///
///     // Read operations work on .read()
///     let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
///         .fetch_one(pools.read())
///         .await
///         .expect("Read pool should allow reads");
/// }
/// ```
///
/// # Why This Matters
///
/// Without this test helper, you might accidentally route write operations through
/// `.read()` and not catch the bug until production when you have an actual replica
/// with replication lag. This helper makes the bug obvious immediately in tests.
///
/// # Example
///
/// ```rust,no_run
/// use sqlx::PgPool;
/// use sqlx_pool_router::{TestDbPools, PoolProvider};
///
/// struct Repository<P: PoolProvider> {
///     pools: P,
/// }
///
/// impl<P: PoolProvider> Repository<P> {
///     async fn get_user(&self, id: i64) -> Result<String, sqlx::Error> {
///         sqlx::query_scalar("SELECT name FROM users WHERE id = $1")
///             .bind(id)
///             .fetch_one(self.pools.read())
///             .await
///     }
///
///     async fn create_user(&self, name: &str) -> Result<i64, sqlx::Error> {
///         sqlx::query_scalar("INSERT INTO users (name) VALUES ($1) RETURNING id")
///             .bind(name)
///             .fetch_one(self.pools.write())
///             .await
///     }
/// }
///
/// #[sqlx::test]
/// async fn test_repository_routing(pool: PgPool) {
///     let pools = TestDbPools::new(pool).await.unwrap();
///     let repo = Repository { pools };
///
///     // Test will fail if create_user incorrectly uses .read()
///     sqlx::query("CREATE TEMP TABLE users (id SERIAL PRIMARY KEY, name TEXT)")
///         .execute(repo.pools.write())
///         .await
///         .unwrap();
///
///     let user_id = repo.create_user("Alice").await.unwrap();
///     let name = repo.get_user(user_id).await.unwrap();
///     assert_eq!(name, "Alice");
/// }
/// ```
#[derive(Clone, Debug)]
pub struct TestDbPools {
    primary: PgPool,
    replica: PgPool,
}

impl TestDbPools {
    /// Create test pools from a single database pool.
    ///
    /// This creates:
    /// - A primary pool (clone of input) for writes
    /// - A replica pool (new connection) configured as read-only
    ///
    /// The replica pool enforces `default_transaction_read_only = on`,
    /// so any write operations will fail with a PostgreSQL error.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use sqlx::PgPool;
    /// use sqlx_pool_router::TestDbPools;
    ///
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let pools = TestDbPools::new(pool).await?;
    ///
    /// // Now you have pools that enforce read/write separation
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(pool: PgPool) -> Result<Self, sqlx::Error> {
        use sqlx::postgres::PgPoolOptions;

        let primary = pool.clone();

        // Create a separate pool with read-only enforcement
        let replica = PgPoolOptions::new()
            .max_connections(pool.options().get_max_connections())
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    // Set all transactions to read-only by default
                    sqlx::query("SET default_transaction_read_only = on")
                        .execute(&mut *conn)
                        .await?;
                    Ok(())
                })
            })
            .connect_with(pool.connect_options().as_ref().clone())
            .await?;

        Ok(Self { primary, replica })
    }
}

impl PoolProvider for TestDbPools {
    fn read(&self) -> &PgPool {
        &self.replica
    }

    fn write(&self) -> &PgPool {
        &self.primary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    /// Helper to create a test database and return its pool and name
    async fn create_test_db(admin_pool: &PgPool, suffix: &str) -> (PgPool, String) {
        let db_name = format!("test_dbpools_{}", suffix);

        // Clean up if exists
        sqlx::query(&format!(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}'",
            db_name
        ))
        .execute(admin_pool)
        .await
        .ok();
        sqlx::query(&format!("DROP DATABASE IF EXISTS {}", db_name))
            .execute(admin_pool)
            .await
            .unwrap();

        // Create fresh database
        sqlx::query(&format!("CREATE DATABASE {}", db_name))
            .execute(admin_pool)
            .await
            .unwrap();

        // Connect to it
        let url = build_test_url(&db_name);
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .unwrap();

        // Create a marker table to identify which database we're connected to
        sqlx::query("CREATE TABLE db_marker (name TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(&format!("INSERT INTO db_marker VALUES ('{}')", db_name))
            .execute(&pool)
            .await
            .unwrap();

        (pool, db_name)
    }

    async fn drop_test_db(admin_pool: &PgPool, db_name: &str) {
        sqlx::query(&format!(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}'",
            db_name
        ))
        .execute(admin_pool)
        .await
        .ok();
        sqlx::query(&format!("DROP DATABASE IF EXISTS {}", db_name))
            .execute(admin_pool)
            .await
            .ok();
    }

    fn build_test_url(database: &str) -> String {
        if let Ok(base_url) = std::env::var("DATABASE_URL") {
            if let Ok(mut url) = url::Url::parse(&base_url) {
                url.set_path(&format!("/{}", database));
                return url.to_string();
            }
        }
        format!("postgres://postgres:password@localhost:5432/{}", database)
    }

    #[sqlx::test]
    async fn test_dbpools_without_replica(pool: PgPool) {
        let db_pools = DbPools::new(pool.clone());

        // Without replica, read() should return primary
        assert!(!db_pools.has_replica());

        // Both read and write should work
        let read_result: (i32,) = sqlx::query_as("SELECT 1")
            .fetch_one(db_pools.read())
            .await
            .unwrap();
        assert_eq!(read_result.0, 1);

        let write_result: (i32,) = sqlx::query_as("SELECT 2")
            .fetch_one(db_pools.write())
            .await
            .unwrap();
        assert_eq!(write_result.0, 2);

        // Deref should also work
        let deref_result: (i32,) = sqlx::query_as("SELECT 3")
            .fetch_one(&*db_pools)
            .await
            .unwrap();
        assert_eq!(deref_result.0, 3);
    }

    #[sqlx::test]
    async fn test_dbpools_with_replica_routes_correctly(_pool: PgPool) {
        // Create admin connection to postgres database
        let admin_url = build_test_url("postgres");
        let admin_pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&admin_url)
            .await
            .unwrap();

        // Create two separate databases to simulate primary and replica
        let (primary_pool, primary_name) = create_test_db(&admin_pool, "primary").await;
        let (replica_pool, replica_name) = create_test_db(&admin_pool, "replica").await;

        let db_pools = DbPools::with_replica(primary_pool.clone(), replica_pool.clone());
        assert!(db_pools.has_replica());

        // read() should return replica
        let read_marker: (String,) = sqlx::query_as("SELECT name FROM db_marker")
            .fetch_one(db_pools.read())
            .await
            .unwrap();
        assert_eq!(
            read_marker.0, replica_name,
            "read() should route to replica"
        );

        // write() should return primary
        let write_marker: (String,) = sqlx::query_as("SELECT name FROM db_marker")
            .fetch_one(db_pools.write())
            .await
            .unwrap();
        assert_eq!(
            write_marker.0, primary_name,
            "write() should route to primary"
        );

        // Deref should return primary
        let deref_marker: (String,) = sqlx::query_as("SELECT name FROM db_marker")
            .fetch_one(&*db_pools)
            .await
            .unwrap();
        assert_eq!(
            deref_marker.0, primary_name,
            "deref should route to primary"
        );

        // Cleanup
        primary_pool.close().await;
        replica_pool.close().await;
        drop_test_db(&admin_pool, &primary_name).await;
        drop_test_db(&admin_pool, &replica_name).await;
    }

    #[sqlx::test]
    async fn test_dbpools_close(pool: PgPool) {
        let db_pools = DbPools::new(pool);

        // Close should not panic
        db_pools.close().await;
    }

    #[sqlx::test]
    async fn test_pgpool_implements_pool_provider(pool: PgPool) {
        // PgPool should implement PoolProvider
        assert_eq!(pool.read() as *const _, pool.write() as *const _);

        // Should be able to use it the same way
        let result: (i32,) = sqlx::query_as("SELECT 1")
            .fetch_one(pool.read())
            .await
            .unwrap();
        assert_eq!(result.0, 1);
    }

    #[sqlx::test]
    async fn test_testdbpools_read_pool_rejects_writes(pool: PgPool) {
        let pools = TestDbPools::new(pool).await.unwrap();

        // Write operations should work on the write pool
        sqlx::query("CREATE TEMP TABLE test_write (id INT)")
            .execute(pools.write())
            .await
            .expect("Write pool should allow CREATE TABLE");

        // Write operations should FAIL on the read pool
        let result = sqlx::query("CREATE TEMP TABLE test_read_reject (id INT)")
            .execute(pools.read())
            .await;

        assert!(result.is_err(), "Read pool should reject CREATE TABLE");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("read-only") || err.contains("cannot execute"),
            "Error should mention read-only restriction, got: {}",
            err
        );
    }

    #[sqlx::test]
    async fn test_testdbpools_read_pool_allows_selects(pool: PgPool) {
        let pools = TestDbPools::new(pool).await.unwrap();

        // Read operations should work on the read pool
        let result: (i32,) = sqlx::query_as("SELECT 1 + 1 as sum")
            .fetch_one(pools.read())
            .await
            .expect("Read pool should allow SELECT");

        assert_eq!(result.0, 2, "Should compute 1 + 1 = 2");
    }

    #[sqlx::test]
    async fn test_testdbpools_write_pool_allows_writes(pool: PgPool) {
        let pools = TestDbPools::new(pool).await.unwrap();

        // Create temp table
        sqlx::query("CREATE TEMP TABLE test_users (id SERIAL PRIMARY KEY, name TEXT)")
            .execute(pools.write())
            .await
            .expect("Write pool should allow CREATE TABLE");

        // Insert data
        sqlx::query("INSERT INTO test_users (name) VALUES ($1)")
            .bind("Alice")
            .execute(pools.write())
            .await
            .expect("Write pool should allow INSERT");

        // Read back from write pool (TEMP tables are session-specific)
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM test_users")
            .fetch_one(pools.write())
            .await
            .expect("Write pool should allow SELECT");

        assert_eq!(count.0, 1, "Should have 1 user");
    }
}

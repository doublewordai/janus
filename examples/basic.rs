//! Basic example demonstrating read/write pool separation.
//!
//! This example shows how to:
//! 1. Connect to primary and replica databases
//! 2. Route read operations to the replica
//! 3. Route write operations to the primary
//!
//! To run this example:
//! 1. Set up PostgreSQL with a primary database
//! 2. Set DATABASE_URL environment variable
//! 3. Optionally set REPLICA_DATABASE_URL (or it will use the same as primary)
//! 4. Run: cargo run --example basic

use sqlx::postgres::PgPoolOptions;
use sqlx_pool_router::{DbPools, PoolProvider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get database URLs from environment
    let primary_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost/test".to_string());

    // Use the same URL for replica in development, or a different one in production
    let replica_url = std::env::var("REPLICA_DATABASE_URL").unwrap_or_else(|_| primary_url.clone());

    println!("🔌 Connecting to databases:");
    println!("   Primary: {}", primary_url);
    println!("   Replica: {}", replica_url);
    println!();

    // Create connection pools
    let primary = PgPoolOptions::new()
        .max_connections(5)
        .connect(&primary_url)
        .await?;

    let replica = PgPoolOptions::new()
        .max_connections(10) // More connections for read-heavy workload
        .connect(&replica_url)
        .await?;

    // Create DbPools with read/write separation
    let pools = if primary_url == replica_url {
        println!("⚠️  Using single pool (primary and replica are the same)");
        DbPools::new(primary)
    } else {
        println!("✓ Using separate pools for read/write separation");
        DbPools::with_replica(primary, replica)
    };

    println!();
    println!("📝 Creating example table...");

    // Create a test table (write operation - uses primary)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            created_at TIMESTAMPTZ DEFAULT NOW()
        )
        "#,
    )
    .execute(pools.write())
    .await?;

    println!("✓ Table created");
    println!();

    // Insert some data (write operation - uses primary)
    println!("📝 Inserting users...");
    for name in &["Alice", "Bob", "Charlie"] {
        sqlx::query("INSERT INTO users (name) VALUES ($1)")
            .bind(name)
            .execute(pools.write())
            .await?;
        println!("   ✓ Inserted {}", name);
    }
    println!();

    // Query data (read operation - uses replica if available)
    println!("📖 Reading users from replica...");
    let users: Vec<(i32, String)> = sqlx::query_as("SELECT id, name FROM users ORDER BY id")
        .fetch_all(pools.read())
        .await?;

    println!("   Found {} users:", users.len());
    for (id, name) in users {
        println!("   - ID {}: {}", id, name);
    }
    println!();

    // Count users (read operation - uses replica if available)
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pools.read())
        .await?;
    println!("📊 Total users (from replica): {}", count.0);
    println!();

    // Update a user (write operation - uses primary)
    println!("✏️  Updating user...");
    sqlx::query("UPDATE users SET name = $1 WHERE id = $2")
        .bind("Alice Smith")
        .bind(1)
        .execute(pools.write())
        .await?;
    println!("   ✓ Updated user 1");
    println!();

    // Read updated data
    let updated_name: (String,) = sqlx::query_as("SELECT name FROM users WHERE id = $1")
        .bind(1)
        .fetch_one(pools.read())
        .await?;
    println!("📖 Updated name (from replica): {}", updated_name.0);
    println!();

    // Clean up
    println!("🧹 Cleaning up...");
    sqlx::query("DROP TABLE users")
        .execute(pools.write())
        .await?;
    println!("   ✓ Table dropped");

    println!();
    println!("✅ Example completed successfully!");
    println!();
    println!("💡 Key takeaways:");
    println!("   - Write operations (INSERT, UPDATE, DELETE) use .write()");
    println!("   - Read operations (SELECT) use .read()");
    println!("   - Reads route to replica for load distribution");
    println!("   - Writes always route to primary for consistency");

    Ok(())
}

pub mod strategies;
pub mod signals;
pub mod trades;
pub mod performance;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use anyhow::Result;
use crate::config::DatabaseConfig;

pub async fn create_pool(config: &DatabaseConfig) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .connect(&config.url)
        .await?;

    Ok(pool)
}

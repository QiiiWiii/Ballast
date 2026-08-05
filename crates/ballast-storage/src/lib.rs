#![forbid(unsafe_code)]

use sqlx::postgres::PgPoolOptions;

pub type DatabasePool = sqlx::PgPool;

pub async fn connect(database_url: &str) -> Result<DatabasePool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
}

pub async fn migrate(pool: &DatabasePool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}

pub async fn ping(pool: &DatabasePool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

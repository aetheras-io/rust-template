pub mod example;
pub mod users;

use sqlx::{
    Connection,
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPool, PgPoolOptions, PgQueryResult},
};

static EMBEDDED_MIGRATE: Migrator = sqlx::migrate!();

pub async fn connect_pg(
    database_url: &str,
    max_connections: u32,
    application_name: Option<&str>,
) -> sqlx::Result<PgPool> {
    let mut opts = database_url.parse::<PgConnectOptions>()?;
    if let Some(application_name) = application_name {
        opts = opts.application_name(application_name);
    }
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect_with(opts)
        .await
}

pub fn ensure_affected(count: u64) -> impl FnOnce(PgQueryResult) -> sqlx::Result<()> {
    move |pg_done| {
        if pg_done.rows_affected() == count {
            Ok(())
        } else {
            Err(sqlx::Error::RowNotFound)
        }
    }
}

pub async fn migrate(pg_pool: &PgPool) -> sqlx::Result<()> {
    EMBEDDED_MIGRATE.run(pg_pool).await?;
    Ok(())
}

pub async fn setup_test_db(name: &'static str) -> Result<PgPool, sqlx::Error> {
    // Initial connection using `PgConnection` instead of `PgPool`
    let mut conn = sqlx::PgConnection::connect(
        "postgres://postgres:123456@localhost/postgres?sslmode=disable",
    )
    .await?;

    let db_name = format!("test_{name}");
    let res = sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE DATABASE \"{db_name}\""
    )))
    .execute(&mut conn)
    .await;
    if res.is_err() {
        println!("WARNING: {db_name} already exists, dropping");
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE \"{db_name}\"")))
            .execute(&mut conn)
            .await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE DATABASE \"{db_name}\""
        )))
        .execute(&mut conn)
        .await?;
    }

    let db_url = format!("postgres://postgres:123456@localhost:5432/{db_name}?sslmode=disable");
    let opts = db_url.parse::<PgConnectOptions>()?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await?;

    migrate(&pool).await?;
    Ok(pool)
}

pub async fn teardown_test_db(name: &'static str, pool: PgPool) -> Result<(), sqlx::Error> {
    pool.close().await;

    let mut conn = sqlx::PgConnection::connect(
        "postgres://postgres:123456@localhost/postgres?sslmode=disable",
    )
    .await?;

    let db_name = format!("test_{name}");
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE \"{db_name}\"")))
        .execute(&mut conn)
        .await?;
    Ok(())
}

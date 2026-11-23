//! Runs the CRF Agent Demo backend.

use apis::{app, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set (see .env)");
    let bind = std::env::var("CRF_API_BIND").unwrap_or_else(|_| "0.0.0.0:3100".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    // The schema is part of the binary: the server brings itself up to date.
    sqlx::migrate!("./migrations").run(&pool).await?;

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("crf-agent-demo backend listening on {bind}");

    axum::serve(listener, app(AppState { pool })).await?;

    Ok(())
}

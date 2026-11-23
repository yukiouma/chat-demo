use sqlx::PgPool;

/// What every handler needs.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
}

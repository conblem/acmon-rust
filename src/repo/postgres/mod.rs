use async_trait::async_trait;

use super::account::AccountRepo;
use super::executor::IntoInner;

struct PostgresAccountRepo<T> {
    inner: T,
}

impl<T> From<T> for PostgresAccountRepo<T> {
    fn from(inner: T) -> Self {
        Self { inner }
    }
}

#[derive(sqlx::FromRow)]
struct PostgresAccount {
    id: i64,
}

#[async_trait]
impl<T> AccountRepo for PostgresAccountRepo<T>
where
    T: Send,
    for<'b> T: IntoInner<'b>,
{
    async fn get_account(&mut self, _input: &str) {
        sqlx::query_as::<_, PostgresAccount>("SELECT * FROM ACCOUNT")
            .fetch_many(self.inner.inner());
    }
}

#[cfg(all(test, feature = "container"))]
mod tests {
    use sqlx::PgPool;
    use testcontainers::{clients, images, Docker};

    use super::*;

    #[tokio::test]
    async fn test() {
        let docker = clients::Cli::default();
        let postgres_image = images::postgres::Postgres::default();
        let node = docker.run(postgres_image);

        let port = node.get_host_port(5432).unwrap();
        let connection_string = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

        let pool = PgPool::connect(&connection_string).await.unwrap();

        let mut account_repo = PostgresAccountRepo::from(&pool);
        account_repo.get_account("test").await;
    }
}

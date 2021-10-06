use sqlx::{Executor, Postgres};

trait IntoInner<'b> {
    type Inner: Executor<'b, Database = Postgres>;
    fn inner(&'b mut self) -> Self::Inner;
}

impl<'a, 'b, R: 'b> IntoInner<'b> for &'a R
where
    &'b R: Executor<'b, Database = Postgres>,
{
    type Inner = &'b R;
    fn inner(&'b mut self) -> Self::Inner {
        self
    }
}

impl<'a, 'b, M: 'b> IntoInner<'b> for &'a mut M
where
    &'b mut M: Executor<'b, Database = Postgres>,
{
    type Inner = &'b mut M;
    fn inner(&'b mut self) -> Self::Inner {
        self
    }
}

#[cfg(all(test, feature = "container"))]
mod tests {
    use sqlx::postgres::PgRow;
    use sqlx::{PgPool, Postgres, Row};
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
        execute(&pool).await;
        execute(&pool).await;

        let mut conn = pool.acquire().await.unwrap();
        execute(&mut conn).await;
        execute(&mut conn).await;
    }

    async fn execute<T>(mut executor: T)
    where
        for<'b> T: IntoInner<'b>,
    {
        let res = sqlx::query("select 1 + 1")
            .try_map(|row: PgRow| row.try_get::<i32, _>(0))
            .fetch_one(executor.inner())
            .await
            .unwrap();
        assert_eq!(2, res);

        let res = sqlx::query("select 2 + 2")
            .try_map(|row: PgRow| row.try_get::<i32, _>(0))
            .fetch_one(executor.inner())
            .await
            .unwrap();
        assert_eq!(4, res);
    }
}

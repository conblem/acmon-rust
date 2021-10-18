use sqlx::{Executor, Postgres};

pub(super) trait IsExecutor<'b> {
    type Bound: Executor<'b, Database = Postgres>;
    fn coerce(&'b mut self) -> Self::Bound;
}

impl<'a, 'b, R: 'b> IsExecutor<'b> for &'a R
where
    &'b R: Executor<'b, Database = Postgres>,
{
    type Bound = &'b R;
    fn coerce(&'b mut self) -> Self::Bound {
        self
    }
}

impl<'a, 'b, M: 'b> IsExecutor<'b> for &'a mut M
where
    &'b mut M: Executor<'b, Database = Postgres>,
{
    type Bound = &'b mut M;
    fn coerce(&'b mut self) -> Self::Bound {
        self
    }
}

#[cfg(all(test, feature = "container"))]
mod tests {
    use sqlx::postgres::PgRow;
    use sqlx::{PgPool, Row};
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

        let mut transaction = pool.begin().await.unwrap();
        execute(&mut transaction).await;
        execute(&mut transaction).await;
    }

    async fn execute<T>(mut executor: T)
    where
        for<'b> T: IsExecutor<'b>,
    {
        let res = sqlx::query("select 1 + 1")
            .try_map(|row: PgRow| row.try_get::<i32, _>(0))
            .fetch_one(executor.coerce())
            .await
            .unwrap();
        assert_eq!(2, res);

        let res = sqlx::query("select 2 + 2")
            .try_map(|row: PgRow| row.try_get::<i32, _>(0))
            .fetch_one(executor.coerce())
            .await
            .unwrap();
        assert_eq!(4, res);
    }
}

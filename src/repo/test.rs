use sqlx::{Executor, Postgres};

struct Wrapper<T> {
    inner: T,
}

impl<'a, R> From<&'a R> for Wrapper<&'a R> {
    fn from(inner: &'a R) -> Self {
        Wrapper {
            inner,
        }
    }
}

impl<'a, M> From<&'a mut M> for Wrapper<&'a mut M> {
    fn from(inner: &'a mut M) -> Self {
        Wrapper {
            inner,
        }
    }
}

trait IntoInner<'b, T> where Self: From<T> {
    type Inner;
    fn inner(&'b mut self) -> Self::Inner;
}

impl<'a, 'b, R: 'b> IntoInner<'b, &'a R> for Wrapper<&'a R> {
    type Inner = &'b R;
    fn inner(&'b mut self) -> &'b R {
        self.inner
    }
}

impl<'a, 'b, M: 'b> IntoInner<'b, &'a mut M> for Wrapper<&'a mut M> {
    type Inner = &'b mut M;
    fn inner(&'b mut self) -> &'b mut M {
        self.inner
    }
}

trait ExecutorBound<'b, T>: IntoInner<'b, T, Inner = Self::Bound>  {
    type Bound: Executor<'b, Database = Postgres>;
}

impl<'a, 'b, R: 'b> ExecutorBound<'b, &'a R> for Wrapper<&'a R>
where
    &'b R: Executor<'b, Database = Postgres>,
{
    type Bound = &'b R;
}

impl<'a, 'b, M: 'b> ExecutorBound<'b, &'a mut M> for Wrapper<&'a mut M>
where
    &'b mut M: Executor<'b, Database = Postgres>,
{
    type Bound = &'b mut M;
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

        let mut conn = pool.acquire().await.unwrap();
        execute(&mut conn).await;
    }

    async fn execute<T>(executor: T)
    where
        for<'b> Wrapper<T>: ExecutorBound<'b, T>,
    {
        let mut wrapper: Wrapper<T> = executor.into();

        let res = sqlx::query("select 1 + 1")
            .try_map(|row: PgRow| row.try_get::<i32, _>(0))
            .fetch_one(wrapper.inner())
            .await
            .unwrap();
        assert_eq!(2, res);

        let res = sqlx::query("select 2 + 2")
            .try_map(|row: PgRow| row.try_get::<i32, _>(0))
            .fetch_one(wrapper.inner())
            .await
            .unwrap();
        assert_eq!(4, res);
    }
}

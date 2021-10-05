use core::convert::Infallible;
use sqlx::{Database, Executor, Postgres};
use std::marker::PhantomData;
use std::ops::DerefMut;

struct Wrapper<'a, T> {
    inner: T,
    _phantom: PhantomData<&'a ()>,
}

impl<'a, R> From<&'a R> for Wrapper<'a, &'a R> {
    fn from(inner: &'a R) -> Self {
        Wrapper {
            inner,
            _phantom: PhantomData,
        }
    }
}

impl<'a, M> From<&'a mut M> for Wrapper<'a, &'a mut M> {
    fn from(inner: &'a mut M) -> Self {
        Wrapper {
            inner,
            _phantom: PhantomData,
        }
    }
}

trait IntoInner<'b> {
    type Inner;
    fn inner(&'b mut self) -> Self::Inner;
}

impl<'a, 'b, R: 'b> IntoInner<'b> for Wrapper<'a, &'a R> {
    type Inner = &'b R;
    fn inner(&'b mut self) -> &'b R {
        self.inner
    }
}

impl<'a, 'b, M: 'b> IntoInner<'b> for Wrapper<'a, &'a mut M> {
    type Inner = &'b mut M;
    fn inner(&'b mut self) -> &'b mut M {
        self.inner
    }
}

trait ExecutorBound<'b>: IntoInner<'b, Inner = Self::Bound> {
    type Bound: Executor<'b, Database = Postgres>;
}

impl<'a, 'b, R: 'b> ExecutorBound<'b> for Wrapper<'a, &'a R>
where
    &'b R: Executor<'b, Database = Postgres>,
{
    type Bound = &'b R;
}

impl<'a, 'b, M: 'b> ExecutorBound<'b> for Wrapper<'a, &'a mut M>
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

    async fn execute<'a, I, T>(executor: I)
    where
        I: Into<Wrapper<'a, T>>,
        for<'b> Wrapper<'a, T>: ExecutorBound<'b>,
    {
        let mut wrapper: Wrapper<'a, T> = executor.into();

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

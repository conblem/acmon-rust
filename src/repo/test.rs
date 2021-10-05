use core::convert::Infallible;
use sqlx::{Executor, Postgres};

struct NeverRef {
    inner: Infallible,
}

struct NeverMut {
    inner: Infallible,
}

enum Wrapper<'a, R, M> {
    Ref { inner: &'a R },
    Mut { inner: &'a mut M },
}

impl<'a, 'b, R, M> Wrapper<'a, R, M>
where
    R: 'b,
    M: 'b,
    Wrapper<'b, R, M>: IntoInner<'b>,
{
    fn get_mut(&'b mut self) -> <Wrapper<'b, R, M> as IntoInner>::Target {
        let wrapper: Wrapper<'b, R, M> = match self {
            Wrapper::Ref { inner } => Wrapper::Ref { inner },
            Wrapper::Mut { inner } => Wrapper::Mut { inner },
        };
        wrapper.inner()
    }
}

impl<'a, R> From<&'a R> for Wrapper<'a, R, NeverMut> {
    fn from(inner: &'a R) -> Self {
        Wrapper::Ref { inner }
    }
}

impl<'a, M> From<&'a mut M> for Wrapper<'a, NeverRef, M> {
    fn from(inner: &'a mut M) -> Self {
        Wrapper::Mut { inner }
    }
}

trait IntoInner<'a> {
    type Target: Executor<'a, Database = Postgres>;
    fn inner(self) -> Self::Target;
}

impl<'a, R> IntoInner<'a> for Wrapper<'a, R, NeverMut>
where
    &'a R: Executor<'a, Database = Postgres>,
{
    type Target = &'a R;

    fn inner(self) -> &'a R {
        match self {
            Wrapper::Ref { inner } => inner,
            Wrapper::Mut { inner } => match inner.inner {},
        }
    }
}

impl<'a, M> IntoInner<'a> for Wrapper<'a, NeverRef, M>
where
    &'a mut M: Executor<'a, Database = Postgres>,
{
    type Target = &'a mut M;

    fn inner(self) -> &'a mut M {
        match self {
            Wrapper::Ref { inner } => match inner.inner {},
            Wrapper::Mut { inner } => inner,
        }
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
    }

    async fn execute<'a, R, M, I>(executor: I)
    where
        R: 'a,
        M: 'a,
        I: Into<Wrapper<'a, R, M>>,
        for<'b> Wrapper<'b, R, M>: IntoInner<'b>,
    {
        let mut wrapper = executor.into();

        let res = sqlx::query("select 1 + 1")
            .try_map(|row: PgRow| row.try_get::<i32, _>(0))
            .fetch_one(wrapper.get_mut())
            .await
            .unwrap();
        assert_eq!(2, res);

        let res = sqlx::query("select 2 + 2")
            .try_map(|row: PgRow| row.try_get::<i32, _>(0))
            .fetch_one(wrapper.get_mut())
            .await
            .unwrap();
        assert_eq!(4, res);
    }
}

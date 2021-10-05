use core::convert::Infallible;
use sqlx::{Database, Executor, Postgres};

enum NeverRef {}
enum NeverMut {}

enum Wrapper<'a, R, M, N> {
    Ref { inner: &'a R },
    Mut { inner: &'a mut M },
    Phantom { inner: &'a mut N },
}

impl<'a, 'b, R: 'a, M: 'a, N: 'a> Wrapper<'a, R, M, N>
where
    Wrapper<'b, R, M, N>: IntoInner,
{
    fn get_mut(&'b mut self) -> <Wrapper<'b, R, M, N> as IntoInner>::Inner {
        let wrapper: Wrapper<'b, R, M, N> = match self {
            Wrapper::Ref { inner } => Wrapper::Ref { inner },
            Wrapper::Mut { inner } => Wrapper::Mut { inner },
            Wrapper::Phantom { inner } => Wrapper::Phantom { inner },
        };
        wrapper.inner()
    }
}

impl<'a, R> From<&'a R> for Wrapper<'a, R, Infallible, NeverRef> {
    fn from(inner: &'a R) -> Self {
        Wrapper::Ref { inner }
    }
}

impl<'a, M> From<&'a mut M> for Wrapper<'a, Infallible, M, NeverMut> {
    fn from(inner: &'a mut M) -> Self {
        Wrapper::Mut { inner }
    }
}

trait IntoInner {
    type Inner;
    fn inner(self) -> Self::Inner;
}

impl<'a, R> IntoInner for Wrapper<'a, R, Infallible, NeverRef> {
    type Inner = &'a R;

    fn inner(self) -> &'a R {
        match self {
            Wrapper::Ref { inner } => inner,
            Wrapper::Mut { inner } => match *inner {},
            Wrapper::Phantom { inner } => match *inner {},
        }
    }
}

impl<'a, M> IntoInner for Wrapper<'a, Infallible, M, NeverMut> {
    type Inner = &'a mut M;

    fn inner(self) -> &'a mut M {
        match self {
            Wrapper::Ref { inner } => match *inner {},
            Wrapper::Mut { inner } => inner,
            Wrapper::Phantom { inner } => match *inner {},
        }
    }
}

trait Bound<'a>: IntoInner<Inner = Self::Target> {
    type Target: Executor<'a, Database = Postgres>;
}

impl<'a, R> Bound<'a> for Wrapper<'a, R, Infallible, NeverRef>
where
    &'a R: Executor<'a, Database = Postgres>,
{
    type Target = &'a R;
}

impl<'a, M> Bound<'a> for Wrapper<'a, Infallible, M, NeverMut>
where
    &'a mut M: Executor<'a, Database = Postgres>,
{
    type Target = &'a mut M;
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

    async fn execute<'a, R: 'a, M: 'a, N: 'a, I>(executor: I)
    where
        I: Into<Wrapper<'a, R, M, N>>,
        for<'b> Wrapper<'b, R, M, N>: Bound<'b>,
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

use futures_util::future::BoxFuture;
use sqlx::{Acquire, Database, Error, Executor, Transaction};
use std::convert::Infallible;
use std::marker::{PhantomData, PhantomPinned};
use std::ops::DerefMut;

struct Never<DB, C> {
    inner: Infallible,
    phantom: PhantomData<(DB, C)>,
}

impl<DB, C> Unpin for Never<DB, C> {}

impl<'a, DB, C> Acquire<'a> for &'_ Never<DB, C>
where
    DB: Database,
    C: DerefMut<Target = DB::Connection> + Send,
{
    type Database = DB;
    type Connection = C;

    fn acquire(self) -> BoxFuture<'a, Result<Self::Connection, Error>> {
        match self.inner {}
    }

    fn begin(self) -> BoxFuture<'a, Result<Transaction<'a, Self::Database>, Error>> {
        match self.inner {}
    }
}

impl<'a, DB, C> Acquire<'a> for &'_ mut Never<DB, C>
where
    DB: Database,
    C: DerefMut<Target = DB::Connection> + Send,
{
    type Database = DB;
    type Connection = C;

    fn acquire(self) -> BoxFuture<'a, Result<Self::Connection, Error>> {
        match self.inner {}
    }

    fn begin(self) -> BoxFuture<'a, Result<Transaction<'a, Self::Database>, Error>> {
        match self.inner {}
    }
}

enum Wrapper<'a, R, M>
where
    R: Unpin,
    M: Unpin,
{
    Ref {
        inner: &'a R,
        _phantom: PhantomPinned,
    },
    Mut {
        inner: &'a mut M,
    },
}

impl<'a, DB, C, R, M> Acquire<'a> for Wrapper<'a, R, M>
where
    DB: Database,
    C: DerefMut<Target = DB::Connection> + Send,
    R: Unpin,
    M: Unpin,
    &'a R: Acquire<'a, Database = DB, Connection = C>,
    &'a mut M: Acquire<'a, Database = DB, Connection = C>,
{
    type Database = DB;
    type Connection = C;

    fn acquire(self) -> BoxFuture<'a, Result<Self::Connection, Error>> {
        match self {
            Wrapper::Ref { inner, _phantom } => inner.acquire(),
            Wrapper::Mut { inner } => inner.acquire(),
        }
    }

    fn begin(self) -> BoxFuture<'a, Result<Transaction<'a, Self::Database>, Error>> {
        match self {
            Wrapper::Ref { inner, _phantom } => inner.begin(),
            Wrapper::Mut { inner } => inner.begin(),
        }
    }
}

impl<'a, DB, C, R: Unpin> From<&'a R> for Wrapper<'a, R, Never<DB, C>>
where
    &'a R: Acquire<'a, Database = DB, Connection = C>,
{
    fn from(inner: &'a R) -> Self {
        Wrapper::Ref {
            inner,
            _phantom: PhantomPinned,
        }
    }
}

impl<'a, DB, C, M: Unpin> From<&'a mut M> for Wrapper<'a, Never<DB, C>, M>
where
    &'a mut M: Acquire<'a, Database = DB, Connection = C>,
{
    fn from(inner: &'a mut M) -> Self {
        Wrapper::Mut { inner }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::database::HasArguments;
    use sqlx::decode::Decode;
    use sqlx::{ColumnIndex, IntoArguments, PgPool, Row, Type};

    use super::*;

    #[tokio::test]
    async fn test() {
        let pool = PgPool::connect("hallo").await.unwrap();
        convert(&pool).await;

        let mut conn = pool.acquire().await.unwrap();
        let res = convert(&mut conn).await;
        println!("{:?}", res);
    }

    async fn convert<'a, DB, C, R, M, I>(input: I) -> i32
    where
        DB: Database,
        for<'c> <DB as HasArguments<'a>>::Arguments: IntoArguments<'c, DB>,
        for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
        C: DerefMut<Target = DB::Connection> + Send,
        R: Unpin + 'a,
        M: Unpin + 'a,
        &'a R: Acquire<'a, Database = DB, Connection = C>,
        &'a mut M: Acquire<'a, Database = DB, Connection = C>,
        I: Into<Wrapper<'a, R, M>>,
        for<'c> i32: Decode<'c, DB>,
        i32: Type<DB>,
        usize: ColumnIndex<DB::Row>,
    {
        let wrapper = input.into();
        let mut conn = wrapper.acquire().await.unwrap();

        sqlx::query("select 1 + 1")
            .try_map(|row: DB::Row| row.try_get::<i32, _>(0))
            .fetch_one(&mut *conn)
            .await
            .unwrap()
    }
}

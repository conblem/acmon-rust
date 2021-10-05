use async_trait::async_trait;

use super::account::AccountRepo;
use super::sqlx::Wrapper;
use sqlx::{Executor, Postgres};

struct PostgresAccountRepo<'a, R, M> {
    inner: Wrapper<'a, R, M>,
}

impl<'a, R, M> PostgresAccountRepo<'a, R, M>
where
    R: 'a,
    M: 'a,
{
    fn new<I>(inner: I) -> Self
    where
        I: Into<Wrapper<'a, R, M>>,
    {
        Self {
            inner: inner.into(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct PostgresAccount {
    id: i64,
}

#[async_trait]
impl<'a, R, M> AccountRepo for PostgresAccountRepo<'a, R, M>
where
    R: Send + 'a,
    M: Send + 'a,
    for<'b> Wrapper<'b, R, M>: Executor<'b, Database = Postgres>,
{
    async fn get_account(&mut self, input: &str) {
        let inner = Wrapper::<'_, R, M>::from(&mut self.inner);

        sqlx::query_as::<_, PostgresAccount>("SELECT * FROM ACCOUNT").fetch_many(inner);
    }
}

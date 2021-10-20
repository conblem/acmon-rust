use sea_query::{Iden, PostgresQueryBuilder, QueryBuilder, Value, Values};
use sqlx::database::HasArguments;
use sqlx::{Database, IntoArguments, Postgres};

mod account;
mod executor;
mod limit;
mod postgres;

pub(super) trait Entity: Send + Sync + Unpin + 'static {
    type Iden: Iden + Copy + Send + Sync + 'static;

    fn table_iden() -> Self::Iden;
    fn id_iden() -> Self::Iden;
    fn id(&self) -> i32;
    fn id_mut(&mut self) -> &mut i32;
    fn columns_iden() -> Vec<Self::Iden>;
    fn values(&self) -> Vec<(Self::Iden, Value)>;
}

type Query<'a, DB: Database> = sqlx::query::Query<'a, DB, <DB as HasArguments<'a>>::Arguments>;
type QueryAs<'a, DB: Database, T> =
    sqlx::query::QueryAs<'a, DB, T, <DB as HasArguments<'a>>::Arguments>;

pub(super) trait IsQueryBuilder<'a>: Database
where
    for<'b> <Self as HasArguments<'a>>::Arguments: IntoArguments<'b, Self>,
{
    type QueryBuilder: QueryBuilder;

    fn query_builder() -> Self::QueryBuilder;
    fn bind_query<'b>(query: Query<'b, Self>, params: &'a Values) -> Query<'b, Self> where 'b: 'a;
    fn bind_query_as<'b, T>(
        query: QueryAs<'b, Self, T>,
        params: &'b Values,
    ) -> QueryAs<'b, Self, T> where 'b: 'a;
}

sea_query::sea_query_driver_postgres!();

impl <'a> IsQueryBuilder<'a> for Postgres {
    type QueryBuilder = PostgresQueryBuilder;

    fn query_builder() -> Self::QueryBuilder {
        PostgresQueryBuilder
    }

    fn bind_query<'b>(query: Query<'b, Self>, params: &'b Values) -> Query<'b, Self> where 'b: 'a {
        sea_query_driver_postgres::bind_query(query, params)
    }

    fn bind_query_as<'b, T>(
        query: QueryAs<'b, Self, T>,
        params: &'b Values,
    ) -> QueryAs<'b, Self, T> where 'b: 'a {
        sea_query_driver_postgres::bind_query_as(query, params)
    }
}

use async_trait::async_trait;
use sqlx::FromRow;
use std::error::Error;

#[derive(FromRow)]
pub(super) struct AccountStruct {
    pub(super) id: i32,
    pub(super) email: String,
}

#[async_trait]
pub(super) trait Repo<E> {
    type Error: Error + 'static;

    async fn read(&mut self, id: i32) -> Result<Option<E>, Self::Error>;
    async fn read_all(&mut self) -> Result<Vec<E>, Self::Error>;
    async fn create(&mut self, account: &mut E) -> Result<(), Self::Error>;
    async fn update(&mut self, account: &E) -> Result<(), Self::Error>;
    async fn delete(&mut self, account: E) -> Result<(), Self::Error>;
}

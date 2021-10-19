use async_trait::async_trait;
use std::error::Error;

pub(super) struct Account {
    pub(super) id: i32,
    pub(super) email: String,
}

#[async_trait]
pub(super) trait AccountRepo {
    type Error: Error + 'static;

    async fn get_accounts(&mut self) -> Result<Vec<Account>, Self::Error>;
    async fn create_account(&mut self, account: &mut Account) -> Result<(), Self::Error>;
    async fn update_account(&mut self, account: &Account) -> Result<(), Self::Error>;
    async fn delete_account(&mut self, account: Account) -> Result<(), Self::Error>;
}

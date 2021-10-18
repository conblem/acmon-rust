use async_trait::async_trait;

pub(super) struct Account {
    pub(super) id: i64,
}

#[async_trait]
pub(super) trait AccountRepo {
    async fn get_accounts(&mut self) -> Vec<Account>;
}

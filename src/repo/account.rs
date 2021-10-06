use async_trait::async_trait;

#[async_trait]
pub(super) trait AccountRepo {
    async fn get_account(&mut self, input: &str);
}

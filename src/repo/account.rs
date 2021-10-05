use async_trait::async_trait;
use etcd_client::Client;

#[async_trait]
pub(super) trait AccountRepo {
    async fn get_account(&mut self, input: &str);
}

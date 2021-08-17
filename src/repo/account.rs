use async_trait::async_trait;
use etcd_client::Client;

#[async_trait]
trait AccountRepo {
    async fn get_account(input: &str);
}

struct EtcdAccountRepo(Client);


#[async_trait]
impl AccountRepo for EtcdAccountRepo {
    async fn get_account(_input: &str) {
    }
}
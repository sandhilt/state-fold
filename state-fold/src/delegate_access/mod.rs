pub mod error;
pub mod fold_middleware;
pub mod sync_middleware;

pub use error::AccessError;
pub use fold_middleware::FoldMiddleware;
pub use sync_middleware::SyncMiddleware;

mod partition_events;
mod utils;

#[cfg(test)]
mod tests {
    use crate::test_utils::mocks::MockFold;
    use crate::StateFoldEnvironment;
    use std::sync::Arc;

    use ethers::providers::Middleware;
    use offchain_utils::offchain_core::ethers;

    use super::{fold_middleware, sync_middleware};
    use crate::test_utils;

    #[tokio::test]
    async fn test_sync_fold() {
        let (_handle, provider) = test_utils::new_geth().await;
        let genesis = provider.get_block_number().await.unwrap();
        let contract =
            test_utils::deploy_simple_storage(Arc::clone(&provider)).await;
        let account = provider.get_accounts().await.unwrap()[0];
        let deployed_address = contract.address();

        let env = StateFoldEnvironment::new(
            Arc::clone(&provider),
            4,
            genesis,
            vec![],
            1,
            (),
        );

        let block0 = test_utils::get_current_block(provider.as_ref()).await;
        let block1 = test_utils::set_value_get_block::<MockFold, _>(
            &env, &contract, "this",
        )
        .await;
        let block2 = test_utils::set_value_get_block::<MockFold, _>(
            &env, &contract, "that",
        )
        .await;
        let block3 = test_utils::set_value_get_block::<MockFold, _>(
            &env, &contract, "other",
        )
        .await;

        sync_middleware::tests::sync_query_test(
            account,
            deployed_address,
            &env,
            (&block0, &block1, &block2, &block3),
        )
        .await;

        fold_middleware::tests::fold_query_test(
            account,
            deployed_address,
            &env,
            (&block0, &block1, &block2, &block3),
        )
        .await;
    }
}

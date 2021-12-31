use crate::{FoldMiddleware, Foldable, StateFoldEnvironment, SyncMiddleware};

use ethers::providers::{FromErr, Middleware, MockProvider};
use ethers::types::{BlockId, BlockNumber, Bloom, H256, U256, U64};
use offchain_utils::offchain_core::ethers;
use offchain_utils::offchain_core::types::Block;

use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct MockError;
impl fmt::Display for MockError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MockError")
    }
}
impl std::error::Error for MockError {}
impl FromErr<MockError> for MockError {
    fn from(src: MockError) -> Self {
        src
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MockFold;

#[async_trait]
impl Foldable for MockFold {
    type InitialState = ();
    type Error = MockError;
    type UserData = ();

    async fn sync<M: Middleware>(
        _initial_state: &Self::InitialState,
        _block: &Block,
        _env: &StateFoldEnvironment<M, ()>,
        _access: Arc<SyncMiddleware<M>>,
    ) -> Result<Self, Self::Error> {
        unreachable!()
    }

    async fn fold<M: Middleware>(
        _previous_state: &Self,
        _block: &Block,
        _env: &StateFoldEnvironment<M, ()>,
        _access: Arc<FoldMiddleware<M>>,
    ) -> Result<Self, Self::Error> {
        unreachable!()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IncrementFold {
    pub(crate) low_hash: u64,
    pub(crate) n: u64,
    pub(crate) initial_state: u64,
}

#[async_trait]
impl Foldable for IncrementFold {
    type InitialState = u64;
    type Error = MockError;
    type UserData = ();

    async fn sync<M: Middleware>(
        initial_state: &Self::InitialState,
        block: &Block,
        _env: &StateFoldEnvironment<M, ()>,
        _access: Arc<SyncMiddleware<M>>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            low_hash: block.hash.to_low_u64_be(),
            n: block.number.as_u64() + initial_state,
            initial_state: *initial_state,
        })
    }

    async fn fold<M: Middleware>(
        previous_state: &Self,
        block: &Block,
        _env: &StateFoldEnvironment<M, ()>,
        _access: Arc<FoldMiddleware<M>>,
    ) -> Result<Self, Self::Error> {
        assert_eq!(
            previous_state.n + 1,
            block.number.as_u64() + previous_state.initial_state
        );

        Ok(Self {
            low_hash: block.hash.to_low_u64_be(),
            n: previous_state.n + 1,
            initial_state: previous_state.initial_state,
        })
    }
}

#[derive(Debug)]
pub(crate) struct MockMiddleware {
    chain: Mutex<HashMap<H256, Block>>,
    block_count: Mutex<U64>,
    latest_block: Mutex<H256>,
    deepest_block: Mutex<U64>,
}

impl MockMiddleware {
    pub(crate) async fn new(initial_block_count: u64) -> Arc<Self> {
        assert!(initial_block_count > 0);

        let latest_block = H256::zero();

        let this = Self {
            chain: Mutex::new(HashMap::new()),
            block_count: Mutex::new(U64::from(0)),
            latest_block: Mutex::new(latest_block),
            deepest_block: Mutex::new(U64::from(0)),
        };

        this.chain.lock().await.insert(
            latest_block,
            Block {
                number: *this.block_count.lock().await,
                hash: latest_block,
                parent_hash: latest_block,

                timestamp: U256::zero(),
                logs_bloom: Bloom::zero(),
            },
        );

        let mut previous_hash = *this.latest_block.lock().await;
        for _ in 0..initial_block_count {
            previous_hash = this.add_block(previous_hash).await.unwrap();
        }

        Arc::new(this)
    }

    pub(crate) async fn add_block(&self, parent_hash: H256) -> Option<H256> {
        let new_number =
            self.chain.lock().await.get(&parent_hash)?.number + U64::from(1);
        let new_hash = self.new_hash().await;
        let new_block = Block {
            number: new_number,
            hash: new_hash,
            parent_hash,

            timestamp: U256::zero(),
            logs_bloom: Bloom::zero(),
        };
        self.chain.lock().await.insert(new_hash, new_block);
        *self.latest_block.lock().await = new_hash;

        let mut deepest_block = self.deepest_block.lock().await;
        if new_number > *deepest_block {
            *deepest_block = new_number;
        }

        Some(new_hash)
    }

    pub(crate) async fn get_block(&self, hash: H256) -> Option<Block> {
        self.chain.lock().await.get(&hash).map(|x| x.clone().into())
    }

    pub(crate) async fn get_block_with_number(
        &self,
        number: U64,
    ) -> Option<Block> {
        self.get_block_with_number_from(number, *self.latest_block.lock().await)
            .await
    }

    pub(crate) async fn get_block_with_number_from(
        &self,
        number: U64,
        tip: H256,
    ) -> Option<Block> {
        let mut current_hash = tip;

        loop {
            match self.chain.lock().await.get(&current_hash) {
                Some(block) => {
                    if block.number == number {
                        return Some(block.clone().into());
                    } else if block.number == 0.into() {
                        return None;
                    } else {
                        current_hash = block.parent_hash;
                    }
                }
                None => break,
            }
        }

        None
    }

    pub(crate) async fn get_latest_block(&self) -> Option<Block> {
        self.chain
            .lock()
            .await
            .get(&*self.latest_block.lock().await)
            .map(|x| x.clone().into())
    }

    async fn new_hash(&self) -> H256 {
        *self.block_count.lock().await += U64::from(1);
        H256::from_low_u64_be(self.block_count.lock().await.as_u64())
    }
}

#[async_trait]
impl Middleware for MockMiddleware {
    type Error = MockError;
    type Provider = MockProvider;
    type Inner = Self;

    fn inner(&self) -> &Self {
        unreachable!()
    }

    async fn get_block_number(&self) -> Result<U64, Self::Error> {
        Ok(MockMiddleware::get_latest_block(self).await.unwrap().number)
    }

    async fn get_block<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<ethers::types::Block<H256>>, Self::Error> {
        let b = match block_hash_or_number.into() {
            BlockId::Hash(h) => {
                MockMiddleware::get_block(self, h).await.unwrap()
            }
            BlockId::Number(BlockNumber::Number(n)) => {
                MockMiddleware::get_block_with_number(self, n)
                    .await
                    .unwrap()
            }
            BlockId::Number(BlockNumber::Latest) => {
                MockMiddleware::get_latest_block(self).await.unwrap()
            }
            x => panic!("get_block not number {:?}", x),
        };

        let mut ret = ethers::types::Block::default();
        ret.hash = Some(b.hash);
        ret.number = Some(b.number);
        ret.parent_hash = b.parent_hash;
        ret.timestamp = U256::zero();
        ret.logs_bloom = Some(Bloom::zero());

        Ok(Some(ret))
    }
}

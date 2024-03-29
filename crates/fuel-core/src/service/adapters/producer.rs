use crate::{
    database::Database,
    service::{
        adapters::{
            BlockProducerAdapter,
            ExecutorAdapter,
            MaybeRelayerAdapter,
            StaticGasPrice,
            TransactionsSource,
            TxPoolAdapter,
        },
        sub_services::BlockProducerService,
    },
};
use fuel_core_executor::executor::OnceTransactionsSource;
use fuel_core_producer::{
    block_producer::gas_price::{
        GasPriceParams,
        GasPriceProvider,
    },
    ports::TxPool,
};
use fuel_core_storage::{
    iter::{
        IterDirection,
        IteratorOverTable,
    },
    not_found,
    tables::{
        ConsensusParametersVersions,
        FuelBlocks,
        StateTransitionBytecodeVersions,
    },
    transactional::Changes,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        header::{
            ConsensusParametersVersion,
            StateTransitionBytecodeVersion,
        },
        primitives::DaBlockHeight,
    },
    fuel_tx,
    fuel_tx::Transaction,
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    services::{
        block_producer::Components,
        executor::{
            ExecutionTypes,
            Result as ExecutorResult,
            TransactionExecutionStatus,
            UncommittedResult,
        },
    },
};
use std::{
    borrow::Cow,
    sync::Arc,
};

impl BlockProducerAdapter {
    pub fn new(block_producer: BlockProducerService) -> Self {
        Self {
            block_producer: Arc::new(block_producer),
        }
    }
}

#[async_trait::async_trait]
impl TxPool for TxPoolAdapter {
    type TxSource = TransactionsSource;

    fn get_source(&self, block_height: BlockHeight) -> Self::TxSource {
        TransactionsSource::new(self.service.clone(), block_height)
    }
}

impl fuel_core_producer::ports::Executor<TransactionsSource> for ExecutorAdapter {
    fn execute_without_commit(
        &self,
        component: Components<TransactionsSource>,
    ) -> ExecutorResult<UncommittedResult<Changes>> {
        self._execute_without_commit(ExecutionTypes::Production(component))
    }
}

impl fuel_core_producer::ports::Executor<Vec<Transaction>> for ExecutorAdapter {
    fn execute_without_commit(
        &self,
        component: Components<Vec<Transaction>>,
    ) -> ExecutorResult<UncommittedResult<Changes>> {
        let Components {
            header_to_produce,
            transactions_source,
            gas_price,
            coinbase_recipient,
        } = component;
        self._execute_without_commit(ExecutionTypes::Production(Components {
            header_to_produce,
            transactions_source: OnceTransactionsSource::new(transactions_source),
            gas_price,
            coinbase_recipient,
        }))
    }
}

impl fuel_core_producer::ports::DryRunner for ExecutorAdapter {
    fn dry_run(
        &self,
        block: Components<Vec<fuel_tx::Transaction>>,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<TransactionExecutionStatus>> {
        self._dry_run(block, utxo_validation)
    }
}

#[async_trait::async_trait]
impl fuel_core_producer::ports::Relayer for MaybeRelayerAdapter {
    async fn get_latest_da_blocks_with_costs(
        &self,
        starting_from: &DaBlockHeight,
    ) -> anyhow::Result<Vec<(DaBlockHeight, u64)>> {
        #[cfg(feature = "relayer")]
        {
            if let Some(sync) = self.relayer_synced.as_ref() {
                sync.await_at_least_synced(starting_from).await?;
                let highest = sync.get_finalized_da_height()?;
                (starting_from.0..=highest.0)
                    .map(|height| get_gas_cost_for_height(height, sync))
                    .collect()
            } else {
                Ok(Vec::new())
            }
        }
        #[cfg(not(feature = "relayer"))]
        {
            anyhow::ensure!(
                **starting_from == 0,
                "Cannot have a da height above zero without a relayer"
            );
            // If the relayer is not enabled, then all blocks are zero.
            Ok(Vec::new())
        }
    }
}

#[cfg(feature = "relayer")]
fn get_gas_cost_for_height(
    height: u64,
    sync: &fuel_core_relayer::SharedState<
        Database<crate::database::database_description::relayer::Relayer>,
    >,
) -> anyhow::Result<(DaBlockHeight, u64)> {
    let da_height = DaBlockHeight(height);
    let cost = sync
        .database()
        .storage::<fuel_core_relayer::storage::EventsHistory>()
        .get(&da_height)?
        .map(|cow| cow.into_owned())
        .unwrap_or_default()
        .iter()
        .map(|event| event.cost())
        .sum();
    Ok((da_height, cost))
}

impl fuel_core_producer::ports::BlockProducerDatabase for Database {
    fn get_block(&self, height: &BlockHeight) -> StorageResult<Cow<CompressedBlock>> {
        self.storage::<FuelBlocks>()
            .get(height)?
            .ok_or(not_found!(FuelBlocks))
    }

    fn block_header_merkle_root(&self, height: &BlockHeight) -> StorageResult<Bytes32> {
        self.storage::<FuelBlocks>().root(height).map(Into::into)
    }

    fn latest_consensus_parameters_version(
        &self,
    ) -> StorageResult<ConsensusParametersVersion> {
        let (version, _) = self
            .iter_all::<ConsensusParametersVersions>(Some(IterDirection::Reverse))
            .next()
            .ok_or(not_found!(ConsensusParametersVersions))??;

        Ok(version)
    }

    fn latest_state_transition_bytecode_version(
        &self,
    ) -> StorageResult<StateTransitionBytecodeVersion> {
        let (version, _) = self
            .iter_all::<StateTransitionBytecodeVersions>(Some(IterDirection::Reverse))
            .next()
            .ok_or(not_found!(StateTransitionBytecodeVersions))??;

        Ok(version)
    }
}

impl GasPriceProvider for StaticGasPrice {
    fn gas_price(&self, _block_height: GasPriceParams) -> Option<u64> {
        Some(self.gas_price)
    }
}

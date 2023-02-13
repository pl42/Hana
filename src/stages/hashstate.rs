use crate::{
    crypto::keccak256,
    etl::collector::*,
    kv::{mdbx::*, tables},
    models::*,
    stagedsync::{stage::*, stages::*},
    stages::stage_util::should_do_clean_promotion,
    upsert_hashed_storage_value,
};
use anyhow::format_err;
use async_trait::async_trait;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::pin;
use tracing::*;

pub fn promote_clean_accounts<'db, E>(
    txn: &MdbxTransaction<'db, RW, E>,
    temp_dir: &TempDir,
) -> anyhow::Result<()>
where
    E: EnvironmentKind,
{
    txn.clear_table(tables::HashedAccount)?;

    let mut collector_account =
        TableCollector::<tables::HashedAccount>::new(temp_dir, OPTIMAL_BUFFER_CAPACITY);

    let mut src = txn.cursor(tables::Account)?;
    src.first()?;
    let mut i = 0;
    let walker = src.walk(None);
    pin!(walker);
    while let Some((address, account)) = walker.next().transpose()? {
        collector_account.push(keccak256(address), account);

        i += 1;
        if i % 5_000_000 == 0 {
            debug!("Converted {} entries", i);
        }
    }

    debug!("Loading hashed entries");
    let mut dst = txn.cursor(tables::HashedAccount.erased())?;
    collector_account.load(&mut dst)?;

    Ok(())
}

pub fn promote_clean_storage<'db, E>(
    txn: &MdbxTransaction<'db, RW, E>,
    path: &TempDir,
) -> anyhow::Result<()>
where
    E: EnvironmentKind,
{
    txn.clear_table(tables::HashedStorage)?;

    let mut collector_storage =
        TableCollector::<tables::HashedStorage>::new(path, OPTIMAL_BUFFER_CAPACITY);

    let mut src = txn.cursor(tables::Storage)?;
    src.first()?;
    let mut i = 0;
    let walker = src.walk(None);
    pin!(walker);
    while let Some((address, (location, value))) = walker.next().transpose()? {
        collector_storage.push(keccak256(address), (keccak256(location), value));

        i += 1;
        if i % 5_000_000 == 0 {
            debug!("Converted {} entries", i);
        }
    }

    debug!("Loading hashed entries");
    let mut dst = txn.cursor(tables::HashedStorage.erased())?;
    collector_storage.load(&mut dst)?;

    Ok(())
}

fn promote_accounts<E>(
    tx: &MdbxTransaction<'_, RW, E>,
    stage_progress: BlockNumber,
) -> anyhow::Result<()>
where
    E: EnvironmentKind,
{
    let changeset_table = tx.cursor(tables::AccountChangeSet)?;
    let mut account_table = tx.cursor(tables::Account)?;
    let mut target_table = tx.cursor(tables::HashedAccount)?;

    let starting_block = stage_progress + 1;

    let walker = changeset_table.walk(Some(starting_block));
    pin!(walker);

    while let Some((_, tables::AccountChange { address, .. })) = walker.next().transpose()? {
        let hashed_address = || keccak256(address);
        if let Some((_, account)) = account_table.seek_exact(address)? {
            target_table.upsert((hashed_address)(), account)?;
        } else if target_table.seek_exact((hashed_address)())?.is_some() {
            target_table.delete_current()?;
        }
    }

    Ok(())
}

fn promote_storage<E>(
    tx: &MdbxTransaction<'_, RW, E>,
    stage_progress: BlockNumber,
) -> anyhow::Result<()>
where
    E: EnvironmentKind,
{
    let changeset_table = tx.cursor(tables::StorageChangeSet)?;
    let mut storage_table = tx.cursor(tables::Storage)?;
    let mut target_table = tx.cursor(tables::HashedStorage)?;

    let starting_block = stage_progress + 1;

    let mut walker = changeset_table.walk(Some(starting_block));

    while let Some((
        tables::StorageChangeKey { address, .. },
        tables::StorageChange { location, .. },
    )) = walker.next().transpose()?
    {
        let hashed_address = keccak256(address);
        let hashed_location = keccak256(location);
        let mut v = U256::ZERO;
        if let Some((found_location, value)) = storage_table.seek_both_range(address, location)? {
            if location == found_location {
                v = value;
            }
        }
        upsert_hashed_storage_value(&mut target_table, hashed_address, hashed_location, v)?;
    }

    Ok(())
}

#[derive(Debug)]
pub struct HashState {
    temp_dir: Arc<TempDir>,
    clean_promotion_threshold: u64,
}

impl HashState {
    pub fn new(temp_dir: Arc<TempDir>, clean_promotion_threshold: Option<u64>) -> Self {
        Self {
            temp_dir,
            clean_promotion_threshold: clean_promotion_threshold
                .unwrap_or(30_000_000_u64 * 1_000_000_u64),
        }
    }
}

#[async_trait]
impl<'db, E> Stage<'db, E> for HashState
where
    E: EnvironmentKind,
{
    fn id(&self) -> StageId {
        HASH_STATE
    }

    async fn execute<'tx>(
        &mut self,
        tx: &'tx mut MdbxTransaction<'db, RW, E>,
        input: StageInput,
    ) -> Result<ExecOutput, StageError>
    where
        'db: 'tx,
    {
        let genesis = BlockNumber(0);
        let past_progress = input.stage_progress.unwrap_or(genesis);
        let max_block = input
            .previous_stage
            .ok_or_else(|| format_err!("Cannot be first stage"))?
            .1;

        if should_do_clean_promotion(
            tx,
            genesis,
            past_progress,
            max_block,
            self.clean_promotion_threshold,
        )? {
            info!("Generating hashed accounts");
            promote_clean_accounts(tx, &*self.temp_dir)?;
            info!("Generating hashed storage");
            promote_clean_storage(tx, &*self.temp_dir)?;
        } else {
            info!("Incrementally hashing accounts");
            promote_accounts(tx, past_progress)?;
            info!("Incrementally hashing storage");
            promote_storage(tx, past_progress)?;
        }

        Ok(ExecOutput::Progress {
            stage_progress: max_block,
            done: true,
            reached_tip: true,
        })
    }

    async fn unwind<'tx>(
        &mut self,
        tx: &'tx mut MdbxTransaction<'db, RW, E>,
        input: UnwindInput,
    ) -> anyhow::Result<UnwindOutput>
    where
        'db: 'tx,
    {
        info!("Unwinding hashed accounts");
        let mut hashed_account_cur = tx.cursor(tables::HashedAccount)?;
        let walker = tx.cursor(tables::AccountChangeSet)?.walk_back(None);
        pin!(walker);
        while let Some((block_number, tables::AccountChange { address, account })) =
            walker.next().transpose()?
        {
            if block_number > input.unwind_to {
                let hashed_address = keccak256(address);

                if let Some(account) = account {
                    hashed_account_cur.put(hashed_address, account)?
                } else if hashed_account_cur.seek_exact(hashed_address)?.is_some() {
                    hashed_account_cur.delete_current()?
                }
            } else {
                break;
            }
        }

        info!("Unwinding hashed storage");
        let mut hashed_storage_cur = tx.cursor(tables::HashedStorage)?;
        let walker = tx.cursor(tables::StorageChangeSet)?.walk_back(None);
        pin!(walker);
        while let Some((
            tables::StorageChangeKey {
                block_number,
                address,
            },
            tables::StorageChange { location, value },
        )) = walker.next().transpose()?
        {
            if block_number > input.unwind_to {
                let hashed_address = keccak256(address);
                let hashed_location = keccak256(location);
                upsert_hashed_storage_value(
                    &mut hashed_storage_cur,
                    hashed_address,
                    hashed_location,
                    value,
                )?;
            } else {
                break;
            }
        }

        Ok(UnwindOutput {
            stage_progress: input.unwind_to,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        execution::{address::*, *},
        kv::new_mem_database,
        res::chainspec::MAINNET,
        u256_to_h256, Buffer, StateWriter,
    };
    use hex_literal::*;
    use std::time::Instant;

    #[tokio::test]
    async fn stage_hashstate() {
        let db = new_mem_database().unwrap();
        let mut tx = db.begin_mutable().unwrap();

        let mut gas = 0;
        tx.set(tables::TotalGas, 0.into(), gas).unwrap();

        let block_number = BlockNumber(1);
        let miner = hex!("5a0b54d5dc17e0aadc383d2db43b0a0d3e029c4c").into();
        let gas_limit = 100_000;

        // This contract initially sets its 0th storage to 0x2a
        // and its 1st storage to 0x01c9.
        // When called, it updates its 0th storage to the input provided.
        let contract_code = hex!("600035600055").to_vec();
        let deployment_code = hex!("602a6000556101c960015560068060166000396000f3").to_vec();

        let sender = hex!("b685342b8c54347aad148e1f22eff3eb3eb29391").into();

        let mut header = PartialHeader {
            number: block_number,
            beneficiary: miner,
            gas_limit,
            gas_used: 63_820,
            ..PartialHeader::empty()
        };

        let transaction = move |nonce, value, action, input| MessageWithSender {
            message: Message::Legacy {
                chain_id: None,
                nonce,
                gas_price: U256::from(20 * GIGA),
                gas_limit,
                action,
                value,
                input,
            },
            sender,
        };

        let mut body = BlockBodyWithSenders {
            transactions: vec![(transaction)(
                0,
                0.as_u256(),
                TransactionAction::Create,
                deployment_code.into_iter().chain(contract_code).collect(),
            )],
            ommers: vec![],
        };

        let mut buffer = Buffer::new(&tx, None);

        let sender_account = Account {
            balance: ETHER.into(),
            ..Account::default()
        };

        buffer.update_account(sender, None, Some(sender_account));

        // ---------------------------------------
        // Execute first block
        // ---------------------------------------
        execute_block(&mut buffer, &MAINNET, &header, &body).unwrap();

        gas += header.gas_used;
        tx.set(tables::TotalGas, header.number, gas).unwrap();

        let contract_address = create_address(sender, 0);

        // ---------------------------------------
        // Execute second block
        // ---------------------------------------

        let new_val = hex!("000000000000000000000000000000000000000000000000000000000000003e");

        let block_number = BlockNumber(2);

        header.number = block_number;
        header.gas_used = 26_201;

        body.transactions = vec![(transaction)(
            1,
            1000.as_u256(),
            TransactionAction::Call(contract_address),
            new_val.to_vec().into(),
        )];

        execute_block(&mut buffer, &MAINNET.clone(), &header, &body).unwrap();

        gas += header.gas_used;
        tx.set(tables::TotalGas, header.number, gas).unwrap();

        // ---------------------------------------
        // Execute third block
        // ---------------------------------------

        let new_val = 0x3b;

        let block_number = BlockNumber(3);

        header.number = block_number;
        header.gas_used = 26_201;

        body.transactions = vec![(transaction)(
            2,
            1000.as_u256(),
            TransactionAction::Call(contract_address),
            u256_to_h256(new_val.as_u256()).0.to_vec().into(),
        )];

        execute_block(&mut buffer, &MAINNET.clone(), &header, &body).unwrap();

        buffer.write_to_db().unwrap();

        gas += header.gas_used;
        tx.set(tables::TotalGas, header.number, gas).unwrap();

        // ---------------------------------------
        // Execute stage forward
        // ---------------------------------------
        assert_eq!(
            HashState {
                temp_dir: Arc::new(TempDir::new().unwrap()),
                clean_promotion_threshold: u64::MAX,
            }
            .execute(
                &mut tx,
                StageInput {
                    restarted: false,
                    first_started_at: (Instant::now(), None),
                    previous_stage: Some((EXECUTION, BlockNumber(3))),
                    stage_progress: None,
                },
            )
            .await
            .unwrap(),
            ExecOutput::Progress {
                stage_progress: BlockNumber(3),
                done: true,
                reached_tip: true,
            }
        );

        // ---------------------------------------
        // Check hashed account
        // ---------------------------------------

        let mut hashed_address_table = tx.cursor(tables::HashedAccount).unwrap();
        let sender_keccak = keccak256(sender);
        let (_, account) = hashed_address_table
            .seek_exact(sender_keccak)
            .unwrap()
            .unwrap();
        assert_eq!(account.nonce, 3);
        assert!(account.balance < ETHER);

        // ---------------------------------------
        // Check hashed storage
        // ---------------------------------------

        let hashed_storage_cursor = tx.cursor(tables::HashedStorage).unwrap();

        let k = keccak256(contract_address);
        let walker = hashed_storage_cursor.walk(Some(k));
        pin!(walker);

        for (location, expected_value) in [(0, new_val), (1, 0x01c9)] {
            let (wk, (hashed_location, value)) = walker.next().transpose().unwrap().unwrap();
            assert_eq!(k, wk);
            assert_eq!(hashed_location, keccak256(u256_to_h256(location.as_u256())));
            assert_eq!(value, expected_value.as_u256());
        }

        assert!(walker.next().transpose().unwrap().is_none());
    }
}

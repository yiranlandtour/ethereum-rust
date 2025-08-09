use ethereum_types::{H256, U256, Address};
use ethereum_core::{Block, Transaction, Account};
use ethereum_storage::Database;
use ethereum_evm::EVM;
use ethereum_trie::PatriciaTrie;
use std::sync::Arc;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

use crate::{Result, DebugError};

/// State diff between pre and post execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateDiff {
    pub accounts: HashMap<Address, AccountDiff>,
    pub storage: HashMap<Address, StorageDiff>,
    pub created_contracts: Vec<Address>,
    pub deleted_accounts: Vec<Address>,
}

/// Account state difference
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDiff {
    pub balance: BalanceDiff,
    pub nonce: NonceDiff,
    pub code: Option<CodeDiff>,
    pub storage_root: Option<H256>,
}

/// Balance difference
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceDiff {
    pub before: U256,
    pub after: U256,
    pub diff: i128,
}

/// Nonce difference
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NonceDiff {
    pub before: u64,
    pub after: u64,
}

/// Code difference
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeDiff {
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}

/// Storage difference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageDiff {
    pub changes: HashMap<H256, StorageChange>,
}

/// Single storage change
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageChange {
    pub before: H256,
    pub after: H256,
}

/// Compute state diff for a transaction
pub async fn compute_state_diff<D: Database + 'static>(
    tx: &Transaction,
    block: &Block,
    db: Arc<D>,
    evm: Arc<EVM<D>>,
) -> Result<StateDiff> {
    // Get pre-state
    let pre_state = get_state_before_tx(tx, block, db.clone()).await?;
    
    // Execute transaction
    let context = create_context(block);
    let mut state = PatriciaTrie::new_with_root(db.clone(), block.header.state_root);
    
    let mut touched_accounts = HashMap::new();
    let mut storage_changes = HashMap::new();
    let mut created_contracts = Vec::new();
    
    // Track state changes during execution
    let _result = evm.execute_transaction_with_tracer(
        tx,
        state.clone(),
        &context,
        |_pc, op, _stack, _memory, _storage| {
            // Track account touches and storage modifications
            match op {
                ethereum_evm::Opcode::SSTORE => {
                    // Track storage change
                }
                ethereum_evm::Opcode::CREATE | ethereum_evm::Opcode::CREATE2 => {
                    // Track contract creation
                }
                _ => {}
            }
        }
    ).await.map_err(|e| DebugError::EvmError(e.to_string()))?;
    
    // Get post-state
    let post_state = get_state_after_tx(&mut state, touched_accounts.keys()).await?;
    
    // Compute differences
    let mut account_diffs = HashMap::new();
    
    for (address, pre_account) in &pre_state.accounts {
        let post_account = post_state.accounts.get(address);
        
        if let Some(post) = post_account {
            // Account exists in both states
            if pre_account != post {
                account_diffs.insert(*address, compute_account_diff(pre_account, post));
            }
        } else {
            // Account was deleted
            // (This is rare in Ethereum after EIP-161)
        }
    }
    
    // Check for newly created accounts
    for (address, post_account) in &post_state.accounts {
        if !pre_state.accounts.contains_key(address) {
            // New account
            created_contracts.push(*address);
            
            account_diffs.insert(*address, AccountDiff {
                balance: BalanceDiff {
                    before: U256::zero(),
                    after: post_account.balance,
                    diff: post_account.balance.as_u128() as i128,
                },
                nonce: NonceDiff {
                    before: 0,
                    after: post_account.nonce,
                },
                code: if !post_account.code.is_empty() {
                    Some(CodeDiff {
                        before: Vec::new(),
                        after: post_account.code.clone(),
                    })
                } else {
                    None
                },
                storage_root: Some(post_account.storage_root),
            });
        }
    }
    
    Ok(StateDiff {
        accounts: account_diffs,
        storage: storage_changes,
        created_contracts,
        deleted_accounts: Vec::new(),
    })
}

/// State snapshot
struct StateSnapshot {
    accounts: HashMap<Address, Account>,
    storage: HashMap<Address, HashMap<H256, H256>>,
}

/// Get state before transaction
async fn get_state_before_tx<D: Database>(
    tx: &Transaction,
    block: &Block,
    db: Arc<D>,
) -> Result<StateSnapshot> {
    let state = PatriciaTrie::new_with_root(db, block.header.state_root);
    
    let mut accounts = HashMap::new();
    let mut storage = HashMap::new();
    
    // Get sender account
    let sender = recover_sender(tx)?;
    if let Some(account) = get_account(&state, sender).await? {
        accounts.insert(sender, account);
    }
    
    // Get recipient account
    if let Some(to) = tx.to {
        if let Some(account) = get_account(&state, to).await? {
            // Get storage if contract
            if !account.code.is_empty() {
                storage.insert(to, get_account_storage(&state, to, &account).await?);
            }
            accounts.insert(to, account);
        }
    }
    
    Ok(StateSnapshot { accounts, storage })
}

/// Get state after transaction
async fn get_state_after_tx<D: Database>(
    state: &mut PatriciaTrie<D>,
    touched_addresses: impl Iterator<Item = &Address>,
) -> Result<StateSnapshot> {
    let mut accounts = HashMap::new();
    let mut storage = HashMap::new();
    
    for address in touched_addresses {
        if let Some(account) = get_account(state, *address).await? {
            if !account.code.is_empty() {
                storage.insert(*address, get_account_storage(state, *address, &account).await?);
            }
            accounts.insert(*address, account);
        }
    }
    
    Ok(StateSnapshot { accounts, storage })
}

/// Get account from state
async fn get_account<D: Database>(
    state: &PatriciaTrie<D>,
    address: Address,
) -> Result<Option<Account>> {
    match state.get(address.as_bytes()).await {
        Ok(Some(data)) => {
            let account = bincode::deserialize(&data)
                .map_err(|e| DebugError::ExecutionError(e.to_string()))?;
            Ok(Some(account))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(DebugError::ExecutionError(e.to_string())),
    }
}

/// Get account storage
async fn get_account_storage<D: Database>(
    state: &PatriciaTrie<D>,
    _address: Address,
    account: &Account,
) -> Result<HashMap<H256, H256>> {
    let storage_trie = PatriciaTrie::new_with_root(
        state.db(),
        account.storage_root,
    );
    
    // Would iterate through storage slots
    // For now, return empty
    Ok(HashMap::new())
}

/// Compute account difference
fn compute_account_diff(before: &Account, after: &Account) -> AccountDiff {
    let balance_diff = if after.balance >= before.balance {
        (after.balance - before.balance).as_u128() as i128
    } else {
        -((before.balance - after.balance).as_u128() as i128)
    };
    
    AccountDiff {
        balance: BalanceDiff {
            before: before.balance,
            after: after.balance,
            diff: balance_diff,
        },
        nonce: NonceDiff {
            before: before.nonce,
            after: after.nonce,
        },
        code: if before.code != after.code {
            Some(CodeDiff {
                before: before.code.clone(),
                after: after.code.clone(),
            })
        } else {
            None
        },
        storage_root: if before.storage_root != after.storage_root {
            Some(after.storage_root)
        } else {
            None
        },
    }
}

/// Recover sender from transaction
fn recover_sender(_tx: &Transaction) -> Result<Address> {
    // Simplified - would use actual signature recovery
    Ok(Address::from([1u8; 20]))
}

/// Create EVM context
fn create_context(block: &Block) -> ethereum_evm::Context {
    ethereum_evm::Context {
        block_number: block.header.number,
        timestamp: block.header.timestamp,
        gas_limit: block.header.gas_limit,
        coinbase: block.header.author,
        difficulty: block.header.difficulty,
        chain_id: 1,
    }
}
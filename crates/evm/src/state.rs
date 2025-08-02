use crate::Account;
use ethereum_types::{Address, H256, U256};
use std::collections::HashMap;

pub trait StateDB {
    fn get_account(&self, address: &Address) -> Option<Account>;
    fn set_account(&mut self, address: Address, account: Account);
    fn get_storage(&self, address: &Address, key: &H256) -> H256;
    fn set_storage(&mut self, address: Address, key: H256, value: H256);
    fn exists(&self, address: &Address) -> bool;
    fn is_empty(&self, address: &Address) -> bool;
    fn remove_account(&mut self, address: &Address);
}

impl StateDB for HashMap<Address, Account> {
    fn get_account(&self, address: &Address) -> Option<Account> {
        self.get(address).cloned()
    }

    fn set_account(&mut self, address: Address, account: Account) {
        self.insert(address, account);
    }

    fn get_storage(&self, address: &Address, key: &H256) -> H256 {
        self.get(address)
            .and_then(|account| account.storage.get(key))
            .copied()
            .unwrap_or_default()
    }

    fn set_storage(&mut self, address: Address, key: H256, value: H256) {
        self.entry(address)
            .or_insert_with(Account::default)
            .storage
            .insert(key, value);
    }

    fn exists(&self, address: &Address) -> bool {
        self.contains_key(address)
    }

    fn is_empty(&self, address: &Address) -> bool {
        self.get(address)
            .map(|account| {
                account.balance.is_zero() 
                && account.nonce == 0 
                && account.code.is_empty()
            })
            .unwrap_or(true)
    }

    fn remove_account(&mut self, address: &Address) {
        self.remove(address);
    }
}

#[derive(Debug, Clone)]
pub struct AccountChange {
    pub address: Address,
    pub balance_change: Option<(U256, U256)>,
    pub nonce_change: Option<(u64, u64)>,
    pub code_change: Option<(Vec<u8>, Vec<u8>)>,
    pub storage_changes: HashMap<H256, (H256, H256)>,
}

#[derive(Debug, Clone)]
pub struct StateChanges {
    pub account_changes: HashMap<Address, AccountChange>,
    pub created_accounts: Vec<Address>,
    pub deleted_accounts: Vec<Address>,
}

impl StateChanges {
    pub fn new() -> Self {
        Self {
            account_changes: HashMap::new(),
            created_accounts: Vec::new(),
            deleted_accounts: Vec::new(),
        }
    }

    pub fn record_balance_change(&mut self, address: Address, old: U256, new: U256) {
        self.account_changes
            .entry(address)
            .or_insert_with(|| AccountChange {
                address,
                balance_change: None,
                nonce_change: None,
                code_change: None,
                storage_changes: HashMap::new(),
            })
            .balance_change = Some((old, new));
    }

    pub fn record_storage_change(&mut self, address: Address, key: H256, old: H256, new: H256) {
        self.account_changes
            .entry(address)
            .or_insert_with(|| AccountChange {
                address,
                balance_change: None,
                nonce_change: None,
                code_change: None,
                storage_changes: HashMap::new(),
            })
            .storage_changes
            .insert(key, (old, new));
    }
}

impl Default for StateChanges {
    fn default() -> Self {
        Self::new()
    }
}
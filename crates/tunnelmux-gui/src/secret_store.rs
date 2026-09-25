#[cfg(test)]
use std::cell::RefCell;
#[cfg(test)]
use std::collections::HashMap;

#[cfg(not(test))]
use anyhow::Context;
#[cfg(not(test))]
use keyring::{Entry, Error as KeyringError};

#[cfg(not(test))]
const SERVICE_NAME: &str = "com.tunnelmux.gui";

#[cfg(not(test))]
fn entry(user: &str) -> anyhow::Result<Entry> {
    Entry::new(SERVICE_NAME, user).context("failed to open the OS credential store entry")
}

pub fn control_token_user() -> &'static str {
    "control-token"
}

pub fn provider_token_user(profile_id: &str, provider: &str) -> String {
    format!("provider/{profile_id}/{provider}")
}

#[cfg(not(test))]
pub fn get(user: &str) -> anyhow::Result<Option<String>> {
    match entry(user)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(error).context("failed to read the OS credential store"),
    }
}

#[cfg(not(test))]
pub fn set(user: &str, value: &str) -> anyhow::Result<()> {
    entry(user)?
        .set_password(value)
        .context("failed to write the OS credential store")
}

#[cfg(not(test))]
pub fn delete(user: &str) -> anyhow::Result<()> {
    match entry(user)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(error).context("failed to delete the OS credential store entry"),
    }
}

#[cfg(test)]
thread_local! {
    static MEMORY_STORE: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

#[cfg(test)]
pub fn get(user: &str) -> anyhow::Result<Option<String>> {
    Ok(MEMORY_STORE.with(|store| store.borrow().get(user).cloned()))
}

#[cfg(test)]
pub fn set(user: &str, value: &str) -> anyhow::Result<()> {
    MEMORY_STORE.with(|store| {
        store
            .borrow_mut()
            .insert(user.to_string(), value.to_string());
    });
    Ok(())
}

#[cfg(test)]
pub fn delete(user: &str) -> anyhow::Result<()> {
    MEMORY_STORE.with(|store| {
        store.borrow_mut().remove(user);
    });
    Ok(())
}

#[cfg(test)]
pub fn clear_for_tests() {
    MEMORY_STORE.with(|store| store.borrow_mut().clear());
}

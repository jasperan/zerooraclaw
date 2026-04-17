pub mod config_store;
pub mod connection;
pub mod embedding;
pub mod memory;
pub mod prompt;
pub mod schema;
pub mod session;
pub mod state;
pub mod vector;

pub use connection::OracleConnectionManager;
pub use embedding::OracleEmbedding;
pub use memory::OracleMemory;
pub use prompt::OraclePromptStore;

use oracle::Connection;
use std::sync::{Mutex, MutexGuard};

/// Lock a shared Oracle `Connection`, converting a poisoned-mutex error into
/// an `anyhow::Error`.  All Oracle-layer sites follow this exact pattern, so
/// centralising keeps the error message uniform and the call-sites terse.
pub(crate) fn lock_conn(conn: &Mutex<Connection>) -> anyhow::Result<MutexGuard<'_, Connection>> {
    conn.lock()
        .map_err(|e| anyhow::anyhow!("Oracle connection mutex poisoned: {e}"))
}

//! Oracle AI Database connection manager.
//!
//! Supports two connection modes:
//! - **FreePDB**: Standard host:port/service connection (Oracle Database Free container)
//! - **ADB**: Autonomous Database with DSN (wallet-less TLS or mTLS with wallet)

use crate::config::OracleConfig;
use oracle::{Connection, Connector};
use std::sync::{Arc, Mutex};
use tracing::info;

/// Manages Oracle database connections with FreePDB and ADB support.
pub struct OracleConnectionManager {
    config: OracleConfig,
    conn: Arc<Mutex<Connection>>,
}

impl OracleConnectionManager {
    /// Validate that the ONNX model name is a safe Oracle identifier.
    ///
    /// Oracle's `VECTOR_EMBEDDING()` function requires a literal identifier for
    /// the model name — it cannot be bound as a parameter. We interpolate it via
    /// `format!()`, so we must reject anything that is not `[A-Za-z0-9_.]`.
    fn validate_onnx_model_name(name: &str) -> anyhow::Result<()> {
        if name.is_empty() {
            anyhow::bail!("ONNX model name cannot be empty");
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        {
            anyhow::bail!(
                "ONNX model name contains invalid characters (only [A-Za-z0-9_.] allowed): {name}"
            );
        }
        Ok(())
    }

    /// Create a new connection manager and establish connection.
    pub fn new(config: &OracleConfig) -> anyhow::Result<Self> {
        Self::validate_onnx_model_name(&config.onnx_model)?;

        let conn = match config.mode.as_str() {
            "adb" => {
                info!("Connecting to Oracle Autonomous Database...");
                Self::connect_adb(config)?
            }
            _ => {
                info!(
                    "Connecting to Oracle FreePDB at {}:{}/{}...",
                    config.host, config.port, config.service
                );
                Self::connect_freepdb(config)?
            }
        };

        info!("Oracle connection established");
        Ok(Self {
            config: config.clone(),
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn connect_freepdb(config: &OracleConfig) -> anyhow::Result<Connection> {
        let connect_string = format!("//{}:{}/{}", config.host, config.port, config.service);
        let conn = Connector::new(&config.user, &config.password, &connect_string).connect()?;
        Ok(conn)
    }

    fn connect_adb(config: &OracleConfig) -> anyhow::Result<Connection> {
        let dsn = config
            .dsn
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("ADB mode requires 'dsn' in [oracle] config"))?;
        let conn = Connector::new(&config.user, &config.password, dsn).connect()?;
        Ok(conn)
    }

    /// Get a shared reference to the connection.
    pub fn conn(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }

    /// Get the agent ID from config.
    pub fn agent_id(&self) -> &str {
        &self.config.agent_id
    }

    /// Get the ONNX model name from config.
    pub fn onnx_model(&self) -> &str {
        &self.config.onnx_model
    }

    /// Get a reference to the config.
    pub fn config(&self) -> &OracleConfig {
        &self.config
    }

    /// Check if the connection is alive.
    pub fn ping(&self) -> bool {
        self.conn
            .lock()
            .map_or(false, |conn| conn.ping().is_ok())
    }
}

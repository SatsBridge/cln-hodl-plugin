use anyhow::{anyhow, Context, Result};
use cln_grpc::pb::node_server::NodeServer;
use cln_plugin::{options, Builder};
use cln_rpc::model::ListinvoicesInvoices;
use log::{debug, info, warn};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod config;
mod hooks;
mod state;
mod tasks;
mod tls;
mod util;

#[derive(Clone, Debug, Copy)]
pub struct HodlUpdate {
    pub state: state::HodlState,
    pub generation: u64,
}
#[derive(Clone, Debug)]
pub struct PluginState {
    pub config: Arc<Mutex<config::Config>>,
    pub blockheight: Arc<Mutex<u32>>,
    pub invoice_amts: Arc<Mutex<BTreeMap<String, u64>>>,
    pub states: Arc<tokio::sync::Mutex<BTreeMap<String, HodlUpdate>>>,
    pub invoices: Arc<Mutex<BTreeMap<String, ListinvoicesInvoices>>>,
    rpc_path: PathBuf,
    identity: tls::Identity,
    ca_cert: Vec<u8>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    debug!("Starting grpc plugin");
    std::env::set_var("CLN_PLUGIN_LOG", "debug");
    let path = Path::new("lightning-rpc");

    let directory = std::env::current_dir()?;
    let (identity, ca_cert) = tls::init(&directory)?;

    let state = PluginState {
        config: Arc::new(Mutex::new(config::Config::new())),
        blockheight: Arc::new(Mutex::new(u32::default())),
        invoice_amts: Arc::new(Mutex::new(BTreeMap::new())),
        states: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
        invoices: Arc::new(Mutex::new(BTreeMap::new())),
        rpc_path: path.into(),
        identity,
        ca_cert,
    };

    let plugin = match Builder::new(tokio::io::stdin(), tokio::io::stdout())
        .option(options::ConfigOption::new(
            "grpc-hodl-port",
            options::Value::Integer(-1),
            "Which port should the grpc plugin listen for incoming connections?",
        ))
        .hook("htlc_accepted", hooks::htlc_handler)
        .subscribe("block_added", hooks::block_added)
        .configure()
        .await?
    {
        Some(p) => {
            info!("read config");
            match config::read_config(&p, state.clone()).await {
                Ok(()) => &(),
                Err(e) => return p.disable(format!("{}", e).as_str()).await,
            };
            p
        }
        None => return Ok(()),
    };

    match plugin.start(state.clone()).await {
        Ok(p) => {
            info!("starting lookup_state task");
            let lookup_state = p.clone();
            tokio::spawn(async move {
                match tasks::lookup_state(lookup_state.clone()).await {
                    Ok(()) => (),
                    Err(e) => warn!("Error in lookup_state thread: {}", e.to_string()),
                };
            });
            let cleanup_state = p.clone();
            tokio::spawn(async move {
                match tasks::clean_up(cleanup_state.clone()).await {
                    Ok(()) => (),
                    Err(e) => warn!("Error in clean_up thread: {}", e.to_string()),
                };
            });
            let plugin_state = p.clone();
            tokio::select! {
                _ = plugin_state.join() => {
                // This will likely never be shown, if we got here our
                // parent process is exiting and not processing out log
                // messages anymore.
                    debug!("Plugin loop terminated")
                }
            }
            Ok(())
        }
        Err(e) => return Err(anyhow!("Error starting plugin: {}", e)),
    }
}

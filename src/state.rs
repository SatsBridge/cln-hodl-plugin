// Huge json!() macros require lots of recursion
#![recursion_limit = "1024"]

use std::{fmt, path::PathBuf};

use anyhow::{anyhow, Error};
use cln_grpc::pb;
use cln_rpc::{
    model::{
        DatastoreMode, DatastoreRequest, DatastoreResponse, DeldatastoreRequest,
        DeldatastoreResponse, ListdatastoreDatastore, ListdatastoreRequest, ListdatastoreResponse,
    },
    ClnRpc, Request, Response,
};
use log::debug;
use crate::plugin;


pub const HODLVOICE_PLUGIN_NAME: &str = "hodlvoice";
const HODLVOICE_DATASTORE_STATE: &str = "state";
const HODLVOICE_DATASTORE_HTLC_EXPIRY: &str = "expiry";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HodlState {
    Open,
    Settled,
    Canceled,
    Accepted,
}
impl HodlState {
    pub fn to_string(&self) -> String {
        match self {
            HodlState::Open => "open".to_string(),
            HodlState::Settled => "settled".to_string(),
            HodlState::Canceled => "canceled".to_string(),
            HodlState::Accepted => "accepted".to_string(),
        }
    }
    pub fn from_str(s: &str) -> Result<HodlState, Error> {
        match s.to_lowercase().as_str() {
            "open" => Ok(HodlState::Open),
            "settled" => Ok(HodlState::Settled),
            "canceled" => Ok(HodlState::Canceled),
            "accepted" => Ok(HodlState::Accepted),
            _ => Err(anyhow!("could not parse HodlState from string")),
        }
    }
    pub fn as_i32(&self) -> i32 {
        match self {
            HodlState::Open => 0,
            HodlState::Settled => 1,
            HodlState::Canceled => 2,
            HodlState::Accepted => 3,
        }
    }
    pub fn is_valid_transition(&self, newstate: &HodlState) -> bool {
        match self {
            HodlState::Open => match newstate {
                HodlState::Settled => false,
                _ => true,
            },
            HodlState::Settled => match newstate {
                HodlState::Settled => true,
                _ => false,
            },
            HodlState::Canceled => match newstate {
                HodlState::Canceled => true,
                _ => false,
            },
            HodlState::Accepted => true,
        }
    }
    // pub async fn accepting(&self) -> Result<(), Error> {
    //     match *self {
    //         HodlState::Open => Ok(()),
    //         _ => Err(anyhow!(
    //             "illegal state transition for accepting: {}",
    //             *self.to_string()
    //         )),
    //     }
    // }

    // pub async fn unaccepting(&self) -> Result<(), Error> {
    //     match *self {
    //         HodlState::Accepted => Ok(()),
    //         _ => Err(anyhow!(
    //             "illegal state transition for unaccepting: {}",
    //             *self.to_string()
    //         )),
    //     }
    // }

    // pub async fn canceling(&self) -> Result<(), Error> {
    //     match *self {
    //         HodlState::Open | HodlState::Accepted => Ok(()),
    //         _ => Err(anyhow!(
    //             "illegal state transition for canceling: {}",
    //             *self.to_string()
    //         )),
    //     }
    // }

    // pub async fn settling(&self) -> Result<(), Error> {
    //     match *self {
    //         HodlState::Accepted => Ok(()),
    //         _ => Err(anyhow!(
    //             "illegal state transition for settling: {}",
    //             *self.to_string()
    //         )),
    //     }
    // }
}
impl fmt::Display for HodlState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HodlState::Open => write!(f, "open"),
            HodlState::Settled => write!(f, "settled"),
            HodlState::Canceled => write!(f, "canceled"),
            HodlState::Accepted => write!(f, "accepted"),
        }
    }
}

async fn datastore_raw(
    rpc_path: &PathBuf,
    key: Vec<String>,
    string: Option<String>,
    hex: Option<String>,
    mode: Option<DatastoreMode>,
    generation: Option<u64>,
) -> Result<DatastoreResponse, Error> {
    let mut rpc = ClnRpc::new(&rpc_path).await?;
    let datastore_request = rpc
        .call(Request::Datastore(DatastoreRequest {
            key: key.clone(),
            string: string.clone(),
            hex,
            mode,
            generation,
        }))
        .await
        .map_err(|e| anyhow!("Error calling datastore: {:?}", e))?;
    debug!("datastore_raw: set {:?} to {}", key, string.unwrap());
    match datastore_request {
        Response::Datastore(info) => Ok(info),
        e => Err(anyhow!("Unexpected result in datastore: {:?}", e)),
    }
}

pub(crate) async fn datastore_new_state(
    rpc_path: &PathBuf,
    pay_hash: String,
    string: String,
) -> Result<DatastoreResponse, Error> {
    datastore_raw(
        rpc_path,
        vec![
            HODLVOICE_PLUGIN_NAME.to_string(),
            pay_hash,
            HODLVOICE_DATASTORE_STATE.to_string(),
        ],
        Some(string),
        None,
        Some(DatastoreMode::MUST_CREATE),
        None,
    )
        .await
}

pub async fn datastore_update_state(
    rpc_path: &PathBuf,
    pay_hash: String,
    string: String,
    generation: u64,
) -> Result<DatastoreResponse, Error> {
    datastore_raw(
        rpc_path,
        vec![
            HODLVOICE_PLUGIN_NAME.to_string(),
            pay_hash,
            HODLVOICE_DATASTORE_STATE.to_string(),
        ],
        Some(string),
        None,
        Some(DatastoreMode::MUST_REPLACE),
        Some(generation),
    )
        .await
}

pub(crate) async fn datastore_update_state_forced(
    rpc_path: &PathBuf,
    pay_hash: String,
    string: String,
) -> Result<DatastoreResponse, Error> {
    datastore_raw(
        rpc_path,
        vec![
            HODLVOICE_PLUGIN_NAME.to_string(),
            pay_hash,
            HODLVOICE_DATASTORE_STATE.to_string(),
        ],
        Some(string),
        None,
        Some(DatastoreMode::MUST_REPLACE),
        None,
    )
        .await
}

pub async fn datastore_htlc_expiry(
    rpc_path: &PathBuf,
    pay_hash: String,
    string: String,
) -> Result<DatastoreResponse, Error> {
    datastore_raw(
        rpc_path,
        vec![
            HODLVOICE_PLUGIN_NAME.to_string(),
            pay_hash,
            HODLVOICE_DATASTORE_HTLC_EXPIRY.to_string(),
        ],
        Some(string),
        None,
        Some(DatastoreMode::CREATE_OR_REPLACE),
        None,
    )
        .await
}

// pub async fn datastore_update_htlc_expiry(
//     rpc_path: &PathBuf,
//     pay_hash: String,
//     string: String,
// ) -> Result<DatastoreResponse, Error> {
//     datastore_raw(
//         rpc_path,
//         vec![
//             HODLVOICE_PLUGIN_NAME.to_string(),
//             pay_hash,
//             HODLVOICE_DATASTORE_HTLC_EXPIRY.to_string(),
//         ],
//         Some(string),
//         None,
//         Some(DatastoreMode::MUST_REPLACE),
//         None,
//     )
//     .await
// }

pub async fn list_datastore_raw(
    rpc_path: &PathBuf,
    key: Option<Vec<String>>,
) -> Result<ListdatastoreResponse, Error> {
    let mut rpc = ClnRpc::new(&rpc_path).await?;
    let datastore_request = rpc
        .call(Request::ListDatastore(ListdatastoreRequest { key }))
        .await
        .map_err(|e| anyhow!("Error calling listdatastore: {:?}", e))?;
    match datastore_request {
        Response::ListDatastore(info) => Ok(info),
        e => Err(anyhow!("Unexpected result in listdatastore: {:?}", e)),
    }
}

pub async fn list_datastore_state(
    rpc_path: &PathBuf,
    pay_hash: String,
) -> Result<ListdatastoreDatastore, Error> {
    let response = list_datastore_raw(
        rpc_path,
        Some(vec![
            HODLVOICE_PLUGIN_NAME.to_string(),
            pay_hash.clone(),
            HODLVOICE_DATASTORE_STATE.to_string(),
        ]),
    )
        .await?;
    let data = response.datastore.first().ok_or_else(|| {
        anyhow!(
            "empty result for list_datastore_state with pay_hash: {}",
            pay_hash
        )
    })?;
    Ok(data.clone())
}

pub async fn list_datastore_htlc_expiry(rpc_path: &PathBuf, pay_hash: String) -> Result<u32, Error> {
    let response = list_datastore_raw(
        rpc_path,
        Some(vec![
            HODLVOICE_PLUGIN_NAME.to_string(),
            pay_hash.clone(),
            HODLVOICE_DATASTORE_HTLC_EXPIRY.to_string(),
        ]),
    )
        .await?;
    let data = response
        .datastore
        .first()
        .ok_or_else(|| {
            anyhow!(
                "empty result for list_datastore_htlc_expiry with pay_hash: {}",
                pay_hash
            )
        })?
        .string
        .as_ref()
        .ok_or_else(|| {
            anyhow!(
                "None string for list_datastore_htlc_expiry with pay_hash: {}",
                pay_hash
            )
        })?;
    let cltv = data.parse::<u32>()?;
    Ok(cltv)
}

async fn del_datastore_raw(
    rpc_path: &PathBuf,
    key: Vec<String>,
) -> Result<DeldatastoreResponse, Error> {
    let mut rpc = ClnRpc::new(&rpc_path).await?;
    let del_datastore_request = rpc
        .call(Request::DelDatastore(DeldatastoreRequest {
            key,
            generation: None,
        }))
        .await
        .map_err(|e| anyhow!("Error calling DelDatastore: {:?}", e))?;
    match del_datastore_request {
        Response::DelDatastore(info) => Ok(info),
        e => Err(anyhow!("Unexpected result in DelDatastore: {:?}", e)),
    }
}

pub async fn del_datastore_state(
    rpc_path: &PathBuf,
    pay_hash: String,
) -> Result<DeldatastoreResponse, Error> {
    del_datastore_raw(
        rpc_path,
        vec![
            HODLVOICE_PLUGIN_NAME.to_string(),
            pay_hash,
            HODLVOICE_DATASTORE_STATE.to_string(),
        ],
    )
        .await
}

pub async fn del_datastore_htlc_expiry(
    rpc_path: &PathBuf,
    pay_hash: String,
) -> Result<DeldatastoreResponse, Error> {
    del_datastore_raw(
        rpc_path,
        vec![
            HODLVOICE_PLUGIN_NAME.to_string(),
            pay_hash.clone(),
            HODLVOICE_DATASTORE_HTLC_EXPIRY.to_string(),
        ],
    )
        .await
}

pub(crate) fn short_channel_id_to_string(scid: u64) -> String {
    let block_height = scid >> 40;
    let tx_index = (scid >> 16) & 0xFFFFFF;
    let output_index = scid & 0xFFFF;
    format!("{}x{}x{}", block_height, tx_index, output_index)
}

pub async fn listdatastore_htlc_expiry(rpc_path: &PathBuf, pay_hash: String) -> Result<u32, Error> {
    let response = listdatastore_raw(
        rpc_path,
        Some(vec![
            HODLVOICE_PLUGIN_NAME.to_string(),
            pay_hash.clone(),
            HODLVOICE_DATASTORE_HTLC_EXPIRY.to_string(),
        ]),
    )
        .await?;
    let data = response
        .datastore
        .first()
        .ok_or_else(|| {
            anyhow!(
                "empty result for listdatastore_htlc_expiry with pay_hash: {}",
                pay_hash
            )
        })?
        .string
        .as_ref()
        .ok_or_else(|| {
            anyhow!(
                "None string for listdatastore_htlc_expiry with pay_hash: {}",
                pay_hash
            )
        })?;
    let cltv = data.parse::<u32>()?;
    Ok(cltv)
}

pub async fn listdatastore_state(
    rpc_path: &PathBuf,
    pay_hash: String,
) -> Result<ListdatastoreDatastore, Error> {
    let response = listdatastore_raw(
        rpc_path,
        Some(vec![
            HODLVOICE_PLUGIN_NAME.to_string(),
            pay_hash.clone(),
            HODLVOICE_DATASTORE_STATE.to_string(),
        ]),
    )
        .await?;
    let data = response.datastore.first().ok_or_else(|| {
        anyhow!(
            "empty result for listdatastore_state with pay_hash: {}",
            pay_hash
        )
    })?;
    Ok(data.clone())
}

pub async fn listdatastore_raw(
    rpc_path: &PathBuf,
    key: Option<Vec<String>>,
) -> Result<ListdatastoreResponse, Error> {
    let mut rpc = ClnRpc::new(&rpc_path).await?;
    let datastore_request = rpc
        .call(Request::ListDatastore(ListdatastoreRequest { key }))
        .await
        .map_err(|e| anyhow!("Error calling listdatastore: {:?}", e))?;
    match datastore_request {
        Response::ListDatastore(info) => Ok(info),
        e => Err(anyhow!("Unexpected result in listdatastore: {:?}", e)),
    }
}

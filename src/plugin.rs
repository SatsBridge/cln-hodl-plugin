tonic::include_proto!("cln");
use plugin::plugin_server::{Plugin, PluginServer};

use crate::state::{
    datastore_new_state, datastore_update_state_forced, listdatastore_htlc_expiry,
    listdatastore_state, short_channel_id_to_string, HodlState,
};

use anyhow::Result;
use cln_rpc::{ClnRpc, model, Request, Response};
use lightning_invoice::{Invoice, InvoiceDescription, SignedRawInvoice};
use log::{debug, trace};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use cln_grpc::pb;
use cln_rpc::model::{InvoiceResponse, requests};
use cln_rpc::primitives::{Amount, AmountOrAny};
use tonic::{Code, Status};


#[derive(Clone)]
pub struct Server {
    rpc_path: PathBuf,
}

impl Server {
    pub async fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            rpc_path: path.to_path_buf(),
        })
    }
}

#[tonic::async_trait]
impl Plugin for Server {
    async fn ping(
        &self,
        request: tonic::Request<PluginPingRequest>, // Accept request of type HelloRequest
    ) -> Result<tonic::Response<PluginPongReply>, Status> { // Return an instance of type HelloReply
        debug!("Got a request: {:?}", request);

        let reply = PluginPongReply {
            message: format!("Hello {}!", request.into_inner().message).into(), // We must use .into_inner() as the fields of gRPC requests and responses are private
        };

        Ok(tonic::Response::new(reply)) // Send back our formatted greeting
    }

    async fn hodl_invoice(
        &self,
        request: tonic::Request<HodlInvoiceRequest>,
    ) -> Result<tonic::Response<HodlInvoiceResponse>, tonic::Status> {
        debug!("Client asked for hodlinvoice");
        let r = request.into_inner();
        let req = requests::InvoiceRequest{
            amount_msat: AmountOrAny::Any,
            description: "".to_string(),
            label: "".to_string(),
            expiry: None,
            fallbacks: None,
            preimage: None,
            exposeprivatechannels: None,
            cltv: None,
            deschashonly: None
        };
        trace!("hodlinvoice request: {:?}", req);
        let mut rpc = ClnRpc::new(&self.rpc_path)
            .await
            .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        let result = rpc.call(Request::Invoice(req)).await.map_err(|e| {
            Status::new(
                Code::Unknown,
                format!("Error calling method HodlInvoice: {:?}", e),
            )
        })?;
        match result {
            Response::Invoice(r) => {
                trace!("hodlinvoice response: {:?}", r);
                match datastore_new_state(
                    &self.rpc_path,
                    r.payment_hash.to_string(),
                    HodlState::Open.to_string(),
                )
                .await
                {
                    Ok(_o) => (),
                    Err(e) => {
                        return Err(Status::new(
                            Code::Internal,
                            format!(
                                "Unexpected result {:?} to method call datastore_new_state",
                                e
                            ),
                        ))
                    }
                }
                Ok(tonic::Response::new(HodlInvoiceResponse{
                    bolt11: "".to_string(),
                    payment_hash: vec![],
                    payment_secret: vec![]  ,
                    expires_at: 0,
                    warning_capacity: None,
                    warning_offline: None,
                    warning_deadends: None,
                    warning_private_unused: None,
                    warning_mpp: None
                }))
            }
            r => Err(Status::new(
                Code::Internal,
                format!("Unexpected result {:?} to method call HodlInvoice", r),
            )),
        }
    }

    async fn hodl_invoice_settle(
        &self,
        request: tonic::Request<HodlInvoiceSettleRequest>,
    ) -> Result<tonic::Response<HodlInvoiceSettleResponse>, tonic::Status> {
        let req = request.into_inner();
        let req: HodlInvoiceSettleRequest = req.into();
        debug!("Client asked for hodlinvoicesettle");
        trace!("hodlinvoicesettle request: {:?}", req);
        let pay_hash = hex::encode(req.payment_hash.clone());
        let data = match listdatastore_state(&self.rpc_path, pay_hash.clone()).await {
            Ok(st) => st,
            Err(e) => {
                return Err(Status::new(
                    Code::Internal,
                    format!(
                        "Unexpected result {:?} to method call listdatastore_state",
                        e
                    ),
                ))
            }
        };
        let HodlState = match HodlState::from_str(&data.string.unwrap()) {
            Ok(hs) => hs,
            Err(e) => {
                return Err(Status::new(
                    Code::Internal,
                    format!(
                        "Unexpected result {:?} to method call HodlState::from_str",
                        e
                    ),
                ))
            }
        };
        match HodlState {
            HodlState::Accepted => {
                let result = datastore_update_state_forced(
                    &self.rpc_path,
                    pay_hash,
                    HodlState::Settled.to_string(),
                )
                .await;
                match result {
                    Ok(_r) => Ok(tonic::Response::new(HodlInvoiceSettleResponse {
                        state: HodlState::Settled.as_i32(),
                    })),
                    Err(e) => Err(Status::new(
                        Code::Internal,
                        format!(
                            "Unexpected result {:?} to method call datastore_update_state",
                            e
                        ),
                    )),
                }
            }
            _ => {
                return Err(Status::new(
                    Code::Internal,
                    format!(
                        "Hodl-Invoice is in wrong state: `{}`. Payment_hash: {}",
                        HodlState.to_string(),
                        pay_hash
                    ),
                ))
            }
        }
    }

    async fn hodl_invoice_cancel(
        &self,
        request: tonic::Request<HodlInvoiceCancelRequest>,
    ) -> Result<tonic::Response<HodlInvoiceCancelResponse>, tonic::Status> {
        let req = request.into_inner();
        let req: HodlInvoiceCancelRequest = req.into();
        debug!("Client asked for hodlinvoiceCancel");
        trace!("hodlinvoiceCancel request: {:?}", req);
        let pay_hash = hex::encode(req.payment_hash.clone());
        let data = match listdatastore_state(&self.rpc_path, pay_hash.clone()).await {
            Ok(st) => st,
            Err(e) => {
                return Err(Status::new(
                    Code::Internal,
                    format!(
                        "Unexpected result {:?} to method call listdatastore_state",
                        e
                    ),
                ))
            }
        };
        let HodlState = match HodlState::from_str(&data.string.unwrap()) {
            Ok(hs) => hs,
            Err(e) => {
                return Err(Status::new(
                    Code::Internal,
                    format!(
                        "Unexpected result {:?} to method call HodlState::from_str",
                        e
                    ),
                ))
            }
        };
        match HodlState {
            HodlState::Open | HodlState::Accepted => {
                let result = datastore_update_state_forced(
                    &self.rpc_path,
                    pay_hash,
                    HodlState::Canceled.to_string(),
                )
                .await;
                match result {
                    Ok(_r) => Ok(tonic::Response::new(HodlInvoiceCancelResponse {
                        state: HodlState::Canceled.as_i32(),
                    })),
                    Err(e) => Err(Status::new(
                        Code::Internal,
                        format!(
                            "Unexpected result {:?} to method call datastore_update_state",
                            e
                        ),
                    )),
                }
            }
            HodlState::Canceled | HodlState::Settled => {
                return Err(Status::new(
                    Code::Internal,
                    format!(
                        "Hodl-Invoice is in wrong state: `{}`. Payment_hash: {}",
                        HodlState.to_string(),
                        pay_hash
                    ),
                ))
            }
        }
    }

    async fn hodl_invoice_lookup(
        &self,
        request: tonic::Request<HodlInvoiceLookupRequest>,
    ) -> Result<tonic::Response<HodlInvoiceLookupResponse>, tonic::Status> {
        let req = request.into_inner();
        let req: HodlInvoiceLookupRequest = req.into();
        debug!("Client asked for hodlinvoiceLookup");
        trace!("hodlinvoiceLookup request: {:?}", req);
        let pay_hash = hex::encode(req.payment_hash.clone());
        let data = match listdatastore_state(&self.rpc_path, pay_hash.clone()).await {
            Ok(st) => st,
            Err(e) => {
                return Err(Status::new(
                    Code::Internal,
                    format!(
                        "Unexpected result {:?} to method call listdatastore_state",
                        e
                    ),
                ))
            }
        };
        let HodlState = match HodlState::from_str(&data.string.unwrap()) {
            Ok(hs) => hs,
            Err(e) => {
                return Err(Status::new(
                    Code::Internal,
                    format!(
                        "Unexpected result {:?} to method call HodlState::from_str",
                        e
                    ),
                ))
            }
        };
        let htlc_expiry = match HodlState {
            HodlState::Accepted => {
                match listdatastore_htlc_expiry(&self.rpc_path, pay_hash.clone()).await {
                    Ok(cltv) => Some(cltv),
                    Err(e) => {
                        return Err(Status::new(
                            Code::Internal,
                            format!(
                                "Unexpected result {:?} to method call listdatastore_htlc_expiry",
                                e
                            ),
                        ))
                    }
                }
            }
            _ => None,
        };
        Ok(tonic::Response::new(HodlInvoiceLookupResponse {
            state: HodlState.as_i32(),
            htlc_expiry,
        }))
    }
}

use hana::{
    hana_tracing::{self, Component},
    binutil::HanaDataDir,
    kv::{mdbx::*, MdbxWithDirHandle},
    rpc::{
        erigon::ErigonApiServerImpl, eth::EthApiServerImpl, net::NetApiServerImpl,
        otterscan::OtterscanApiServerImpl, trace::TraceApiServerImpl,
    },
};
use clap::Parser;
use ethereum_jsonrpc::{
    ErigonApiServer, EthApiServer, NetApiServer, OtterscanApiServer, TraceApiServer,
};
use jsonrpsee::{core::server::rpc_module::Methods, http_server::HttpServerBuilder};
use std::{future::pending, net::SocketAddr, sync::Arc};
use tracing_subscriber::prelude::*;

#[derive(Parser)]
#[clap(name = "Hana RPC", about = "RPC server for Hana")]
pub struct Opt {
    #[clap(long)]
    pub datadir: HanaDataDir,

    #[clap(long)]
    pub listen_address: SocketAddr,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    hana_tracing::build_subscriber(Component::RPCDaemon).init();

    let db: Arc<MdbxWithDirHandle<NoWriteMap>> = Arc::new(
        MdbxEnvironment::<NoWriteMap>::open_ro(
            mdbx::Environment::new(),
            &opt.datadir,
            hana::kv::tables::CHAINDATA_TABLES.clone(),
        )?
        .into(),
    );

    let server = HttpServerBuilder::default()
        .build(opt.listen_address)
        .await?;

    let mut api = Methods::new();
    api.merge(
        EthApiServerImpl {
            db: db.clone(),
            call_gas_limit: 100_000_000,
        }
        .into_rpc(),
    )
    .unwrap();
    api.merge(NetApiServerImpl.into_rpc()).unwrap();
    api.merge(ErigonApiServerImpl { db: db.clone() }.into_rpc())
        .unwrap();
    api.merge(OtterscanApiServerImpl { db: db.clone() }.into_rpc())
        .unwrap();
    api.merge(
        TraceApiServerImpl {
            db,
            call_gas_limit: 100_000_000,
        }
        .into_rpc(),
    )
    .unwrap();

    let _server_handle = server.start(api)?;

    pending().await
}

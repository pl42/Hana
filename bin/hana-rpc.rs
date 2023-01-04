use hana::{binutil::HanaDataDir, rpc::eth::EthApiServerImpl};
use clap::Parser;
use ethereum_jsonrpc::EthApiServer;
use jsonrpsee::http_server::HttpServerBuilder;
use std::{future::pending, net::SocketAddr, sync::Arc};
use tracing_subscriber::{prelude::*, EnvFilter};

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

    let env_filter = if std::env::var(EnvFilter::DEFAULT_ENV)
        .unwrap_or_default()
        .is_empty()
    {
        EnvFilter::new("hana=info,rpc=info")
    } else {
        EnvFilter::from_default_env()
    };
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(env_filter)
        .init();

    let db = Arc::new(
        hana::kv::mdbx::MdbxEnvironment::<mdbx::NoWriteMap>::open_ro(
            mdbx::Environment::new(),
            &opt.datadir,
            hana::kv::tables::CHAINDATA_TABLES.clone(),
        )?
        .into(),
    );

    let server = HttpServerBuilder::default()
        .build(opt.listen_address)
        .await?;
    let _server_handle = server.start(
        EthApiServerImpl {
            db,
            call_gas_limit: 100_000_000,
        }
        .into_rpc(),
    )?;

    pending().await
}

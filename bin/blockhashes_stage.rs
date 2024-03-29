use hana::stagedsync;
use structopt::StructOpt;
use tracing_subscriber::{prelude::*, EnvFilter};

#[derive(StructOpt)]
#[structopt(name = "Hana", about = "Ethereum client based on Thorax architecture")]
pub struct Opt {
    #[structopt(long, env)]
    pub tokio_console: bool,
}

#[allow(unreachable_code)]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    let filter = if std::env::var(EnvFilter::DEFAULT_ENV)
        .unwrap_or_default()
        .is_empty()
    {
        EnvFilter::new("hana=info")
    } else {
        EnvFilter::from_default_env()
    };
    let registry = tracing_subscriber::registry()
        // the `TasksLayer` can be used in combination with other `tracing` layers...
        .with(tracing_subscriber::fmt::layer().with_target(false));

    if opt.tokio_console {
        let (layer, server) = console_subscriber::TasksLayer::new();
        registry
            .with(filter.add_directive("tokio=trace".parse()?))
            .with(layer)
            .init();
        tokio::spawn(async move { server.serve().await.expect("server failed") });
    } else {
        registry.with(filter).init();
    }

    let db = hana::new_mem_database()?;

    let mut staged_sync = stagedsync::StagedSync::new();
    staged_sync.push(hana::stages::BlockHashes);
    staged_sync.run(&db).await?;

    Ok(())
}

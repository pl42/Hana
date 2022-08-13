use hana::stagedsync;
use std::time::Duration;
use structopt::StructOpt;
use tokio::time::sleep;

#[derive(StructOpt)]
#[structopt(name = "Hana", about = "Ethereum client based on Thorax architecture")]
pub struct Opt {}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    let _ = opt;
    let db = hana::new_mem_database()?;

    let mut staged_sync = stagedsync::StagedSync::new(|| async move {
        sleep(Duration::from_millis(6000)).await;
    });
    staged_sync.push(hana::stages::HeaderDownload);
    // staged_sync.push(hana::stages::BlockHashes);
    // staged_sync.push(hana::stages::ExecutionStage);

    // stagedsync::StagedSync::new(vec![], vec![]);
    staged_sync.run(&db).await?;
}

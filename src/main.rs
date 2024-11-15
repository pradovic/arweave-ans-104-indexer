use reqwest::Client;
use tokio::sync::mpsc;

use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use clap::Parser as ClapParser;

use tokio::io::{AsyncWriteExt, BufReader};

#[derive(ClapParser)]
#[command(author, version, about, long_about = None)]
struct Args {
    tx_id: String,

    #[arg(short, long, default_value = "bundle")]
    output: std::path::PathBuf,
}

#[tokio::main]
async fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");

    let args = Args::parse();
    let file = match tokio::fs::File::create(&args.output).await {
        Ok(file) => file,
        Err(e) => {
            tracing::error!("Failed to open output file: {}", e);
            return;
        }
    };

    let (tx, rx) = mpsc::channel(128);

    let write_handle = tokio::spawn(write_task(rx, file));

    tracing::info!("Starting processing for transaction ID: {}", args.tx_id);

    let client = Client::new();
    let response = match client
        .get(format!("https://arweave.net/{}", args.tx_id))
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            tracing::error!("Failed to fetch transaction: {}", e);
            return;
        }
    };

    let response_bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("Failed to read response: {}", e);
            return;
        }
    };

    let cursor = std::io::Cursor::new(response_bytes);
    let mut buffered = BufReader::with_capacity(65536, cursor);

    match arweave_ans_1040_indexer::process_bundle(&mut buffered, tx, &args.tx_id).await {
        Ok(_) => tracing::info!("Processing complete"),
        Err(e) => {
            tracing::error!("Processing failed: {}", e);
            return;
        }
    }

    match write_handle.await {
        Ok(_) => tracing::info!("Write task complete"),
        Err(e) => tracing::error!("Write task failed: {}", e),
    }
}

// the output format is not optimized for performance
// the goal was simplicity and readability
// the performance can be vastly improved if the intended use case is for machine to machine communication
// in this case we would choose one of the more efficient binary serialization formats (such as Protocol Buffers, Apache Avro, or MessagePack, BSON, ...)
async fn write_task(
    mut rx: mpsc::Receiver<arweave_ans_1040_indexer::DataItem>,
    mut file: tokio::fs::File,
) -> std::io::Result<()> {
    while let Some(item) = rx.recv().await {
        file.write_all(serde_json::to_string_pretty(&item)?.as_bytes())
            .await?;
        file.write_all(b"\n").await?;
    }
    Ok(())
}

use anyhow::Result;
use iroh::{protocol::Router, Endpoint};
use iroh_blobs::{
    net_protocol::Blobs,
    rpc::client::blobs::WrapOption,
    store::{ExportFormat, ExportMode},
    ticket::BlobTicket,
    util::SetTagOption,
};
use std::path::PathBuf;
#[tokio::main]
async fn main() -> Result<()> {
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;

    let blobs = Blobs::memory().build(&endpoint);

    let router = Router::builder(endpoint)
        .accept(iroh_blobs::ALPN, blobs.clone())
        .spawn()
        .await?;

    let client = blobs.client();

    let args: Vec<String> = std::env::args().skip(1).collect();

    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();

    match args_ref.as_slice() {
        ["send", filename] => {
            let path: PathBuf = filename.parse()?;
            let abs_path = std::path::absolute(path.clone())?;

            println!("Blaking it");

            let blob = client
                .add_from_path(
                    abs_path.clone(),
                    true,
                    SetTagOption::Auto,
                    WrapOption::NoWrap,
                )
                .await?
                .finish()
                .await?;

            let node_id = router.endpoint().node_id();
            let ticket = BlobTicket::new(node_id.into(), blob.hash, blob.format)?;

            println!("File hashed. Fetch this file by running:");
            println!(
                "cargo run receive {:?} {:?}",
                ticket.to_string(),
                path.clone()
            );
            tokio::signal::ctrl_c().await?;
        }
        ["receive", ticket, filename] => {
            let abs_path: PathBuf = std::path::absolute(filename.parse::<PathBuf>()?.clone())?;
            let ticket: BlobTicket = ticket.parse()?;

            println!("Starting download");

            client
                .download(ticket.hash(), ticket.node_addr().clone())
                .await?
                .finish()
                .await?;

            println!("download finished");

            println!("copying to dest");

            client
                .export(
                    ticket.hash(),
                    abs_path,
                    ExportFormat::Blob,
                    ExportMode::Copy,
                )
                .await?
                .finish()
                .await?;

            println!("Finished download.");
        }
        _ => {
            println!("Commands are wrong")
        }
    }

    println!("Gracefull shutdown");
    Ok(())
}

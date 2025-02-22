use anyhow::Result;
use iroh::{protocol::Router, Endpoint};
use iroh_blobs::{
    net_protocol::Blobs, rpc::client::blobs::WrapOption, ticket::BlobTicket, util::SetTagOption,
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
            let path_clone = path.clone();
            let abs_path = std::path::absolute(path)?;

            println!("Blaking it");

            let blob = client
                .add_from_path(abs_path, true, SetTagOption::Auto, WrapOption::NoWrap)
                .await?
                .finish()
                .await?;

            let node_id = router.endpoint().node_id();
            let ticket = BlobTicket::new(node_id.into(), blob.hash, blob.format)?;
            println!("File hashed. Fetch this file by running:");
            println!("cargo run receive {:?} {:?}", ticket, path_clone);
            tokio::signal::ctrl_c().await?;
        }
        ["receive", _ticket, filename] => {
            todo!()
        }
        _ => {
            println!("Commands are wrong")
        }
    }
    println!("shutting down the node");
    router.shutdown().await?;

    Ok(())
}

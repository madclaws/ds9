use anyhow::Result;
use iroh::{protocol::Router, Endpoint, NodeId, PublicKey};
use iroh_blobs::{
    net_protocol::Blobs,
    rpc::client::blobs::WrapOption,
    store::{ExportFormat, ExportMode},
    ticket::BlobTicket,
    util::SetTagOption,
};
use serde::{Deserialize, Serialize};
use std::{any, path::PathBuf, thread::spawn, u8};
use iroh_gossip::{net::Gossip, proto::TopicId};
// This should handle the file and directory
// fn import(path: PathBuf) {

// }

#[derive(Debug, Deserialize, Serialize)]
enum Message {
    AboutMe {from: NodeId, name: String},
    Message {from: NodeId, text: String}
}

impl Message {
    fn from_bytes(bytes: &[u8]) -> Result<Self>{
        serde_json::from_slice(bytes).map_err(Into::into)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;

    let blobs = Blobs::memory().build(&endpoint);
    let gossip = Gossip::builder().spawn(endpoint.clone()).await?;
    let router = Router::builder(endpoint)
        .accept(iroh_blobs::ALPN, blobs.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn()
        .await?;

    let client = blobs.client();

    let args: Vec<String> = std::env::args().skip(1).collect();

    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();

    match args_ref.as_slice() {
        ["send", path] => {
            let path: PathBuf = path.parse()?;
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
        ["receive", ticket, path] => {
            let abs_path: PathBuf = std::path::absolute(path.parse::<PathBuf>()?.clone())?;
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
        ["comms"] => {
            println!("In comms mode");
            let topic_id = TopicId::from_bytes(rand::random());
            let node_ids: Vec<PublicKey> = vec![];

            let topic = gossip.subscribe(topic_id, node_ids)?;
            
            let (sender, _receiver) = topic.split();
            
            sender.broadcast("sup".into()).await?
        }
        _ => {
            println!("Commands are wrong")
        }
    }

    println!("Gracefull shutdown");
    Ok(())
}

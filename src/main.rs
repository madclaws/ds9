use anyhow::{Context, Ok, Result};
use iroh::{protocol::Router, Endpoint, NodeId, PublicKey};
use iroh_blobs::{
    net_protocol::Blobs,
    rpc::client::blobs::WrapOption,
    store::{mem::Store, ExportFormat, ExportMode, ImportMode},
    ticket::BlobTicket,
    util::SetTagOption,
    BlobFormat,
};
use iroh_gossip::{net::Gossip, proto::TopicId};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{any, path::Component, path::Path, path::PathBuf, thread::spawn, u8};
use walkdir::{self, WalkDir};
// use futures_lite::future::jo;
// This should handle the file and directory

async fn import(path: PathBuf, blobs: Blobs<Store>) -> Result<()> {
    let path = path.canonicalize()?;
    anyhow::ensure!(path.exists(), "path {} does not exist", path.display());
    let root = path.parent().context("context get parent")?;
    let files = WalkDir::new(path.clone()).into_iter();
    let data_sources = files
        .map(|entry| {
            let entry = entry?;
            if !entry.file_type().is_file() {
                return Ok(None);
            }
            let path = entry.into_path();
            let relative = path.strip_prefix(root)?;
            let name = canonicalized_path_to_string(relative, true)?;
            anyhow::Ok(Some((name, path)))
        })
        .filter_map(Result::transpose)
        .collect::<anyhow::Result<Vec<_>>>()?;
    let client = blobs.client();
    let dats_futs = data_sources
        .iter()
        .map(|entry| async {
            let blob = client
                .add_from_path(
                    entry.1.clone(),
                    true,
                    SetTagOption::Auto,
                    WrapOption::NoWrap,
                )
                .await?
                .finish()
                .await?;
            anyhow::Ok(blob)
        })
        .collect::<Vec<_>>();

    let dats = futures_lite::future::block_on(async {
        let mut res = Vec::new();
        for fut in dats_futs {
            res.push(fut.await);
        }
        res
    });
    println!("{:?}", dats);
    Ok(())
}

pub fn canonicalized_path_to_string(
    path: impl AsRef<Path>,
    must_be_relative: bool,
) -> anyhow::Result<String> {
    let mut path_str = String::new();
    let parts = path
        .as_ref()
        .components()
        .filter_map(|c| match c {
            Component::Normal(x) => {
                let c = match x.to_str() {
                    Some(c) => c,
                    None => return Some(Err(anyhow::anyhow!("invalid character in path"))),
                };

                if !c.contains('/') && !c.contains('\\') {
                    Some(Ok(c))
                } else {
                    Some(Err(anyhow::anyhow!("invalid path component {:?}", c)))
                }
            }
            Component::RootDir => {
                if must_be_relative {
                    Some(Err(anyhow::anyhow!("invalid path component {:?}", c)))
                } else {
                    path_str.push('/');
                    None
                }
            }
            _ => Some(Err(anyhow::anyhow!("invalid path component {:?}", c))),
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let parts = parts.join("/");
    path_str.push_str(&parts);
    Ok(path_str)
}

#[derive(Debug, Deserialize, Serialize)]
enum Message {
    AboutMe { from: NodeId, name: String },
    Message { from: NodeId, text: String },
}

impl Message {
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
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
        ["t", path] => {
            import(path.parse::<PathBuf>()?, blobs.clone()).await?;
        }
        _ => {
            println!("Commands are wrong")
        }
    }

    println!("Gracefull shutdown");
    Ok(())
}

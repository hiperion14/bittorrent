use std::fs;
use std::env;
use std::sync::Arc;
mod tracker;
mod torrent_parser;
mod download;
mod file;
mod message;
mod queue;
mod piece;
mod peers;
use bencode::Bee;
use bencode::BeeValue;
use peers::Download;
use torrent_parser::Torrent;

pub type Address = ([u8; 4], u16);

fn read_torrent(path: &String) -> Bee {
    let torrent_result = fs::read(path);
    let torrent = match torrent_result {
        Ok(file) => file,
        Err(error) => panic!("Error on opening the torrent file: {:?}", error),
    };
    
    BeeValue::from_bytes(&torrent)
}


#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let path = &args[1];
    let torrent = Arc::new(Torrent::new(&read_torrent(path)));
    let download = Download::new(&torrent).await;

    download.connect().await;
}
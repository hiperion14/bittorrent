use std::fs;
use std::env;
use std::sync::Arc;
mod tracker;
mod torrent_parser;
mod download;
mod file;
mod message;
mod worker;
mod piece;
use bencode::Bee;
use bencode::BeeValue;
use torrent_parser::Torrent;
use tracker::get_peers;
use worker::work;


fn read_torrent(path: &String) -> Bee {
    let torrent_result = fs::read(path);
    let torrent = match torrent_result {
        Ok(file) => file,
        Err(error) => panic!("Error on opening the torrent file: {:?}", error),
    };
    
    return BeeValue::from_bytes(&torrent);
}


fn main() {
    let args: Vec<String> = env::args().collect();
    let path = &args[1];
    let torrent = Arc::new(Torrent::new(&read_torrent(path)));
    let peers = get_peers(&torrent);

    work(peers.peers, &torrent);
}
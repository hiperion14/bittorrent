use std::{sync::{Arc, Mutex}, collections::HashSet};
use tokio::sync::{mpsc::channel, broadcast};

use crate::{queue::PieceQueue, tracker::get_peers, torrent_parser::Torrent, file::TorrentFiles, download::{Peer, Status}, piece::PieceWrite, Address};


pub struct Download {
    work_queue: Arc<PieceQueue>,
    peers: Arc<Mutex<HashSet<Address>>>,
    files: TorrentFiles,
    torrent: Arc<Torrent>
}

impl Download {
    pub async fn new(torrent: &Arc<Torrent>) -> Self {
        Self {
            work_queue: Arc::new(PieceQueue::new()),
            peers: Arc::new(Mutex::new(HashSet::new())),
            files: TorrentFiles::new(torrent).await,
            torrent: torrent.clone()
        }
    }

    pub async fn connect(&self) {
        let (result_sender, mut result_receiver) = channel::<PieceWrite>(self.torrent.num_pieces);
        let (tx, rx) = broadcast::channel::<Status>(10);

        for tracker in &self.torrent.torrent["announce-list"].get_list().unwrap() {
            let addr = tracker[0].get_string().unwrap();
            let torrent = self.torrent.clone();
            let peers_mutex = self.peers.clone();
            let result_sender = result_sender.clone();
            let work_queue = self.work_queue.clone();
            let rx = rx.resubscribe();
            tokio::spawn(async move {
                let tracker = match get_peers(&torrent, addr).await {
                    Some(a) => a,
                    None => return
                };
                println!("Connected to tracker");
                for peer in tracker.announce.peers {
                    if peers_mutex.lock().unwrap().insert(peer) {
                        let torrent = torrent.clone();
                        let result_sender = result_sender.clone();
                        let work_queue = work_queue.clone();
                        let rx = rx.resubscribe();

                        tokio::spawn(async move {
                            let mut status = Peer::new(peer, work_queue, result_sender, rx, Status::Leeching, torrent);
                            status.connect().await
                        });
                    }
                }
            });

            
        }

        let mut completed = 0;

        for _ in 0..self.torrent.num_pieces {
            let j = result_receiver.recv().await.unwrap();
            self.work_queue.complete(j.piece_index);
            completed += 1;
    
            println!("Completed piece: {}. {:.2}%", j.piece_index, completed as f64 / self.torrent.num_pieces as f64 * 100.0);
    
            self.files.write_to_file(&self.torrent, j.piece_index, j.data);
    
            if completed == self.torrent.num_pieces {
                println!("Finished");
                tx.send(Status::Closing).unwrap();
            }
        }
    }
}
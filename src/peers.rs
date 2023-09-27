use std::{sync::{Arc, mpsc::channel, Mutex}, collections::HashSet, thread};

use crate::{worker::WorkQueue, tracker::get_peers, torrent_parser::Torrent, file::TorrentFiles, download::{connect, Status}, piece::PieceWrite};

pub struct Download {
    work_queue: Arc<WorkQueue>,
    peers: Arc<Mutex<HashSet<([u8; 4], u16)>>>,
    files: TorrentFiles,
    torrent: Arc<Torrent>
}

impl Download {
    pub fn new(torrent: &Arc<Torrent>) -> Self {
        Self {
            work_queue: Arc::new(WorkQueue::new()),
            peers: Arc::new(Mutex::new(HashSet::new())),
            files: TorrentFiles::new(torrent),
            torrent: torrent.clone()
        }
    }

    pub fn connect(&self) {
        let mut tracker_handles = vec![];
        let (result_sender, result_receiver) = channel::<PieceWrite>();
        let result_sender = Arc::new(result_sender);
        
        for tracker in &self.torrent.torrent["announce-list"].get_list().unwrap() {
            let addr = tracker[0].get_string().unwrap();
            let torrent = self.torrent.clone();
            let peers_mutex = self.peers.clone();
            let result_sender = result_sender.clone();
            let work_queue = self.work_queue.clone();


            tracker_handles.push(thread::spawn(move || {
                if let Some(peers) = get_peers(&torrent, addr) {
                    let mut peer_handles = vec![];
                    for peer in peers.announce.peers {
                        if peers_mutex.lock().unwrap().insert(peer) {
                            let torrent = torrent.clone();
                            let result_sender = result_sender.clone();
                            let work_queue = work_queue.clone();


                            peer_handles.push(thread::spawn(move || {
                                let mut status = Status::new(work_queue, result_sender);
                                connect(peer, &mut status, &torrent)
                            }));
                        }
                    }

                    for handle in peer_handles {
                        handle.join().unwrap();
                    }
                }
                
            }));

            
        }

        let mut completed = 0;

        for _ in 0..self.torrent.num_pieces {
            let j = result_receiver.recv().unwrap();
            completed += 1;
    
            println!("Completed piece: {}. {:.2}%", j.piece_index, completed as f64 / self.torrent.num_pieces as f64 * 100.0);
    
            self.files.write_to_file(&self.torrent, j.piece_index, j.data);
    
            if completed == self.torrent.num_pieces {
                println!("Finished");
                for _ in 0..self.torrent.num_pieces {
                    self.work_queue.push_terminate();
                }
            }
        }

        for handle in tracker_handles {
            handle.join().unwrap();
        }
    }
}
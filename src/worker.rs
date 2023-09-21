use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, Condvar};
use std::thread;
use crate::download::{Status, connect};
use crate::file::TorrentFiles;
use crate::piece::{Piece, PieceWrite};
use crate::torrent_parser::Torrent;

pub struct WorkQueue<T> {
    queue: Mutex<Vec<T>>,
    condvar: Condvar,
}

impl<T> WorkQueue<T> {
    pub fn new() -> Self {
        WorkQueue {
            queue: Mutex::new(Vec::new()),
            condvar: Condvar::new(),
        }
    }

    pub fn push(&self, item: T) {
        let mut queue = self.queue.lock().unwrap();
        queue.push(item);
        self.condvar.notify_all();
    }

    pub fn pop(&self, can_process: &dyn Fn(&T, &Vec<bool>) -> bool, bitfield: &Vec<bool>) -> Option<T> {
        let mut queue = self.queue.lock().unwrap();
        
        loop {
            for (i, a) in queue.iter().enumerate() {
                if can_process(a, bitfield) {
                    return Some(queue.remove(i));
                }
            }
            queue = self.condvar.wait(queue).unwrap();
        }
    }
}

pub fn work(peers: Vec<([u8; 4], u16)>, torrent: &Arc<Torrent>) {
    let work_queue = Arc::new(WorkQueue::<Piece>::new());
    let files = TorrentFiles::new(torrent);
    let mut completed = 0;
    let mut handles = vec![];

    for piece_index in 0..torrent.num_pieces {
        work_queue.push(Piece::new(piece_index as i32, &torrent));
    }

    
    let (result_sender, result_receiver) = channel::<PieceWrite>();
    for peer in peers {
        let torrent = Arc::clone(&torrent);
        let work_queue = work_queue.clone();
        let sender = result_sender.clone();
        handles.push(thread::spawn(move || {
            let mut status = Status::new(work_queue, sender);
            connect(peer, &mut status, &torrent)
            // some work here
        }));
    }

    for _ in 0..torrent.num_pieces {
        let j = result_receiver.recv().unwrap();
        completed += 1;

        println!("Completed {:.2}%", completed as f64 / torrent.num_pieces as f64 * 100.0);

        files.write_to_file(torrent, j.piece_index, j.data);

        if completed == torrent.num_pieces {
            println!("Finished");
            for _handle in handles.iter() {
                work_queue.push(Piece::terminate_piece());
            }
        }
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
}
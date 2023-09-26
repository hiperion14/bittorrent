use std::collections::HashMap;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, Condvar};
use std::thread;
use crate::download::{Status, connect};
use crate::file::TorrentFiles;
use crate::piece::PieceWrite;
use crate::torrent_parser::Torrent;

pub struct WorkQueue {
    queue: Mutex<HashMap<usize, usize>>,
    condvar: Condvar,
}

impl WorkQueue {
    pub fn new() -> Self {
        WorkQueue {
            queue: Mutex::new(HashMap::new()),
            condvar: Condvar::new(),
        }
    }

    pub fn push(&self, item: usize, frequency: usize) {
        let mut queue = self.queue.lock().unwrap();
        let count = queue.entry(item).or_insert(0);
        *count += frequency;
        self.condvar.notify_all();
    }

    pub fn pop(&self, can_process: &dyn Fn(usize, &Vec<bool>) -> bool, bitfield: &Vec<bool>) -> Option<(usize, usize)> {
        let mut queue = self.queue.lock().unwrap();
        
        loop {
            let mut count_vec: Vec<_> = queue.clone().into_iter().collect();
            count_vec.sort_by(|a, b| b.1.cmp(&a.1));

            for (item, _) in count_vec.iter() {
                if *item == usize::MAX {
                    return None
                }

                if can_process(*item, bitfield) {
                    return queue.remove_entry(item);
                }
            }
            queue = self.condvar.wait(queue).unwrap();
        }
    }
}

pub fn work(peers: Vec<([u8; 4], u16)>, torrent: &Arc<Torrent>) {
    let work_queue = Arc::new(WorkQueue::new());
    let files = TorrentFiles::new(torrent);
    let mut completed = 0;
    let mut handles = vec![];

    
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

        println!("Completed piece: {}. {:.2}%", j.piece_index, completed as f64 / torrent.num_pieces as f64 * 100.0);

        files.write_to_file(torrent, j.piece_index, j.data);

        if completed == torrent.num_pieces {
            println!("Finished");
            for _handle in handles.iter() {
                work_queue.push(usize::MAX, 1);
            }
        }
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
}
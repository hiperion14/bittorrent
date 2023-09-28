use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Condvar};

pub struct PieceQueue {
    finished: AtomicBool,
    queue: Mutex<HashMap<usize, usize>>,
    condvar: Condvar,
}

impl PieceQueue {
    pub fn new() -> Self {
        PieceQueue {
            finished: AtomicBool::new(false),
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

    pub fn finish(&self) {
        self.finished.store(true, Ordering::Relaxed);
        self.condvar.notify_all();
    }

    pub fn pop(&self, can_process: &dyn Fn(usize, &[bool]) -> bool, bitfield: &[bool]) -> Option<(usize, usize)> {
        let mut queue = self.queue.lock().unwrap();
        
        loop {
            if self.finished.load(Ordering::Acquire) {
                return None
            }

            let mut count_vec: Vec<_> = queue.clone().into_iter().collect();
            count_vec.sort_by(|a, b| b.1.cmp(&a.1));

            for (item, _) in count_vec.iter() {
                if can_process(*item, bitfield) {
                    if let Some((_, freq)) = queue.remove_entry(item) {
                        return Some((*item, freq))
                    }
                }
            }
            queue = self.condvar.wait(queue).unwrap();
        }
    }
}
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, Condvar};

pub struct PieceQueue {
    queue: Mutex<HashMap<usize, usize>>,
    completed: Mutex<HashSet<usize>>,
    condvar: Condvar,
}

impl PieceQueue {
    pub fn new() -> Self {
        PieceQueue {
            queue: Mutex::new(HashMap::new()),
            completed: Mutex::new(HashSet::new()),
            condvar: Condvar::new(),
        }
    }

    pub fn push(&self, item: usize, frequency: usize) {
        if self.completed.lock().unwrap().contains(&item) {
            return
        }

        let mut queue = self.queue.lock().unwrap();
        let count = queue.entry(item).or_insert(0);
        *count += frequency;
        self.condvar.notify_all();
    }

    pub fn complete(&self, item: usize) -> bool {
        self.completed.lock().unwrap().insert(item)
    }

    pub fn pop(&self, can_process: &dyn Fn(usize, &[bool]) -> bool, bitfield: &[bool]) -> (usize, usize) {
        let mut queue = self.queue.lock().unwrap();
        
        loop {
            let mut count_vec: Vec<_> = queue.clone().into_iter().collect();
            count_vec.sort_by(|a, b| b.1.cmp(&a.1));

            for (item, _) in count_vec.iter() {
                if can_process(*item, bitfield) {
                    if let Some((_, freq)) = queue.remove_entry(item) {
                        return (*item, freq)
                    }
                }
            }
            
            queue = self.condvar.wait(queue).unwrap();
        }
    }
}
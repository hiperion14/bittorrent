use std::collections::HashMap;
use std::sync::{Mutex, Condvar};

pub struct WorkQueue {
    queue: Mutex<HashMap<Option<usize>, usize>>,
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
        let count = queue.entry(Some(item)).or_insert(0);
        *count += frequency;
        self.condvar.notify_all();
    }

    pub fn push_terminate(&self) {
        let mut queue = self.queue.lock().unwrap();
        queue.insert(None, 1);
    }

    pub fn pop(&self, can_process: &dyn Fn(usize, &[bool]) -> bool, bitfield: &[bool]) -> Option<(usize, usize)> {
        let mut queue = self.queue.lock().unwrap();
        
        loop {
            let mut count_vec: Vec<_> = queue.clone().into_iter().collect();
            count_vec.sort_by(|a, b| b.1.cmp(&a.1));

            for (item, _) in count_vec.iter() {
                if let Some(piece) = item {
                    if can_process(*piece, bitfield) {
                        if let Some((_, freq)) = queue.remove_entry(item) {
                            return Some((*piece, freq))
                        }
                    }
                } else {
                    return None
                }
            }
            queue = self.condvar.wait(queue).unwrap();
        }
    }
}
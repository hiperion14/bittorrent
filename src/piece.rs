use tokio::{net::TcpStream, io::AsyncWriteExt};

use crate::{torrent_parser::Torrent, message::builders};

pub struct Piece {
    pub piece_index: i32,
    pub frequency: usize,
    pub blocks: Option<Vec<Vec<u8>>>,
    pub length: usize,
    
    blocks_requested: Vec<bool>,
    completed: usize,
    requested: usize,
}

pub struct PieceWrite {
    pub data: Vec<u8>,
    pub piece_index: usize,
}


impl Piece {
    pub fn new(piece_index: i32, frequency: usize, torrent: &Torrent) -> Self {
        Self {
            length: torrent.blocks_per_piece(piece_index) as usize,
            completed: 0,
            requested: 0,
            piece_index,
            blocks: None,
            blocks_requested: vec![false; torrent.blocks_per_piece(piece_index) as usize],
            frequency
        }
    }

    pub fn get_needed(&mut self, torrent: &Torrent) -> Option<usize> {
        //println!("bpp: {}", torrent.blocks_per_piece(self.piece_index));
        if self.blocks.is_none() {
            self.blocks = Some(vec![Vec::new(); torrent.blocks_per_piece(self.piece_index) as usize])
        }

        for (i, requested) in self.blocks_requested.iter().enumerate() {
            if !requested {
                return Some(i);
            }
        }
        None
    }

    pub fn add_block(&mut self, index: usize, data: Vec<u8>) -> bool {
        self.blocks.as_mut().unwrap()[index] = data;
        self.completed += 1;
        self.completed == self.length
    }

    pub fn is_done(&self) -> bool {
        self.requested == self.length
    }

    pub async fn request(&mut self, socket: &mut TcpStream, torrent: &Torrent) -> bool {
        if let Some(block_index) = self.get_needed(torrent) {
            let _ = socket.write_all(&builders::build_request(
                self.piece_index, 
                (block_index * 16384).try_into().unwrap(),
                torrent.block_len(self.piece_index, block_index.try_into().unwrap()).try_into().unwrap(),
            )).await;
            self.requested += 1;
            self.blocks_requested[block_index] = true;
            return false;
        }
        true
    }
}
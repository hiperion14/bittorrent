use bencode::Bee;
use sha1::{Sha1, Digest};

#[derive(Clone)]
pub struct Torrent {
    pub size: i128,
    pub piece_len: i32,
    pub num_pieces: usize,
    pub torrent: Bee,
    pub hashes: Vec<Vec<u8>>
}




impl Torrent {
    pub fn new(torrent: &Bee) -> Torrent {
        Torrent {
            size: size(torrent),
            piece_len: torrent["info"]["piece length"].get_int().unwrap().try_into().unwrap(),
            num_pieces: torrent["info"]["pieces"].get_raw().unwrap().len()/20,
            torrent: torrent.to_owned(),
            hashes: torrent["info"]["pieces"].get_raw().unwrap().chunks(20).map(|a| a.to_vec()).collect()
        }
    }
}

impl Torrent {
    pub fn info_hash(&self) -> Vec<u8> {
        let mut hasher = Sha1::new();
        hasher.update(&self.torrent["info"].get_decoded());
        let result: Vec<u8> = hasher.finalize().to_vec();
        result
    }
    pub fn piece_len(&self, piece_index: i32) -> i32 {
        let total_len = self.size;
        let piece_len: i128 = self.piece_len.try_into().unwrap();
        let last_piece_length: i32 = (total_len % piece_len).try_into().unwrap();
        let last_piece_index = total_len/piece_len;
    
        if last_piece_index == piece_index.try_into().unwrap() {last_piece_length} else {piece_len.try_into().unwrap()}
    
    }
    
    pub fn blocks_per_piece(&self, piece_index: i32) -> i32 {
        let piece_length = self.piece_len(piece_index);
        piece_length / 16384
    }
    
    pub fn block_len(&self, piece_index: i32, block_index: i32) -> i32 {
        let piece_length = self.piece_len(piece_index);
        let last_piece_length = piece_length % 16384;
        let last_piece_index = piece_length / 16384;
        if block_index == last_piece_index {last_piece_length} else {16384}
    }
}



pub fn size(torrent: &Bee) -> i128 {
    let dict = torrent["info"].get_dict().unwrap();
    if dict.contains_key("length") {
        return dict.get("length").unwrap().get_int().unwrap();
    }
    let binding = dict.get("files").unwrap().get_list().unwrap();
    let test = binding.iter().map(|a| a["length"].get_int().unwrap()).reduce(|a, b| a + b).unwrap();
    test
}


use bytes::{BytesMut, BufMut};
use rand::{rngs::ThreadRng, Rng};
use crate::torrent_parser::Torrent;

pub fn build_handshake(torrent: &Torrent) -> BytesMut {
    let mut buf = BytesMut::with_capacity(68);
    let mut rng: ThreadRng = rand::thread_rng();
    buf.put_u8(19);
    buf.put_slice(b"BitTorrent protocol");
    buf.put_u32(0);
    buf.put_u32(0);
    
    buf.put_slice(&Torrent::info_hash(torrent));
    
    buf.put_slice(b"-AT0001-");
    buf.put_u64(rng.gen::<u64>());
    buf.put_u32(rng.gen::<u32>());
    return buf;
}

pub fn build_keep_alive() -> BytesMut {
    let mut buf = BytesMut::with_capacity(4);
    buf.put_u32(0);
    return buf;
}

pub fn build_choke() -> BytesMut {
    let mut buf = BytesMut::with_capacity(5);
    buf.put_u32(1);
    buf.put_u8(0);
    return buf;
}

pub fn build_unchoke() -> BytesMut {
    let mut buf = BytesMut::with_capacity(5);
    buf.put_u32(1);
    buf.put_u8(1);
    return buf;
}

pub fn build_interested() -> BytesMut {
    let mut buf = BytesMut::with_capacity(5);
    buf.put_u32(1);
    buf.put_u8(2);
    return buf;
}

pub fn build_uninterested() -> BytesMut {
    let mut buf = BytesMut::with_capacity(5);
    buf.put_u32(1);
    buf.put_u8(3);
    return buf;
}

pub fn build_have(piece_index: u32) -> BytesMut {
    let mut buf = BytesMut::with_capacity(9);
    //len
    buf.put_u32(5);
    //id=4 have message
    buf.put_u8(4);
    //piece index
    buf.put_u32(piece_index);
    return buf;
}

pub fn build_bitfield(bitfield: &[u8]) -> BytesMut {
    let mut buf = BytesMut::with_capacity(bitfield.len()+5);
    //bitfield length
    buf.put_u32((bitfield.len()+1).try_into().unwrap());
    //id=5 bitfield message
    buf.put_u8(5);
    //bitifield bits
    buf.put_slice(bitfield);
    return buf
}

pub fn build_request(piece_index: i32, block_index: i32, block_length: u32) -> BytesMut {
    let mut buf = BytesMut::with_capacity(17);
    //length: 13
    buf.put_u32(13);
    //id=6 request message
    buf.put_u8(6);
    //piece_index
    buf.put_i32(piece_index);
    buf.put_i32(block_index);
    buf.put_u32(block_length);
    return buf
}

pub fn build_piece(piece_index: i32, block_index: i32, block: Vec<u8>) -> BytesMut {
    let mut buf = BytesMut::with_capacity(block.len()+13);
    //length
    buf.put_u32((block.len()+9).try_into().unwrap());
    //id=7 piece message
    buf.put_u8(7);
    //piece_index
    buf.put_i32(piece_index);
    buf.put_i32(block_index);
    buf.put_slice(&block);
    return buf;
}

pub fn build_cancel(piece_index: i32, block_index: i32, block_length: i32) -> BytesMut {
    let mut buf = BytesMut::with_capacity(17);
    //length
    buf.put_u32(13);
    //id=8 cancel message
    buf.put_u8(8);
    //piece_index
    buf.put_i32(piece_index);
    buf.put_i32(block_index);
    buf.put_i32(block_length);
    return buf
}

pub fn build_port(port: u16) -> BytesMut {
    let mut buf = BytesMut::with_capacity(7);
    //length
    buf.put_u32(3);
    //id=9 port message
    buf.put_u8(9);
    //listen_port
    buf.put_u16(port);
    return buf
}


pub struct PieceMessage {
    pub piece_index: i32,
    pub block_begin: i32,
    pub block: Vec<u8>
}

pub struct HaveMessage {
    pub piece_index: i32
}

pub struct BitfieldMessage {
    pub bitfield: Vec<bool>
}

pub struct Message {
    pub size: i32,
    pub id: i8,
    pub have_message: Option<HaveMessage>,
    pub bitfield_message: Option<BitfieldMessage>,
    pub piece_message: Option<PieceMessage>
}

pub fn parse(msg: &[u8]) -> Option<Message> {
    let id = if msg.len() > 4 {i8::from_be_bytes(msg[4..5].try_into().unwrap())} else {-1};
    if msg.len() < 5 {
        return None
    }
    let payload = msg[5..].to_vec();
    let size = i32::from_be_bytes(msg[0..4].try_into().unwrap());
    let mut message = Message {
        size,
        id,
        have_message: None,
        bitfield_message: None,
        piece_message: None,
    };

    if id == 4 {
        //Have Message
        message.have_message = Some(HaveMessage {
            piece_index: i32::from_be_bytes(payload[0..4].try_into().unwrap()),
        });
    } else if id == 5 {
        //Bitfield Message
        let mut bitfield: Vec<bool> = Vec::with_capacity(payload.len()*8);
        for byte in payload {
            for i in 0..8 {
                let byte = ((byte) >> (7-i)) & 1;
                bitfield.push(byte==1)
            }
        }

        message.bitfield_message = Some(BitfieldMessage {
            bitfield
        });
    } else if id == 7 {
        //Piece Message
        message.piece_message = Some(PieceMessage {
            piece_index: i32::from_be_bytes(payload[0..4].try_into().unwrap()),
            block_begin: i32::from_be_bytes(payload[4..8].try_into().unwrap()),
            block: payload[8..].to_vec()
        })
    }

    return Some(message);
}
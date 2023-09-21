use std::{net::{SocketAddr, TcpStream, Shutdown}, io::{Read, Write}, sync::{Arc, mpsc::Sender}};

use std::time::Instant;

use crate::{message::{build_interested, parse, build_handshake, BitfieldMessage, PieceMessage}, torrent_parser::Torrent, worker::WorkQueue, piece::{Piece, PieceWrite}};

pub struct Status {
    worker: Arc<WorkQueue<Piece>>,
    piece: Option<Piece>,
    choked: bool,
    last_piece: Instant,
    bitfield: Vec<bool>,
    sender: Sender<PieceWrite>,
}

impl Status {
    pub fn new(worker: Arc<WorkQueue<Piece>>, sender: Sender<PieceWrite>) -> Self {
        Status {
            worker,
            choked: false,
            bitfield: Vec::new(),
            piece: None,
            last_piece: Instant::now(),
            sender: sender,
        }
    } 
}

pub fn exit_socket(socket: &mut TcpStream, status: &mut Status, torrent: &Arc<Torrent>) {
    exit(status, torrent);
    socket.shutdown(Shutdown::Both).unwrap();
}

pub fn exit(status: &mut Status, torrent: &Arc<Torrent>) {
    if status.piece.is_some() {
        status.worker.push(Piece::new(status.piece.as_ref().unwrap().piece_index, torrent))
    }
}

fn on_socket(msg: &[u8], socket: &mut TcpStream, status: &mut Status, torrent: &Arc<Torrent>) -> bool {
    if is_handshake(msg) {
        let _ = socket.write_all(&build_interested());
        return true;
    } else {
        let m = parse(msg);
        if m.is_none() {
            return false
        }
        let m = m.unwrap();
        match m.id {
            0 => choke_handler(status),
            1 => unchoke_handler(socket, status, torrent),
            4 => have_handler(),
            5 => return bitfield_handler(status, &m.bitfield_message.unwrap()),
            7 => return piece_handler(socket, status, torrent, &m.piece_message.unwrap()),
            _ => return true,
        }
        return true
    }
}

pub fn get_packet_size(packet: &[u8]) -> Option<usize> {
    if packet.len() < 2 {
        return None
    }
    if packet[1] == 0x42 && packet[2] == 0x69 {
        let packet_size: usize = (packet[0]+49).try_into().unwrap();
        return Some(packet_size)
    } else {
        if packet.len() < 4 {
            return None;
        } 
        let packet_size: usize = (u32::from_be_bytes(packet[0..4].try_into().unwrap())+4).try_into().unwrap();
        return Some(packet_size)
    }
}

pub fn connect(peer: ([u8; 4], u16), status: &mut Status, torrent: &Arc<Torrent>) {
    let mut socket = match TcpStream::connect(SocketAddr::from(peer.to_owned())) {
        Ok(s) => s,
        Err(_) => {
            return exit(status, torrent);
        }
    };

    let mut buffer: Vec<u8> = Vec::new();
    let mut temp_buffer: [u8; 65536] = [0; 65536];
    let mut current_size: i32 = 0;

    socket.write_all(&build_handshake(&torrent)).unwrap();
    status.last_piece = Instant::now();

    
    loop {
        let stream_result = match socket.read(&mut temp_buffer) {
            Ok(n) if n == 0 => return exit_socket(&mut socket, status, torrent),
            Ok(n) => n,
            Err(_) => {
                return exit_socket(&mut socket, status, torrent);
            }
        };
        let size: i32 = stream_result.try_into().unwrap();

        buffer.extend_from_slice(&temp_buffer[..size.try_into().unwrap()]);
        current_size += size;

        while current_size > 0 {
            
            if status.last_piece.elapsed().as_secs() > 10 {
                return exit_socket(&mut socket, status, torrent);
            }

            let packet_size = get_packet_size(&buffer);
            if packet_size.is_none() {
                return exit_socket(&mut socket, status, torrent);
            }
            let packet_size = packet_size.unwrap();

            let packet_size_i32: i32 = packet_size.try_into().unwrap();
            if current_size < packet_size_i32 {
                break;
            }

            if !on_socket(&buffer[0..packet_size], &mut socket, status, &torrent) { 
                return exit_socket(&mut socket, status, torrent);
            };
            
            buffer.drain(0..packet_size);
            current_size -= packet_size_i32;
        }

    }
}

fn is_handshake(msg: &[u8]) -> bool {
    return msg.len() == (u8::from_be_bytes(msg[0..1].try_into().unwrap())+49).into() 
    && String::from_utf8(msg[1..20].try_into().unwrap()).unwrap() == "BitTorrent protocol"
}

fn choke_handler(status: &mut Status) {
    status.choked = true;
}

fn unchoke_handler(socket: &mut TcpStream, status: &mut Status, torrent: &Arc<Torrent>) {
    status.choked = false;
    request_piece(socket, status, torrent);
    
}

fn have_handler() {
    //todo!();
}

fn bitfield_handler(status: &mut Status, bitfield_message: &BitfieldMessage) -> bool {
    status.bitfield = Vec::with_capacity(bitfield_message.bitfield.len());
    for b in bitfield_message.bitfield.iter() {
        status.bitfield.push(*b);
    }
    status.piece = status.worker.pop(&is_available, &status.bitfield);
    if status.piece.as_ref().unwrap().length == 0 {
        return false
    }
    true
}

fn is_available(piece: &Piece, bitfield: &Vec<bool>) -> bool {
    return bitfield[piece.piece_index as usize];
}

fn piece_handler(socket: &mut TcpStream, status: &mut Status, torrent: &Arc<Torrent>, piece_resp: &PieceMessage) -> bool {
    let completed = status.piece.as_mut().unwrap().add_block((piece_resp.block_begin/16384) as usize, piece_resp.block.clone());
    
    if completed {
        let piece_write = PieceWrite {
            data: status.piece.as_ref().unwrap().blocks.as_ref().unwrap().into_iter().map(|a| a.clone()).flatten().collect(),
            piece_index: status.piece.as_ref().unwrap().piece_index as usize,
        };
        status.sender.send(piece_write).unwrap();

        status.piece = status.worker.pop(&is_available, &status.bitfield);

        if status.piece.as_ref().unwrap().length == 0 {
            return false;
        }

        status.last_piece = Instant::now();
        
        request_piece(socket, status, torrent);
    }
    true
    
}

fn request_piece(socket: &mut TcpStream, status: &mut Status, torrent: &Arc<Torrent>) {
    if status.choked {return};
    if status.piece.is_none() {
        return;
    }
    while !status.piece.as_ref().unwrap().is_done(){
        let stop = status.piece.as_mut().unwrap().request(socket, &torrent);
        if stop {
            return
            
        }
    }
}
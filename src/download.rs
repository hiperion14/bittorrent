use std::{net::SocketAddr, sync::Arc};
use sha1::{Sha1, Digest};
use tokio::sync::mpsc::Sender;
use tokio::sync::broadcast::Receiver;
use std::time::Instant;
use tokio::{net::TcpStream, io::{AsyncWriteExt, AsyncReadExt}};
use crate::{message::{build_interested, parse, build_handshake, BitfieldMessage, PieceMessage}, torrent_parser::Torrent, worker::PieceQueue, piece::{Piece, PieceWrite}, Address};

#[derive(Debug, Clone)]
pub enum Status {
    Closing,
    Leeching,
    Peering,
    Seeding,
    Halted
}

pub struct Peer {
    worker: Arc<PieceQueue>,
    piece: Option<Piece>,
    frequency: Option<usize>,
    choked: bool,
    last_piece: Instant,
    bitfield: Vec<bool>,
    piece_sender: Sender<PieceWrite>,
    status_receiver: Receiver<Status>,
    status: Status
}

impl Peer {
    pub fn new(worker: Arc<PieceQueue>, piece_sender: Sender<PieceWrite>, status_receiver: Receiver<Status>, status: Status) -> Self {
        Peer {
            worker,
            choked: false,
            bitfield: Vec::new(),
            piece: None,
            frequency: None,
            last_piece: Instant::now(),
            piece_sender,
            status_receiver,
            status
        }
    }
}

pub fn exit_socket(socket: &mut TcpStream, status: &mut Peer) {
    exit(status);
    let _ = socket.shutdown();
}

pub fn exit(peer: &mut Peer) {
    if peer.piece.is_some() {
        peer.worker.push(peer.piece.as_ref().unwrap().piece_index as usize, peer.frequency.unwrap())
    }
}

async fn on_socket(msg: &[u8], socket: &mut TcpStream, peer: &mut Peer, torrent: &Arc<Torrent>) -> bool {
    if is_handshake(msg) {
        let _ = socket.write_all(&build_interested()).await;
        true
    } else {
        let m = parse(msg);
        if m.is_none() {
            return false
        }
        let m = m.unwrap();
        match m.id {
            0 => choke_handler(peer),
            1 => unchoke_handler(socket, peer, torrent).await,
            5 => return bitfield_handler(peer, &m.bitfield_message.unwrap(), torrent),
            7 => return piece_handler(socket, peer, torrent, &m.piece_message.unwrap()).await,
            _ => return true,
        }
        true
    }
}

pub fn get_packet_size(packet: &[u8]) -> Option<usize> {
    if packet.len() < 2 {
        return None
    }
    if packet[1] == 0x42 && packet[2] == 0x69 {
        let packet_size: usize = (packet[0]+49).try_into().unwrap();
        Some(packet_size)
    } else {
        if packet.len() < 4 {
            return None;
        } 
        let packet_size: usize = (u32::from_be_bytes(packet[0..4].try_into().unwrap())+4).try_into().unwrap();
        Some(packet_size)
    }
}

pub async fn connect(peer_addr: Address, peer: &mut Peer, torrent: &Arc<Torrent>) {
    let mut socket = match TcpStream::connect(SocketAddr::from(peer_addr.to_owned())).await {
        Ok(s) => s,
        Err(_) => {
            return exit(peer);
        }
    };

    let mut buffer: Vec<u8> = Vec::new();
    let mut temp_buffer: [u8; 65536] = [0; 65536];
    let mut current_size: i32 = 0;

    socket.write_all(&build_handshake(torrent)).await.unwrap();
    peer.last_piece = Instant::now();

    
    loop {
        tokio::select! {
            stream = socket.read(&mut temp_buffer) => {
                let size: i32 = match stream {
                    Ok(n) if n == 0 => return exit_socket(&mut socket, peer),
                    Ok(n) => n,
                    Err(_) => {
                        return exit_socket(&mut socket, peer);
                    }
                }.try_into().unwrap();

                buffer.extend_from_slice(&temp_buffer[..size.try_into().unwrap()]);
                current_size += size;

                while current_size > 0 { 
                    if peer.last_piece.elapsed().as_secs() > 10 {
                        return exit_socket(&mut socket, peer);
                    }
                    if let Some(packet_size) = get_packet_size(&buffer) {
                        let packet_size_i32: i32 = packet_size.try_into().unwrap();
                        if current_size < packet_size_i32 {
                            break;
                        }

                        if !on_socket(&buffer[0..packet_size], &mut socket, peer, torrent).await { 
                            return exit_socket(&mut socket, peer);
                        };
                        
                        buffer.drain(0..packet_size);
                        current_size -= packet_size_i32;
                        continue;
                    }

                    return exit_socket(&mut socket, peer);
                }
            },

            status = peer.status_receiver.recv() => {
                if let Ok(status) = status {
                    peer.status = status;
                    match peer.status {
                        Status::Closing => return exit_socket(&mut socket, peer),
                        Status::Leeching => todo!(),
                        Status::Seeding => todo!(),
                        Status::Peering => todo!(),
                        Status::Halted => todo!(),
                    }
                }
                
            }
        }
    }
}

fn is_handshake(msg: &[u8]) -> bool {
    msg.len() == (u8::from_be_bytes(msg[0..1].try_into().unwrap())+49).into() 
    && String::from_utf8(msg[1..20].try_into().unwrap()).unwrap() == "BitTorrent protocol"
}

fn choke_handler(status: &mut Peer) {
    status.choked = true;
}

async fn unchoke_handler(socket: &mut TcpStream, status: &mut Peer, torrent: &Arc<Torrent>) {
    status.choked = false;
    request_piece(socket, status, torrent).await;
    
}

fn add_piece(status: &mut Peer, torrent: &Arc<Torrent>) -> bool {
    if let Some((piece, freq)) = status.worker.pop(&is_available, &status.bitfield) {
        status.piece = Some(Piece::new(piece as i32, torrent));
        status.frequency = Some(freq);
        return true
    }

    false
}

fn bitfield_handler(status: &mut Peer, bitfield_message: &BitfieldMessage, torrent: &Arc<Torrent>) -> bool {
    status.bitfield = Vec::with_capacity(bitfield_message.bitfield.len());
    for (i, b) in bitfield_message.bitfield.iter().enumerate() {
        if *b {
            status.worker.push(i, 1);
        }
        status.bitfield.push(*b);
    }
    
    add_piece(status, torrent)
}

fn is_available(piece: usize, bitfield: &[bool]) -> bool {
    bitfield[piece]
}

fn is_correct(piece: &PieceWrite, torrent: &Arc<Torrent>) -> bool {
    let mut hasher = Sha1::new();
    hasher.update(&piece.data);
    let result: Vec<u8> = hasher.finalize().to_vec();
    result == torrent.hashes[piece.piece_index]
}

async fn piece_handler(socket: &mut TcpStream, status: &mut Peer, torrent: &Arc<Torrent>, piece_resp: &PieceMessage) -> bool {
    let completed = status.piece.as_mut().unwrap().add_block((piece_resp.block_begin/16384) as usize, piece_resp.block.clone());
    
    if completed {
        let piece_write = PieceWrite {
            data: status.piece.as_ref().unwrap().blocks.as_ref().unwrap().iter().flat_map(|a| a.clone()).collect(),
            piece_index: status.piece.as_ref().unwrap().piece_index as usize,
        };
        
        if is_correct(&piece_write, torrent) {
            if status.piece_sender.send(piece_write).await.is_err() {
                return false
            }
        } else {
            status.worker.push(status.piece.as_ref().unwrap().piece_index as usize, status.frequency.unwrap());
        }

        if !add_piece(status, torrent) {
            return false
        }

        status.last_piece = Instant::now();

        if !status.choked {
            request_piece(socket, status, torrent).await;
        }
    }
    true
}

async fn request_piece(socket: &mut TcpStream, status: &mut Peer, torrent: &Arc<Torrent>) {
    if status.choked {return};
    if status.piece.is_none() {
        return;
    }
    while !status.piece.as_ref().unwrap().is_done(){
        let stop = status.piece.as_mut().unwrap().request(socket, torrent).await;
        if stop {
            return
            
        }
    }
}
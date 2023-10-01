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
    status: Status,
    addr: Address,
    torrent: Arc<Torrent>
}

impl Peer {
    pub fn new(addr: Address, worker: Arc<PieceQueue>, piece_sender: Sender<PieceWrite>, status_receiver: Receiver<Status>, status: Status, torrent: Arc<Torrent>) -> Self {
        Peer {
            worker,
            choked: false,
            bitfield: Vec::new(),
            piece: None,
            frequency: None,
            last_piece: Instant::now(),
            piece_sender,
            status_receiver,
            torrent,
            status,
            addr
        }
    }

    fn exit_socket(&mut self, socket: &mut TcpStream) {
        self.exit();
        let _ = socket.shutdown();
    }

    fn exit(&mut self) {
        if self.piece.is_some() {
            self.worker.push(self.piece.as_ref().unwrap().piece_index as usize, self.frequency.unwrap())
        }
    }

    async fn on_socket(&mut self, msg: &[u8], socket: &mut TcpStream) -> bool {
        if is_handshake(msg) {
            let _ = socket.write_all(&build_interested()).await;
        } else {
            let m = match parse(msg) {
                Some(a) => a,
                None => return false
            };

            match m.id {
                0 => self.choke_handler(),
                1 => self.unchoke_handler(socket).await,
                5 => return self.bitfield_handler(&m.bitfield_message.unwrap()),
                7 => return self.piece_handler(socket, &m.piece_message.unwrap()).await,
                _ => return true,
            }
        }
        true
    }

    fn set_status(&mut self, status: Status, socket: &mut TcpStream) {
        self.status = status;
        match self.status {
            Status::Closing => return self.exit_socket(socket),
            Status::Leeching => todo!(),
            Status::Seeding => todo!(),
            Status::Peering => todo!(),
            Status::Halted => todo!(),
        }
    }

    pub async fn connect(&mut self) {
        let mut socket = match TcpStream::connect(SocketAddr::from(self.addr.to_owned())).await {
            Ok(s) => s,
            Err(_) => {
                return self.exit();
            }
        };
    
        let mut buffer: Vec<u8> = Vec::new();
        let mut temp_buffer: [u8; 65536] = [0; 65536];
        let mut current_size: i32 = 0;
    
        socket.write_all(&build_handshake(&self.torrent)).await.unwrap();
        self.last_piece = Instant::now();
    
        
        loop {
            tokio::select! {
                stream = socket.read(&mut temp_buffer) => {
                    let size: i32 = match stream {
                        Ok(n) if n == 0 => return self.exit_socket(&mut socket),
                        Ok(n) => n,
                        Err(_) => {
                            return self.exit_socket(&mut socket);
                        }
                    }.try_into().unwrap();
    
                    buffer.extend_from_slice(&temp_buffer[..size.try_into().unwrap()]);
                    current_size += size;
    
                    while current_size > 0 { 
                        if self.last_piece.elapsed().as_secs() > 10 {
                            return self.exit_socket(&mut socket);
                        }
                        if let Some(packet_size) = get_packet_size(&buffer) {
                            let packet_size_i32: i32 = packet_size.try_into().unwrap();
                            if current_size < packet_size_i32 {
                                break;
                            }
    
                            if !self.on_socket(&buffer[0..packet_size], &mut socket).await { 
                                return self.exit_socket(&mut socket);
                            };
                            
                            buffer.drain(0..packet_size);
                            current_size -= packet_size_i32;
                            continue;
                        }
    
                        return self.exit_socket(&mut socket);
                    }
                },
                
                status = self.status_receiver.recv() => {
                    if let Ok(status) = status {
                        self.set_status(status, &mut socket)
                    }
                }
            }
        }
    }

    fn choke_handler(&mut self) {
        self.choked = true;
    }

    async fn unchoke_handler(&mut self, socket: &mut TcpStream) {
        self.choked = false;
        self.request_piece(socket).await;
    }

    fn pop_piece(&mut self) -> bool {
        if let Some((piece, freq)) = self.worker.pop(&is_available, &self.bitfield) {
            self.piece = Some(Piece::new(piece as i32, &self.torrent));
            self.frequency = Some(freq);
            return true
        }

        false
    }

    fn bitfield_handler(&mut self, bitfield_message: &BitfieldMessage) -> bool {
        self.bitfield = Vec::with_capacity(bitfield_message.bitfield.len());
        for (i, b) in bitfield_message.bitfield.iter().enumerate() {
            if *b {
                self.worker.push(i, 1);
            }
            self.bitfield.push(*b);
        }
        
        self.pop_piece()
    }

    async fn piece_handler(&mut self, socket: &mut TcpStream, piece_resp: &PieceMessage) -> bool {
        let completed = self.piece.as_mut().unwrap().add_block((piece_resp.block_begin/16384) as usize, piece_resp.block.clone());
        
        if completed {
            let piece_write = PieceWrite {
                data: self.piece.as_ref().unwrap().blocks.as_ref().unwrap().iter().flat_map(|a| a.clone()).collect(),
                piece_index: self.piece.as_ref().unwrap().piece_index as usize,
            };
            
            if is_correct(&piece_write, &self.torrent) {
                if self.piece_sender.send(piece_write).await.is_err() {
                    return false
                }
            } else {
                self.worker.push(self.piece.as_ref().unwrap().piece_index as usize, self.frequency.unwrap());
            }
    
            if !self.pop_piece() {
                return false
            }
    
            self.last_piece = Instant::now();
    
            if !self.choked {
                self.request_piece(socket).await;
            }
        }

        true
    }

    async fn request_piece(&mut self, socket: &mut TcpStream) {
        if self.choked || self.piece.is_none() {
            return
        };

        while !self.piece.as_ref().unwrap().is_done(){
            if self.piece.as_mut().unwrap().request(socket, &self.torrent).await {
                return
            }
        }
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

fn is_handshake(msg: &[u8]) -> bool {
    msg.len() == (u8::from_be_bytes(msg[0..1].try_into().unwrap())+49).into() 
    && String::from_utf8(msg[1..20].try_into().unwrap()).unwrap() == "BitTorrent protocol"
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

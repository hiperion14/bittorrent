use bytes::{BytesMut, BufMut};
use rand::{Rng, rngs::ThreadRng};
use url::Url;
use crate::{torrent_parser::Torrent, Address};
use tokio::net::UdpSocket;


#[derive(Debug)]
struct Resp {
    _action: u32,
    _transaction_id: u32,
    connection_id: u64
}

#[derive(Debug)]
pub struct Announce {
    pub action: u32,
    pub transaction_id: u32,
    pub interval: u32,
    pub leechers: u32,
    pub seeders: u32,
    pub peers: Vec<Address>
}

pub struct Tracker {
    pub announce: Announce,
    pub url: String,
}

impl Announce {
    pub fn from_buff(buf: &[u8], num_bytes: usize) -> Announce {
        let mut peers: Vec<Address> = vec![];
        for n in 0..(num_bytes-20)/6 {
            let ip: [u8; 4] = buf[n*6+20..n*6+24].try_into().unwrap();
            let peer: Address = (
                ip,
                u16::from_be_bytes(buf[n*6+24..n*6+26].try_into().unwrap())
            );
            peers.push(peer);
        }

        Announce { 
            action: u32::from_be_bytes(buf[0..4].try_into().unwrap()), 
            transaction_id: u32::from_be_bytes(buf[4..8].try_into().unwrap()),
            interval: u32::from_be_bytes(buf[8..12].try_into().unwrap()),
            leechers: u32::from_be_bytes(buf[12..16].try_into().unwrap()),
            seeders: u32::from_be_bytes(buf[16..20].try_into().unwrap()),
            peers
        }

    }
}

impl Resp {
    pub fn from_buff(buf: &[u8]) -> Resp {
        Resp {
            _action: u32::from_be_bytes(buf[0..4].try_into().unwrap()),
            _transaction_id: u32::from_be_bytes(buf[4..8].try_into().unwrap()),
            connection_id: u64::from_be_bytes(buf[8..16].try_into().unwrap()),
        }
    }
}

enum RespTypes {
    Connect,
    Announce
}

impl RespTypes {
    pub fn from_u32(x: u32) -> RespTypes {
        match x {
            0 => RespTypes::Connect,
            1 => RespTypes::Announce,
            _ => RespTypes::Announce,
        }
    }
}



async fn on_socket(buf: &[u8], socket: &UdpSocket, tracker: &String, torrent: &Torrent, num_bytes: usize) -> Option<Announce> {
    let action = RespTypes::from_u32(u32::from_be_bytes(buf[0..4].try_into().unwrap()));
    match action {
        RespTypes::Announce => {
            Some(Announce::from_buff(buf, num_bytes))
        },
        RespTypes::Connect => {
            let resp = Resp::from_buff(buf);
            let _ = socket.send_to(&build_announce_req(resp.connection_id, torrent, 6881), tracker).await;
            None
        }
    }
}

pub async fn get_peers(torrent: &Torrent, addr: String) -> Option<Tracker> {
    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(n) => n,
        Err(_) => return None 
    };

    if !addr.starts_with("udp") {
        return None
    }
    let addr = parse_udp(addr);
    match socket.send_to(&build_conn_req(), &addr).await {
        Ok(_) => {},
        Err(_) => return None
    };
    
    let mut buf = [0; 2048];
    loop {
        // Receive data into the buffer
        let (num_bytes, _src_addr) = match socket.recv_from(&mut buf).await {
            Ok(a) => a,
            Err(_) => return None
        };

        let result = on_socket(&buf, &socket, &addr, torrent, num_bytes).await;
        if let Some(announce) = result {
            return Some(Tracker {
                announce,
                url: addr.to_string(),
            })
        }
    }

}

fn build_conn_req() -> BytesMut {
    let mut rng: ThreadRng = rand::thread_rng();
    let mut buffer = BytesMut::with_capacity(16);
    buffer.put_u64(0x41727101980);
    buffer.put_u32(0);
    buffer.put_u32(rng.gen::<u32>());
    buffer
}

fn build_announce_req(conn_id: u64, torrent: &Torrent, port: u16) -> BytesMut {
    let mut buf = BytesMut::with_capacity(98);
    let mut rng: ThreadRng = rand::thread_rng();
    buf.put_u64(conn_id);
    buf.put_u32(1);
    buf.put_u32(rng.gen::<u32>());

    buf.put_slice(torrent.info_hash().as_slice());

    //peer-id
    buf.put_slice(b"-AT0001-");
    buf.put_u64(rng.gen::<u64>());
    buf.put_u32(rng.gen::<u32>());

    buf.put_u64(0);

    buf.put_u64(torrent.size.try_into().unwrap());

    buf.put_u64(0);

    buf.put_u32(0);
    buf.put_u32(0);
    buf.put_u32(rng.gen::<u32>());
    buf.put_i32(-1);
    buf.put_u16(port);
    

    buf
}

fn parse_udp(udp: String) -> String {
    let url = Url::parse(&udp).unwrap();
    let host = url.host_str().unwrap();
    let port = url.port().unwrap().to_string();
    let addr = format!("{}:{}", host, port);
    addr
}
use crate::torrent_parser::Torrent;
use std::fs::{File, OpenOptions};
use std::io::{SeekFrom, Seek, Write};

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub offset: i128,
    pub path: String,
    pub size: i128,
}

pub struct TorrentFiles {
    files: Vec<FileInfo>
}

impl TorrentFiles {
    pub async fn new(torrent: &Torrent) -> Self {
        let mut files: Vec<FileInfo> = Vec::new();
        let mut constant_size: i128 = 0;
        if let Some(files_torrent) = torrent.torrent["info"].get_dict().unwrap().get("files") {
            let files_torrent = files_torrent.get_list().unwrap();
            for file in files_torrent {
                let size = file["length"].get_int().unwrap();
                files.push(FileInfo { 
                    offset: constant_size,
                    path: file["path"][0].get_string().unwrap(),
                    size
                });
                constant_size += size;
    
                let mut file = File::create(file["path"][0].get_string().unwrap()).unwrap();
                file.seek(SeekFrom::End(size as i64 - 1)).unwrap();
                file.write_all(&[0]).unwrap();
            }
        } else {
            let name = torrent.torrent["info"]["name"].get_string().unwrap();
            let size = torrent.torrent["info"]["length"].get_int().unwrap();
            files.push(
                FileInfo { offset: 0, path: name, size: size }
            )
        }
        
        
        Self {
            files
        }
    }

    pub fn write_to_file(&self, torrent: &Torrent, piece_index: usize, data: Vec<u8>) {
        let start_bytes = piece_index as i128 * torrent.piece_len as i128;
        let finish_bytes = start_bytes + torrent.piece_len as i128;
        
        for file_info in &self.files {
            let start_offset = file_info.offset;
            let end_offset = file_info.size + file_info.offset;

            if start_offset <= finish_bytes && end_offset >= start_bytes {
                let data_start = (start_offset - start_bytes).max(0) as usize;
                let data_end = (end_offset - start_bytes).min(data.len() as i128) as usize;

                let mut file = OpenOptions::new().write(true).create(true).open(&file_info.path).unwrap();
                file.seek(SeekFrom::Start((data_start as i128 + start_bytes - start_offset) as u64)).unwrap();
                file.write_all(&data[data_start..data_end]).unwrap()
            }
        }
    }
}
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use crate::torrent_parser::Torrent;

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
    pub fn new(torrent: &Torrent) -> Self {
        let mut files: Vec<FileInfo> = Vec::new();
        let files_torrent = torrent.torrent["info"]["files"].get_list().unwrap();
        let mut constant_size: i128 = 0;
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
        Self {
            files
        }
    }

    pub fn write_to_file(&self, torrent: &Torrent, piece_index: usize, data: Vec<u8>) {
        let piece_start_bytes = piece_index as i128 * torrent.piece_len as i128;
        let piece_finish_bytes = piece_start_bytes + torrent.piece_len as i128;
        for file_info in &self.files {
            let file_start_offset = file_info.offset;
            let file_end_offset = file_info.size + file_info.offset;
            if file_start_offset <= piece_finish_bytes && file_end_offset >= piece_start_bytes {
                let data_start = (file_start_offset - piece_start_bytes).max(0) as usize;
                let data_end = (file_end_offset - piece_start_bytes).min(data.len() as i128) as usize;
                let mut file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .open(&file_info.path).unwrap();

                file.seek(SeekFrom::Start((data_start as i128 + piece_start_bytes - file_start_offset) as u64)).unwrap();
                file.write_all(&data[data_start..data_end]).unwrap()
            }
        }
    }
}
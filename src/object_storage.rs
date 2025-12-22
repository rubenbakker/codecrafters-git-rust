use std::fs::{File, Permissions};
use anyhow::anyhow;
use bytes::Buf;
use flate2::read::ZlibDecoder;
use std::io::{BufRead, Read};
use std::path::PathBuf;

pub enum GitObject {
    Blob(Blob),
    Tree(Tree),
}

pub struct Blob {
    content: Vec<u8>,
}

pub enum TreeEntryPermission {
    Directory,
    RegularFile,
    SymbolicLink,
    Executable,
}

pub struct TreeEntry {
    pub permission: TreeEntryPermission,
    pub name: String,
    pub hash: Vec<u8>,
}

pub struct Tree {
    pub entries: Vec<TreeEntry>,
}

pub struct ObjectStorage {}

impl GitObject {
    pub fn from_file_path(path: &PathBuf) -> anyhow::Result<Self> {
        let mut file = File::open(path)?;
        let mut data = vec![];
        file.read_to_end(&mut data)?;
        Self::from_data(data.as_ref())
    }

    pub fn from_data(data: &[u8]) -> anyhow::Result<Self> {
        let mut zlib_decoder = ZlibDecoder::new(&data[..]);
        let mut result: Vec<u8> = vec![];
        zlib_decoder.read_to_end(&mut result)?;
        let mut type_prefix_buf: Vec<u8> = vec![0; 4];
        let mut reader = result.reader();
        reader.read_exact(&mut type_prefix_buf)?;
        let type_prefix = String::from_utf8(type_prefix_buf)?;
        let mut content: Vec<u8> = vec![];
        let null_byte : u8 = 0;
        let _ = (reader).skip_until(null_byte);
        let _ = reader.read_to_end(&mut content)?;
        if type_prefix == "blob" {
            Ok(GitObject::Blob(Blob::from(&content)?))
        } else if type_prefix == "tree" {
            Ok(GitObject::Tree(Tree::from(&content)?))
        } else {
            Err(anyhow!("Only blob and tree objects are supported ({})", type_prefix))
        }
    }
}

impl Blob {
    fn from(content: &[u8]) -> anyhow::Result<Self> {
        let null_byte: u8 = 0;
        let mut reader = content.reader();
        (reader).skip_until(null_byte)?;
        let mut content: Vec<u8> = vec![];
        let _ = reader.read_to_end(&mut content)?;
        eprintln!("length: {}", content.len());
        Ok(Self {
            content: Vec::from(content),
        })
    }

    fn content(self: &Self) -> Vec<u8> {
        self.content.clone()
    }

    pub fn as_str(self: &Self) -> anyhow::Result<String> {
        let v = self.content.to_vec();
        Ok(String::from_utf8(v)?)
    }
}

impl Tree {
    fn from(content: &[u8]) -> anyhow::Result<Self> {
        let mut reader = content.reader();
        let null_byte: u8 = 0;
        let space_byte: u8 = 32;
        let mut entries: Vec<TreeEntry> = vec![];
        loop {
            let mut permission_buf: Vec<u8> = vec![];
            if let Ok(size) = reader.read_until(space_byte, &mut permission_buf) {
                if size == 0 {
                    break;
                }
                eprintln!("before permission {}", permission_buf.len());
                let permission = String::from_utf8(permission_buf)?;
                eprintln!("permission: {} {}", permission, permission.len());
                let mut name_buf: Vec<u8> = vec![];
                let _ = reader.read_until(null_byte, &mut name_buf)?;
                let name = String::from_utf8(name_buf)?;
                let name = name.get(0..name.len()-1).unwrap();
                eprintln!("name: {}", name);
                let mut hash_bytes_buf = vec![0; 20];
                let permission = match permission.as_str().trim() {
                    "100644" => TreeEntryPermission::RegularFile,
                    "40000" => TreeEntryPermission::Directory,
                    "100755" => TreeEntryPermission::Executable,
                    "120000" => TreeEntryPermission::SymbolicLink,
                    &_ => Err(anyhow!("Unsupported permission value {}", permission.as_str()))?
                };
                let _ = (reader).read_exact(&mut hash_bytes_buf)?;
                entries.push(TreeEntry {
                    permission,
                    name: String::from(name),
                    hash: hash_bytes_buf
                });
            } else {
                break;
            }
        }
        Ok(Self { entries })
    }
}

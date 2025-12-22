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
        let mut header_string = String::new();
        let _ = result.reader().read_to_string(&mut header_string)?;
        let content = result.as_ref();
        if header_string.starts_with("blob") {
            Ok(GitObject::Blob(Blob::from(content)?))
        } else if header_string.starts_with("tree") {
            Ok(GitObject::Tree(Tree::from(content)?))
        } else {
            Err(anyhow!("Only blob and tree objects are supported."))
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
                let permission = String::from_utf8(permission_buf)?;
                let mut name_buf: Vec<u8> = vec![];
                let _ = reader.read_until(null_byte, &mut name_buf)?;
                let name = String::from_utf8(name_buf)?;
                let mut hash_bytes_buf = vec![20];
                let permission = match permission.as_str() {
                    "100644" => TreeEntryPermission::RegularFile,
                    "040000" => TreeEntryPermission::Directory,
                    "100755" => TreeEntryPermission::Executable,
                    "120000" => TreeEntryPermission::SymbolicLink,
                    &_ => Err(anyhow!("Unsupported permission value {}", permission.as_str()))?
                };
                let hash_bytes = (reader).read_exact(&mut hash_bytes_buf)?;
                entries.push(TreeEntry {
                    permission,
                    name,
                    hash: hash_bytes_buf
                })
            } else {
                break;
            }
        }
        Ok(Self { entries })
    }
}

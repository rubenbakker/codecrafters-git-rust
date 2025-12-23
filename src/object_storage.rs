use anyhow::anyhow;
use bytes::Buf;
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use sha1::{Digest, Sha1};
use std::fs::File;
use std::io::{BufRead, Read, Write};
use std::{fs, path};
use std::path::PathBuf;

pub enum GitObject {
    Blob(Blob),
    Tree(Tree),
}

pub struct Blob {
    content: Vec<u8>,
}

pub struct ObjectStorage {}

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
        let null_byte: u8 = 0;
        let _ = (reader).skip_until(null_byte);
        let _ = reader.read_to_end(&mut content)?;
        if type_prefix == "blob" {
            Ok(GitObject::Blob(Blob::from(&content)?))
        } else if type_prefix == "tree" {
            Ok(GitObject::Tree(Tree::from(&content)?))
        } else {
            Err(anyhow!(
                "Only blob and tree objects are supported ({})",
                type_prefix
            ))
        }
    }
}

impl Blob {
    pub fn new_with_file_path(path: &PathBuf) -> anyhow::Result<Self> {
        let mut file = File::open(path)?;
        let mut content: Vec<u8> = vec![];
        file.read_to_end(&mut content)?;
        Ok(Self { content })
    }

    fn from(content: &[u8]) -> anyhow::Result<Self> {
        Ok(Self {
            content: Vec::from(content),
        })
    }

    pub fn as_str(self: &Self) -> anyhow::Result<String> {
        let v = self.content.to_vec();
        Ok(String::from_utf8(v)?)
    }

    pub fn write_to_oject_storage(self: &Self) -> anyhow::Result<String> {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        let mut full_content: Vec<u8> = vec![];
        let header = format!("blob {}\0", self.content.len())
            .as_bytes()
            .to_vec()
            .clone();
        full_content.write_all(header.as_slice())?;
        full_content.write_all(self.content.as_slice())?;
        let hash = Sha1::digest(&full_content);
        let hash = base16ct::lower::encode_string(&hash);
        e.write_all(full_content.as_ref())?;
        let compressed = e.finish()?;
        let dir_path = ObjectStorage::get_dir_for_hash(hash.as_str())?;
        if !dir_path.exists() {
            let _result = fs::create_dir(dir_path)?;
        }
        let output_file_path = ObjectStorage::get_path_for_hash(hash.as_str())?;
        let mut output_file = File::create(output_file_path)?;
        output_file.write_all(compressed.as_ref())?;
        Ok(hash)
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
                let name = name.get(0..name.len() - 1).unwrap();
                let mut hash_bytes_buf = vec![0; 20];
                let permission = match permission.as_str().trim() {
                    "100644" => TreeEntryPermission::RegularFile,
                    "40000" => TreeEntryPermission::Directory,
                    "100755" => TreeEntryPermission::Executable,
                    "120000" => TreeEntryPermission::SymbolicLink,
                    &_ => Err(anyhow!(
                        "Unsupported permission value {}",
                        permission.as_str()
                    ))?,
                };
                let _ = (reader).read_exact(&mut hash_bytes_buf)?;
                entries.push(TreeEntry {
                    permission,
                    name: String::from(name),
                    hash: hash_bytes_buf,
                });
            } else {
                break;
            }
        }
        Ok(Self { entries })
    }
}

impl TreeEntryPermission {
    pub fn to_string(self: &Self) -> String {
        match self {
            TreeEntryPermission::Directory => "40000",
            TreeEntryPermission::RegularFile => "100644",
            TreeEntryPermission::SymbolicLink => "120000",
            TreeEntryPermission::Executable => "100755",
        }
        .to_string()
    }
}

impl TreeEntry {
    pub fn to_hash_hex_string(self: &Self) -> String {
        base16ct::lower::encode_string(&self.hash)
    }
}

impl ObjectStorage {
    pub fn init_cwd() -> anyhow::Result<()> {
        fs::create_dir(".git")?;
        fs::create_dir(".git/objects")?;
        fs::create_dir(".git/refs")?;
        fs::write(".git/HEAD", "ref: refs/heads/main\n")?;
        Ok(())
    }
    pub fn get_dir_for_hash(hash: &str) -> anyhow::Result<PathBuf> {
        let dir = hash.get(0..2).ok_or(anyhow!("invalid hex"))?;
        let dir_path = path::Path::new(".git").join("objects").join(dir);
        Ok(dir_path)
    }

    pub fn get_path_for_hash(hash: &str) -> anyhow::Result<PathBuf> {
        let filename = hash.get(2..).ok_or(anyhow!("invalid hex"))?;
        let file_path = Self::get_dir_for_hash(hash)?.join(filename);
        Ok(file_path)
    }

}

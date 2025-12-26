use anyhow::anyhow;
use bytes::{Buf, BufMut};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use sha1::{Digest, Sha1};
use std::fs::File;
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::{fs, path};

pub enum GitObject {
    Blob(Blob),
    Tree(Tree),
    Commit(Commit),
}

pub struct Blob {
    content: Vec<u8>,
}

pub type Sha = [u8; 20];

pub struct Commit {
    tree: Sha,
    parents: Vec<Sha>,
    message: String,
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
    pub hash: Sha,
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
        let mut zlib_decoder = ZlibDecoder::new(data);
        let mut result: Vec<u8> = vec![];
        zlib_decoder.read_to_end(&mut result)?;
        let mut type_prefix_buf: Vec<u8> = vec![0; 4];
        let mut reader = result.reader();
        reader.read_exact(&mut type_prefix_buf)?;
        let type_prefix = String::from_utf8(type_prefix_buf)?;
        let mut content: Vec<u8> = vec![];
        let _ = (reader).skip_until(0);
        let _ = reader.read_to_end(&mut content)?;
        if type_prefix == "blob" {
            Ok(GitObject::Blob(Blob::from(&content)?))
        } else if type_prefix == "tree" {
            Ok(GitObject::Tree(Tree::from(&content)?))
        } else if type_prefix == "comm" {
            Ok(GitObject::Commit(Commit::from(&content)?))
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

    pub fn as_str(&self) -> anyhow::Result<String> {
        let v = self.content.to_vec();
        Ok(String::from_utf8(v)?)
    }

    pub fn write_to_object_storage(&self) -> anyhow::Result<Sha> {
        let mut full_content: Vec<u8> = vec![];
        let header = ObjectStorage::header_for_content_length("blob", self.content.len())?;
        full_content.write_all(header.as_slice())?;
        full_content.write_all(self.content.as_slice())?;
        ObjectStorage::write_object(&full_content)
    }
}

impl Tree {
    fn write_to_object_storage(&self) -> anyhow::Result<Sha> {
        let content: Vec<u8> = vec![];
        let mut content_writer = content.writer();
        for entry in &self.entries {
            _ = content_writer.write(entry.permission.to_string_repr().as_bytes())?;
            _ = content_writer.write(b" ")?;
            _ = content_writer.write(entry.name.as_bytes())?;
            _ = content_writer.write(b"\0")?;
            _ = content_writer.write(&entry.hash)?;
        }
        let content = content_writer.get_ref();
        let header = ObjectStorage::header_for_content_length("tree", content.len())?;
        let mut full_content: Vec<u8> = vec![];
        full_content.write_all(header.as_slice())?;
        full_content.write_all(content)?;
        let hash = ObjectStorage::write_object(&full_content)?;
        Ok(hash)
    }

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
                (reader).read_exact(&mut hash_bytes_buf)?;
                entries.push(TreeEntry {
                    permission,
                    name: String::from(name),
                    hash: hash_bytes_buf.try_into().unwrap(),
                });
            } else {
                break;
            }
        }
        Ok(Self { entries })
    }
}

impl TreeEntryPermission {
    pub fn to_string_repr(&self) -> String {
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
    pub fn to_hash_hex_string(&self) -> String {
        ObjectStorage::sha_to_hex_string(&self.hash)
    }
}

impl Commit {
    fn from(content: &[u8]) -> anyhow::Result<Self> {
        let mut reader: bytes::buf::Reader<&[u8]> = content.reader();
        let mut tree: Option<Sha> = None;
        let mut parents: Vec<Sha> = vec![];
        while let Some((prefix, payload)) = Self::read_commit_line(&mut reader)? {
            match prefix.as_str() {
                "tree" => tree = Some(ObjectStorage::hex_string_to_sha(&payload)?),
                "parent" => parents.push(ObjectStorage::hex_string_to_sha(&payload)?),
                "committer" => (),
                "author" => (),
                _ => (),
            }
        }
        let mut message: Vec<u8> = vec![];
        _ = reader.read_to_end(&mut message);
        Ok(Commit {
            tree: tree.unwrap(),
            parents,
            message: String::from_utf8(message)?,
        })
    }

    fn read_commit_line(
        reader: &mut bytes::buf::Reader<&[u8]>,
    ) -> anyhow::Result<Option<(String, String)>> {
        let mut line = String::new();
        _ = reader.read_line(&mut line)?;
        let split = line.split_once(" ");
        Ok(split.map(|(prefix, payload)| (prefix.to_owned(), payload.trim().to_owned())))
    }

    fn write_to_object_storage(&self) -> anyhow::Result<Sha> {
        let content: Vec<u8> = vec![];
        let mut content_writer = content.writer();
        content_writer.write(b"tree ")?;
        content_writer.write(ObjectStorage::sha_to_hex_string(&self.tree).as_bytes())?;
        content_writer.write(b"\n")?;
        for parent in &self.parents {
            content_writer.write(b"parent ")?;
            content_writer.write(ObjectStorage::sha_to_hex_string(parent).as_bytes())?;
            content_writer.write(b"\n")?;
        }
        let author_line = format!(
            "author {} <{}> {} +{:04}\n",
            "Ruben Bakker", "ruben@uncomplex.ch", 0, 0
        );
        content_writer.write(author_line.as_bytes())?;
        let committer_line = format!(
            "committer {} <{}> {} +{:04}\n",
            "Ruben Bakker", "ruben@uncomplex.ch", 0, 0
        );
        content_writer.write(committer_line.as_bytes())?;
        content_writer.write(b"\n")?;
        content_writer.write(&self.message.as_bytes())?;
        content_writer.write(b"\n")?;
        let content = content_writer.get_ref();
        let header = ObjectStorage::header_for_content_length("commit", content.len())?;
        let mut full_content: Vec<u8> = vec![];
        full_content.write_all(header.as_slice())?;
        full_content.write_all(content)?;
        let hash = ObjectStorage::write_object(&full_content)?;
        Ok(hash)
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

    pub fn write_object(content: &[u8]) -> anyhow::Result<Sha> {
        let hash: Sha = Sha1::digest(&content).to_vec().try_into().unwrap();
        let hash_string = Self::sha_to_hex_string(&hash);
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        e.write_all(content.as_ref())?;
        let compressed = e.finish()?;
        let dir_path = ObjectStorage::get_dir_for_hash(hash_string.as_str())?;
        if !dir_path.exists() {
            let _result = fs::create_dir(dir_path)?;
        }
        let output_file_path = ObjectStorage::get_path_for_hash(hash_string.as_str())?;
        let mut output_file = File::create(output_file_path)?;
        output_file.write_all(compressed.as_ref())?;
        Ok(hash)
    }

    pub fn header_for_content_length(header_type: &str, length: usize) -> anyhow::Result<Vec<u8>> {
        Ok(format!("{} {}\0", header_type, length)
            .as_bytes()
            .to_vec()
            .clone())
    }

    pub fn write_tree_cwd() -> anyhow::Result<Sha> {
        Self::write_tree(&PathBuf::from("."))
    }

    pub fn write_tree(path: &PathBuf) -> anyhow::Result<Sha> {
        let dir = fs::read_dir(&path)?;
        let mut tree_entries: Vec<TreeEntry> = vec![];
        for entry in dir {
            if let Ok(entry) = entry {
                if entry.file_name() == ".git" {
                    continue;
                }
                let file_name = entry.file_name().to_str().unwrap().to_string();
                let file_type = entry.file_type()?;
                if file_type.is_dir() {
                    let hash = Self::write_tree(&entry.path())?;
                    tree_entries.push(TreeEntry {
                        permission: TreeEntryPermission::Directory,
                        name: file_name,
                        hash,
                    });
                } else {
                    let blob = Blob::new_with_file_path(&entry.path())?;
                    let hash = blob.write_to_object_storage()?;
                    (tree_entries).push(TreeEntry {
                        permission: TreeEntryPermission::RegularFile,
                        name: file_name,
                        hash,
                    });
                }
            }
        }
        tree_entries.sort_by(|a, b| a.name.cmp(&b.name));
        // TDO
        let tree = Tree {
            entries: tree_entries,
        };
        tree.write_to_object_storage()
    }

    pub(crate) fn commit_tree(
        tree_sha: &Sha,
        parent_sha: &Sha,
        message: &str,
    ) -> anyhow::Result<Sha> {
        let parents: Vec<Sha> = vec![parent_sha.to_owned()];
        let commit = Commit {
            tree: tree_sha.to_owned(),
            parents,
            message: String::from(message),
        };
        commit.write_to_object_storage()
    }

    pub fn sha_to_hex_string(sha: &Sha) -> String {
        base16ct::lower::encode_string(sha)
    }

    pub fn hex_string_to_sha(hex_str: &str) -> anyhow::Result<Sha> {
        let mut sha = [0u8; 20];
        base16ct::mixed::decode(hex_str, &mut sha)?;
        Ok(sha)
    }

    pub fn checkout_sha(path: &PathBuf, sha: &Sha) -> anyhow::Result<()> {
        let object = ObjectStorage::git_object_from_sha(sha)?;
        match object {
            GitObject::Tree(tree) => {
                for entry in &tree.entries {
                    match entry.permission {
                        TreeEntryPermission::Directory => {
                            let mut dir = path.clone();
                            dir.push(&entry.name);
                            std::fs::create_dir(&dir)?;
                            Self::checkout_sha(&dir, &entry.hash)?;
                        }
                        TreeEntryPermission::RegularFile => {
                            let mut filepath = path.clone();
                            filepath.push(&entry.name);
                            Self::checkout_sha(&filepath, &entry.hash)?;
                        }
                        TreeEntryPermission::SymbolicLink => todo!(),
                        TreeEntryPermission::Executable => todo!(),
                    }
                }
                Ok(())
            }
            GitObject::Blob(blob) => {
                let mut file = File::create(path)?;
                file.write_all(&blob.content)?;
                Ok(())
            }
            GitObject::Commit(_) => Ok(()),
        }
    }

    pub(crate) fn checkout(sha: &Sha) -> anyhow::Result<()> {
        if let GitObject::Commit(commit) = Self::git_object_from_sha(sha)? {
            let path = std::path::absolute(".")?;
            ObjectStorage::checkout_sha(&path, &commit.tree)?;
            Ok(())
        } else {
            Err(anyhow!("{:?} isn't a commit", sha))
        }
    }

    fn git_object_from_sha(sha: &Sha) -> anyhow::Result<GitObject> {
        let file_path = ObjectStorage::get_path_for_hash(&ObjectStorage::sha_to_hex_string(sha))?;
        GitObject::from_file_path(&file_path)
    }
}

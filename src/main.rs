use anyhow::anyhow;
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use sha1::{Digest, Sha1};
#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path;
use std::path::PathBuf;
use std::string::String;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        if args[1] == "init" {
            fs::create_dir(".git")?;
            fs::create_dir(".git/objects")?;
            fs::create_dir(".git/refs")?;
            fs::write(".git/HEAD", "ref: refs/heads/main\n")?;
            println!("Initialized git directory");
        } else if args[1] == "cat-file" {
            if args.len() > 3 && args[2] == "-p" {
                let hash = args[3].to_string();
                let file_path = get_path_for_hash(hash.as_str())?;
                let file_path_str = file_path.to_str().unwrap();
                let mut file = File::open(file_path_str)?;
                let mut data = vec![];
                file.read_to_end(&mut data)?;
                let mut zlib_decoder = ZlibDecoder::new(&data[..]);
                let mut result = String::new();
                zlib_decoder.read_to_string(&mut result)?;
                if result.starts_with("blob") {
                    let result_str = result.as_str();
                    let parts: Vec<&str> = result_str.split("\0").collect();
                    let content = result.get(parts[0].len()..).unwrap();
                    print!("{}", content);
                }
            }
        } else if args[1] == "hash-object" {
            if args.len() > 3 && args[2] == "-w" {
                let path = args[3].to_string();
                let mut file = File::open(path.as_str())?;
                let mut content: Vec<u8> = vec![];
                file.read_to_end(&mut content)?;
                let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
                let mut full_content : Vec<u8> = vec![];
                let header = format!("blob {}\0", content.len()).as_bytes().to_vec().clone();
                full_content.write_all(header.as_slice())?;
                full_content.write_all(content.as_slice())?;
                let hash = Sha1::digest(&full_content);
                let hash = base16ct::lower::encode_string(&hash);
                print!("{}", hash);
                (e).write_all(full_content.as_ref())?;
                let compressed = e.finish()?;
                let dir_path = get_dir_for_hash(hash.as_str())?;
                if !dir_path.exists() {
                    let _result = fs::create_dir(dir_path)?;
                }
                let output_file_path = get_path_for_hash(hash.as_str())?;
                let mut output_file = File::create(output_file_path)?;
                (output_file).write_all(compressed.as_ref())?;
            }
        } else {
            println!("unknown command: {}", args[1]);
        }
    } else {
        println!("Usage init or cat-file");
    }
    Ok(())
}

fn get_dir_for_hash(hash: &str) -> anyhow::Result<PathBuf> {
    let dir = hash.get(0..2).ok_or(anyhow!("invalid hex"))?;
    let dir_path = path::Path::new(".git").join("objects").join(dir);
    Ok(dir_path)
}

fn get_path_for_hash(hash: &str) -> anyhow::Result<PathBuf> {
    let filename = hash.get(2..).ok_or(anyhow!("invalid hex"))?;
    let file_path = get_dir_for_hash(hash)?.join(filename);
    Ok(file_path)
}

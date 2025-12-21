#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use std::path;
use std::string::String;
use flate2::read::ZlibDecoder;
use flate2::Compression;
use std::fs::File;
use std::io::Read;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        if args[1] == "init" {
            fs::create_dir(".git").unwrap();
            fs::create_dir(".git/objects").unwrap();
            fs::create_dir(".git/refs").unwrap();
            fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
            println!("Initialized git directory")
        } else if args[1] == "cat-file" {
            if args.len() > 3 {
                if args[2] == "-p" {
                    let hash = args[3].to_string();
                    let dir = hash.get(0..2).unwrap();
                    let filename = hash.get(2..).unwrap();
                    let file_path = path::Path::new(".git")
                        .join("objects")
                        .join(dir)
                        .join(filename);
                    let file_path_str = file_path.to_str().unwrap();
                    eprintln!(
                        "hash: {} -> {}",
                        file_path_str,
                        fs::exists(file_path_str).unwrap()
                    );
                    let mut file = File::open(file_path_str).unwrap();
                    let mut data = vec!();
                    file.read_to_end(&mut data).unwrap();
                    let mut zlib_decoder = ZlibDecoder::new(&data[..]);
                    let mut result = String::new();
                    zlib_decoder.read_to_string(&mut result).unwrap();
                    if result.starts_with("blob") {
                        let result_str = result.as_str();
                        let parts: Vec<&str> = result_str.split("\0").collect();
                        print!("{}", parts[1]);
                    }
                }
            }
        } else {
            println!("unknown command: {}", args[1])
        }
    } else {
        println!("Usage init or cat-file")
    }
}

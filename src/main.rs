mod object_storage;

use crate::object_storage::{Blob, GitObject, ObjectStorage};
#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
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
                cat_file(hash)?;
            }
        } else if args[1] == "hash-object" {
            if args.len() > 3 && args[2] == "-w" {
                let path = args[3].to_string();
                hash_object(path)?
            }
        } else if args[1] == "ls-tree" {
            if args.len() > 2 {
                let name_only = args[2] == "--name-only";
                let hash = args.last().unwrap().to_string();
                ls_tree(hash, name_only)?;
            }
        } else {
            println!("unknown command: {}", args[1]);
        }
    } else {
        println!("Usage init or cat-file");
    }
    Ok(())
}

fn ls_tree(hash: String, name_only: bool) -> anyhow::Result<()> {
    let file_path = ObjectStorage::get_path_for_hash(hash.as_str())?;
    eprintln!("file_path: {}", file_path.to_str().unwrap());
    if let GitObject::Tree(tree) = GitObject::from_file_path(&file_path)? {
        for entry in tree.entries {
            if name_only {
                println!("{}", entry.name)
            } else {
                println!("{} {} {}", entry.permission.to_string(), entry.name, entry.to_hash_hex_string())
            }
        }
    } else {
        eprintln!("not a tree object");
    }
    Ok(())
}

fn cat_file(hash: String) -> anyhow::Result<()> {
    let file_path = ObjectStorage::get_path_for_hash(hash.as_str())?;
    if let GitObject::Blob(blob) = GitObject::from_file_path(&file_path)? {
        print!("{}", &blob.as_str()?)
    }
    Ok(())
}


fn hash_object(path: String) -> anyhow::Result<()> {
    let path = PathBuf::from(path);
    let blob = Blob::new_with_file_path(&path)?;
    let hash = blob.write_to_oject_storage()?;
    println!("{}", hash);
    Ok(())
}


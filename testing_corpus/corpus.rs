use std::fs::{File, create_dir_all, exists, write};
use std::io;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use refpack::format::Reference;
use refpack::{CompressionOptions, easy_compress};
use ureq::AsSendBody;
use zip::ZipArchive;

pub const CORPUS_DIR: &str = "testing_corpus";
pub const SILESIA_ZIP: &str = "silesia.zip";
pub const MARKER_FILE: &str = "corpus_prepared.txt";
pub const MARKER_CONTENTS: &str = "This is an automatically generated file to mark that the \
                                   corpus has been prepared for tests and benchmarks. Delete it \
                                   to regenerate the files.";
pub const UNCOMPRESSED_DIR: &str = "uncompressed";
pub const COMPRESSED_DIR: &str = "compressed";

// We leech off microsofts data in this house
pub const SILESIA_CORPUS_URL: &str = "https://sun.aei.polsl.pl/~sdeor/corpus/silesia.zip";

pub fn prepare_corpus() -> Result<(), io::Error> {
    let marker_path = Path::new(CORPUS_DIR).join(MARKER_FILE);
    let marker_exists = exists(&marker_path)?;
    if marker_exists {
        return Ok(());
    }
    println!("Preparing corpus...");
    println!("Downloading zip...");
    let mut download_buf = vec![];
    ureq::get(SILESIA_CORPUS_URL)
        .call()
        .expect("Failed to download corpus")
        .as_body()
        .into_reader()
        .read_to_end(&mut download_buf)?;
    let mut outfile = File::create(Path::new(CORPUS_DIR).join("silesia.zip"))?;
    outfile.write_all(&download_buf)?;
    println!("Decompressing zip...");
    create_dir_all(Path::new(CORPUS_DIR).join(UNCOMPRESSED_DIR))?;
    let silesia_path = Path::new(CORPUS_DIR).join(SILESIA_ZIP);
    let file = File::open(&silesia_path)?;
    let mut zip = ZipArchive::new(file)?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let outpath = Path::new(CORPUS_DIR)
            .join(UNCOMPRESSED_DIR)
            .join(file.name());
        let mut outfile = File::create(&outpath)?;
        io::copy(&mut file, &mut outfile)?;
    }
    println!("Compressing files with refpack...");
    let compressed_dir = Path::new(CORPUS_DIR).join(COMPRESSED_DIR);
    create_dir_all(&compressed_dir)?;
    let uncompressed_dir = Path::new(CORPUS_DIR).join(UNCOMPRESSED_DIR);
    for entry in uncompressed_dir.read_dir()? {
        let entry = entry?.file_name();
        let mut uncompressed_file =
            File::open(Path::new(CORPUS_DIR).join(UNCOMPRESSED_DIR).join(&entry))?;
        let mut read_data = vec![];
        uncompressed_file.read_to_end(&mut read_data)?;
        let compressed =
            easy_compress::<Reference>(&read_data, CompressionOptions::Optimal).unwrap();
        let mut compressed_file =
            File::create(Path::new(CORPUS_DIR).join(COMPRESSED_DIR).join(&entry))?;
        compressed_file.write_all(&compressed)?;
    }
    File::create(&marker_path)?;
    write(&marker_path, MARKER_CONTENTS)?;
    Ok(())
}

pub const SILESIA_CORPUS_LIST: [&str; 12] = [
    "dickens", "mozilla", "mr", "nci", "ooffice", "osdb", "reymont", "samba", "sao", "webster",
    "x-ray", "xml",
];

pub fn get_uncompressed_file(file: &str) -> Option<PathBuf> {
    if !SILESIA_CORPUS_LIST.contains(&file) {
        return None;
    }
    Some(Path::new(CORPUS_DIR).join(UNCOMPRESSED_DIR).join(file))
}

pub fn get_compressed_file(file: &str) -> Option<PathBuf> {
    if !SILESIA_CORPUS_LIST.contains(&file) {
        return None;
    }
    Some(Path::new(CORPUS_DIR).join(COMPRESSED_DIR).join(file))
}

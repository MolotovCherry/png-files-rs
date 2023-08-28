mod png;

use std::{
    fs,
    io::{self},
    path::PathBuf,
};

use bincode::error::{DecodeError, EncodeError};
use clap::Parser;
use flate2::{CompressError, DecompressError};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Decode files from PNG
    #[arg(short, long, required = true)]
    #[arg(conflicts_with = "encode")]
    decode: bool,

    /// Encode files into PNG
    #[arg(short, long, required = true)]
    #[arg(conflicts_with = "decode")]
    encode: bool,

    /// The input file path
    #[arg(short, long, required = true)]
    input: PathBuf,

    /// The file path to output to in encode mode
    /// The output directory to decode files to in decode mode
    #[arg(short, long, default_value = ".")]
    output: PathBuf,

    /// The file path to output to in encode mode
    /// In decode mode, the list of files to decode
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

#[derive(thiserror::Error, Debug)]
pub enum PngFilesError {
    #[error("{0}")]
    Msg(&'static str),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0:?}")]
    Encode(#[from] EncodeError),
    #[error("{0:?}")]
    Decode(#[from] DecodeError),
    #[error("{0:?}")]
    Compress(#[from] CompressError),
    #[error("{0:?}")]
    Decompress(#[from] DecompressError),
}

fn main() -> Result<(), PngFilesError> {
    let args = Args::parse();

    let image = fs::read(args.input)?;

    let mut png = png::Png::new(image)?;

    let data = png.get_file("bar");
    println!("{data:?}");
    png.insert_file("bar", "bazeroonie".as_bytes().to_vec(), true)?;

    std::fs::write("foos.png", png.into_bytes()).unwrap();

    Ok(())
}

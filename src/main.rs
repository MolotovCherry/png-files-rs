mod png;

use std::{
    borrow::Cow,
    fs,
    io::{self},
    path::PathBuf,
};

use bincode::error::{DecodeError, EncodeError};
use clap::Parser;
use flate2::{CompressError, DecompressError};

use self::png::Png;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Decode files from PNG
    #[arg(short, long, required = true)]
    #[arg(conflicts_with_all = ["encode", "remove"])]
    decode: bool,

    /// Encode files into PNG
    #[arg(short, long, required = true)]
    #[arg(conflicts_with_all = ["decode", "remove"])]
    encode: bool,

    // Remove files from PNG
    #[arg(short, long, required = true)]
    #[arg(conflicts_with_all = ["encode", "decode"])]
    remove: bool,

    /// The input file path
    #[arg(short, long, required = true)]
    input: PathBuf,

    /// The file path to output to in encode mode
    /// The output directory to decode files to in decode mode
    /// Does nothing in remove mode
    #[arg(short, long, default_value = ".")]
    output: PathBuf,

    /// In encode mode, the list of files to encode into output file
    /// In decode mode, the list of files to decode from input file
    /// In remove mode, the list of files to remove from input file
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

#[derive(thiserror::Error, Debug)]
pub enum PngFilesError {
    #[error("{0}")]
    Msg(Cow<'static, str>),
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

    let image = fs::read(&args.input)?;

    let mut png = Png::new(image)?;

    if args.encode {
        for file in args.files {
            let data = std::fs::read(&file)?;
            let key = file.file_name();
            // key is the base filename + ext
            let key = key.unwrap().to_str().unwrap();
            png.insert_file(key, data, true)?;
        }

        std::fs::write(args.output, png.into_bytes())?;
    } else if args.decode {
        for file in args.files {
            let key = file.file_name();
            // key is the base filename + ext
            let key = key.unwrap().to_str().unwrap();

            let file = png
                .get_file(key)
                .ok_or(PngFilesError::Msg(Cow::Owned(format!(
                    "Key {key} not found in image"
                ))))?;

            let path = args.output.join(key);
            std::fs::write(path, file)?;
        }
    } else if args.remove {
        for file in args.files {
            let key = file.file_name();
            // key is the base filename + ext
            let key = key.unwrap().to_str().unwrap();

            png.remove_file(key);
        }

        std::fs::write(args.input, png.into_bytes())?;
    }

    Ok(())
}

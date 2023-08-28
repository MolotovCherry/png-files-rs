use std::{
    borrow::Cow,
    io::{Cursor, Read, Seek, Write},
    ops::Range,
    rc::Rc,
};

use bincode::{BorrowDecode, Encode};
use byteorder::{BigEndian, ReadBytesExt};
use flate2::{
    write::{DeflateDecoder, DeflateEncoder},
    Compression,
};

use crate::PngFilesError;

const PNG_HEADER: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

// Custom file chunk type
//
// fiLe
// 1101
// ||||
// |||+- Safe-to-copy bit is 1 (lowercase letter; bit 5 is 1)
// ||+-- Reserved bit is 0     (uppercase letter; bit 5 is 0)
// |+--- Private bit is 1      (lowercase letter; bit 5 is 1)
// +---- Ancillary bit is 1    (lowercase letter; bit 5 is 1)
const CHUNK_TYPE: &str = "fiLe";

// representing a file object inside the png file
#[derive(Debug, Encode, BorrowDecode)]
struct File<'a> {
    key: &'a str,
    data: Cow<'a, [u8]>,
}

impl File<'_> {
    // Decode data contained with deflate
    fn decode_data(&self) -> Result<Vec<u8>, PngFilesError> {
        let mut writer = DeflateDecoder::new(Vec::new());
        writer.write_all(&self.data)?;
        Ok(writer.finish()?)
    }
}

pub struct Png {
    chunks: Vec<PngChunk>,
    capacity: usize,
}

struct PngChunk {
    source: DataSource,
    chunk_type: ChunkType,
    crc: u32,
    len: u32,
}

impl PngChunk {
    /// return byte slice of png chunk's data
    fn as_data(&self) -> &[u8] {
        use DataSource::*;
        match &self.source {
            Range { data, range } => &data[range.clone()],
            Data(data) => data,
        }
    }
}

#[derive(PartialEq, Eq)]
enum ChunkType {
    Png(String),
    File { key: String },
}

impl ChunkType {
    /// Get the key for ChunkType::File
    fn get_key(&self) -> Option<&str> {
        match self {
            Self::File { key } => Some(key),
            _ => None,
        }
    }

    fn as_bytes(&self) -> &[u8] {
        self.as_ref().as_bytes()
    }
}

impl AsRef<str> for ChunkType {
    fn as_ref(&self) -> &str {
        match self {
            ChunkType::Png(_type) => _type,
            ChunkType::File { .. } => CHUNK_TYPE,
        }
    }
}

enum DataSource {
    Range {
        data: Rc<Vec<u8>>,
        range: Range<usize>,
    },

    Data(Vec<u8>),
}

impl PngChunk {
    // output a perfect representation of the chunk in binary
    pub fn into_bytes(self) -> Vec<u8> {
        let data = self.as_data();

        // 4 - len
        // 4 - chunk type
        // data len
        // 4 - crc
        let mut chunk: Vec<u8> = Vec::with_capacity(4 + 4 + data.len() + 4);
        // all numbers are BE
        // len
        chunk.extend_from_slice(&self.len.to_be_bytes());
        // chunk type
        chunk.extend_from_slice(self.chunk_type.as_bytes());
        // data
        chunk.extend_from_slice(data);
        // crc
        chunk.extend_from_slice(&self.crc.to_be_bytes());

        chunk
    }
}

impl Png {
    pub fn new(data: Vec<u8>) -> Result<Self, PngFilesError> {
        let file_len = data.len();
        let data = Rc::new(data);

        // enclose in scope to make sure borrow is dropped

        let mut cursor = Cursor::new(&**data);

        // validate header
        //
        // Refer to the PNG file spec
        // http://www.libpng.org/pub/png/spec/1.2/PNG-Structure.html
        //

        let mut buf_res = [0; PNG_HEADER.len()];
        cursor.read_exact(&mut buf_res)?;

        if PNG_HEADER != buf_res {
            Err(PngFilesError::Msg(Cow::Borrowed(
                "Input file is not PNG format",
            )))?;
        }

        let mut chunks = Vec::new();

        loop {
            if cursor.position() as usize >= file_len {
                break;
            }

            let len: usize = cursor
                .read_u32::<BigEndian>()
                .map_err(|_| PngFilesError::Msg(Cow::Borrowed("Failed to read len")))?
                .try_into()
                .map_err(|_| PngFilesError::Msg(Cow::Borrowed("Failed to convert len to usize")))?;

            let cur_pos: usize = cursor.position().try_into().map_err(|_| {
                PngFilesError::Msg(Cow::Borrowed("Failed to convert cursor pos to usize"))
            })?;

            // borrow slice of type + data for crc check later
            // chunk type - 4 bytes
            // data len - variable
            let crc_data =
                cursor
                    .get_ref()
                    .get(cur_pos..cur_pos + 4 + len)
                    .ok_or(PngFilesError::Msg(Cow::Borrowed(
                        "Invalid chunk (type or data missing)",
                    )))?;
            let data_crc = crc32fast::hash(crc_data);

            let mut chunk_type = [0; 4];
            cursor
                .read_exact(&mut chunk_type)
                .map_err(|_| PngFilesError::Msg(Cow::Borrowed("Failed to read chunk type")))?;
            let chunk_type = std::str::from_utf8(&chunk_type)
                .map_err(|_| PngFilesError::Msg(Cow::Borrowed("Invalid chunk type")))?;

            let range_pos: usize = cursor.position().try_into().map_err(|_| {
                PngFilesError::Msg(Cow::Borrowed("Failed to convert index to usize"))
            })?;
            let chunk_data = if chunk_type == CHUNK_TYPE {
                // if it's a data chunk we're interested in, save the data
                // slice the ref so we can borrow data instead of needing to allocate
                Some(
                    cursor
                        .get_ref()
                        .get(range_pos..range_pos + len)
                        .ok_or(PngFilesError::Msg(Cow::Borrowed("fiLe data not found")))?,
                )
            } else {
                None
            };

            // skip past data section since we didn't advance cursor before
            cursor.seek(std::io::SeekFrom::Current(len as i64))?;

            let crc = cursor
                .read_u32::<BigEndian>()
                .map_err(|_| PngFilesError::Msg(Cow::Borrowed("Failed to read crc")))?;

            // validate chunk, cause why not
            if data_crc != crc {
                Err(PngFilesError::Msg(Cow::Borrowed(
                    "Crc check failed; PNG file is corrupted",
                )))?;
            }

            chunks.push(if chunk_type == CHUNK_TYPE {
                // our special file chunk
                let chunk_data = chunk_data.unwrap();

                let file = Self::decode_file(chunk_data)?;

                PngChunk {
                    chunk_type: ChunkType::File {
                        key: file.key.to_owned(),
                    },

                    source: DataSource::Range {
                        data: data.clone(),
                        range: Range {
                            start: range_pos,
                            end: range_pos + len,
                        },
                    },

                    crc,
                    // this was originally u32, truncation is ok
                    len: len as u32,
                }
            } else {
                // regular chunk
                PngChunk {
                    chunk_type: ChunkType::Png(chunk_type.to_owned()),
                    source: DataSource::Range {
                        data: data.clone(),
                        range: Range {
                            start: range_pos,
                            end: range_pos + len,
                        },
                    },
                    crc,
                    // this was originally u32, truncation is ok
                    len: len as u32,
                }
            });
        }

        Ok(Self {
            chunks,
            capacity: file_len,
        })
    }

    /// Returns none if file failed to decode or was not found
    pub fn get_file(&self, key: &str) -> Option<Vec<u8>> {
        self.chunks
            .iter()
            .find(|&c| {
                let chunk_type = c.chunk_type.as_ref();

                if chunk_type == CHUNK_TYPE {
                    c.chunk_type.get_key().unwrap() == key
                } else {
                    false
                }
            })
            .and_then(|c| {
                Self::decode_file(c.as_data())
                    .ok()
                    .and_then(|f| f.decode_data().ok())
            })
    }

    // note: decoded file is NOT deflate decoded in order to allow for slice borrow
    fn decode_file(data: &[u8]) -> Result<File<'_>, PngFilesError> {
        let (file, _) =
            bincode::borrow_decode_from_slice::<File<'_>, _>(data, bincode::config::standard())?;

        Ok(file)
    }

    /// File data is encoded with deflate
    fn encode_file(mut file: File<'_>) -> Result<Vec<u8>, PngFilesError> {
        let mut deflater = DeflateEncoder::new(Vec::new(), Compression::best());
        deflater.write_all(&file.data)?;
        let data = deflater.finish()?;
        file.data = Cow::Owned(data);

        let data = bincode::encode_to_vec::<File, _>(file, bincode::config::standard())?;

        Ok(data)
    }

    /// Remove a file from png, returning whether one was removed or not
    pub fn remove_file(&mut self, key: &str) -> bool {
        let idx = self.chunks.iter().position(|c| {
            let chunk_type = c.chunk_type.as_ref();

            if chunk_type == CHUNK_TYPE {
                c.chunk_type.get_key().unwrap() == key
            } else {
                false
            }
        });

        if let Some(idx) = idx {
            self.chunks.remove(idx);
            true
        } else {
            false
        }
    }

    /// insert file chunk into PNG
    /// `replace` overwrites existing key if it exists
    pub fn insert_file(
        &mut self,
        key: &str,
        data: Vec<u8>,
        replace: bool,
    ) -> Result<(), PngFilesError> {
        // find existing item with key if it exists
        let idx = self.chunks.iter().position(|c| {
            let chunk_type = c.chunk_type.as_ref();

            if chunk_type == CHUNK_TYPE {
                c.chunk_type.get_key().unwrap() == key
            } else {
                false
            }
        });

        // check that no key already exists in data
        if !replace && idx.is_some() {
            Err(PngFilesError::Msg(Cow::Borrowed("Key already in use")))?;
        }

        let file = File {
            key,
            data: Cow::Borrowed(&data),
        };

        let data = Self::encode_file(file)?;

        // calculate crc from chunk type first THEN data
        let mut h = crc32fast::Hasher::new();
        h.update(CHUNK_TYPE.as_bytes());
        h.update(&data);
        let crc = h.finalize();

        let len = data.len();

        if len > u32::MAX as usize {
            Err(PngFilesError::Msg(Cow::Borrowed(
                "Data cannot be bigger than u32::MAX bytes",
            )))?;
        }

        let chunk = PngChunk {
            source: DataSource::Data(data),
            chunk_type: ChunkType::File {
                key: key.to_owned(),
            },
            crc,
            len: len as u32,
        };

        // either insert or replace already existing key
        if idx.is_none() {
            self.chunks.push(chunk);
        } else if let Some(idx) = idx {
            let _ = std::mem::replace(&mut self.chunks[idx], chunk);
        }

        Ok(())
    }

    pub fn into_bytes(self) -> Vec<u8> {
        // the capacity could be more, but at a minimum
        let mut bytes = Vec::with_capacity(self.capacity);

        bytes.extend_from_slice(PNG_HEADER);
        for chunk in self.chunks {
            bytes.extend(chunk.into_bytes());
        }

        bytes
    }
}

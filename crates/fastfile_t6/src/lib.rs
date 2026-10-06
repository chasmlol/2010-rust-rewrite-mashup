#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod asset_type;
mod cipher;
mod content;
mod envelope;
pub mod light_grid;
mod salsa20;
pub mod schema;
mod walk;
pub mod weapon;
pub mod world;

pub use asset_type::AssetType;
pub use cipher::{STREAM_COUNT, ZoneCipher};
pub use content::{XFILE_HEADER_LEN, ZoneHeader};
pub use envelope::{
    CHUNKS_OFFSET, Chunk, Chunks, FileHeader, FileHeaderError, MAGIC_ENCRYPTED, MAGIC_SIGNED,
    MAX_XFILE_COUNT, ZONE_NAME_LEN, ZONE_VERSION_PC, parse_file_header,
};
pub use walk::{
    LoadedAsset, Ptr, WalkError, XFILE_BLOCK_TEMP, XFILE_BLOCK_VIRTUAL, ZoneBlocks, ZoneLoad,
    load_zone,
};

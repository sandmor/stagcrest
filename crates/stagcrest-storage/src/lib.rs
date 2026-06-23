pub mod compress;
pub mod format;
pub mod port;

#[cfg(not(target_arch = "wasm32"))]
pub mod redb;

pub mod null;

pub use compress::{compress_stored, decompress_stored};
pub use format::{InactiveChunk, InactiveChunkReader, StorageFormatError};
pub use port::{
    encode_chunk_key, load_inactive_chunk, store_inactive_chunk, ChunkStorage, StorageError,
};
pub use null::NullChunkStorage;

#[cfg(not(target_arch = "wasm32"))]
pub use redb::RedbChunkStorage;

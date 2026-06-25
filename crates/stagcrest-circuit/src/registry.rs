use stagcrest_protocol::BlockPos;

pub trait PowerLookup: Sync {
    fn power_at(&self, pos: BlockPos) -> u8;
}

pub use stagcrest_mod_server::BlockRegistry;

#[cfg(test)]
mod persistence_tests {
    use stagcrest_protocol::{BlockId, BlockPos, BlockState, ChunkPos, CHUNK_SIZE};
    use stagcrest_storage::{ChunkStorage, RedbChunkStorage};
    use stagcrest_world::World;

    #[test]
    fn native_redb_persist_reload_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("stagcrest_persist_test_{}", std::process::id()));
        let _ = std::fs::remove_file(dir.join("world.redb"));
        let storage = RedbChunkStorage::open(dir.join("world.redb")).unwrap();

        let air = BlockId(0);
        let stone = BlockId(1);
        let mut world = World::with_lru_capacity(64, air);
        let pos = ChunkPos { x: 2, y: 0, z: -3 };

        world.set_blocks([(BlockPos::new(32, 0, -48), stone, BlockState(0))]);
        world.finalize_generated_chunk(pos);
        world.mark_chunk_generated(pos);

        let evicted = world.unload_far_chunks_3d(ChunkPos { x: 0, y: 0, z: 0 }, 0, 0);
        assert_eq!(evicted.0.len(), 1);
        world
            .persist_and_evict(&storage, evicted.0.into_iter().next().unwrap())
            .unwrap();
        assert!(!world.has_chunk(pos));
        assert!(storage.contains(pos));

        world
            .load_area_from_storage(&storage, pos, 0, 0, 0..=0)
            .unwrap();
        assert!(world.has_chunk(pos));
        assert_eq!(world.get_block(BlockPos::new(32, 0, -48)).0, stone);
    }

    #[test]
    fn inactive_chunk_meshes_without_active_promotion() {
        let air = BlockId(0);
        let stone = BlockId(1);
        let mut world = World::with_lru_capacity(64, air);
        let pos = ChunkPos { x: 0, y: 0, z: 0 };

        let blocks: Vec<_> = (0..CHUNK_SIZE)
            .map(|x| (BlockPos::new(x, 0, 0), stone, BlockState(0)))
            .collect();
        world.set_blocks(blocks);
        world.finalize_generated_chunk(pos);

        assert!(world.chunk(pos).is_none());
        assert!(world.chunk_view(pos).is_some());
        assert_eq!(world.get_block(BlockPos::new(0, 0, 0)).0, stone);

        world.set_block(BlockPos::new(0, 0, 0), air, BlockState(0));
        assert!(world.chunk(pos).is_some());
    }
}

use stagcrest_protocol::BlockPos;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitEvent {
    Evaluate(BlockPos),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledEval {
    pub fire_tick: u64,
    pub seq: u64,
    pub pos: BlockPos,
    pub output: u8,
}

#[derive(Debug, Clone, Default)]
pub struct EventQueue {
    pub events: VecDeque<CircuitEvent>,
    pub pending: HashSet<BlockPos>,
    pending_delays: HashMap<BlockPos, ScheduledEval>,
    next_seq: u64,
}

impl EventQueue {
    pub fn enqueue_evaluate(&mut self, pos: BlockPos) {
        if self.pending.insert(pos) {
            self.events.push_back(CircuitEvent::Evaluate(pos));
        }
    }

    pub fn dequeue(&mut self) -> Option<CircuitEvent> {
        let event = self.events.pop_front()?;
        let CircuitEvent::Evaluate(pos) = event;
        self.pending.remove(&pos);
        Some(event)
    }

    pub fn schedule_delay(&mut self, fire_tick: u64, pos: BlockPos, output: u8) {
        self.schedule_delay_with_seq(fire_tick, pos, output, None);
    }

    pub fn schedule_delay_with_seq(
        &mut self,
        fire_tick: u64,
        pos: BlockPos,
        output: u8,
        seq: Option<u64>,
    ) {
        let seq = seq.unwrap_or_else(|| {
            let next = self.next_seq;
            self.next_seq = self.next_seq.saturating_add(1);
            next
        });
        self.pending_delays.insert(
            pos,
            ScheduledEval {
                fire_tick,
                seq,
                pos,
                output,
            },
        );
    }

    pub fn cancel_delay(&mut self, pos: BlockPos) {
        self.pending_delays.remove(&pos);
    }

    pub fn take_delay(&mut self, pos: BlockPos) -> Option<ScheduledEval> {
        self.pending_delays.remove(&pos)
    }

    pub fn has_pending_delay(&self, pos: BlockPos) -> bool {
        self.pending_delays.contains_key(&pos)
    }

    pub fn drain_due_delays(&mut self, tick: u64) -> Vec<ScheduledEval> {
        let due: Vec<BlockPos> = self
            .pending_delays
            .iter()
            .filter(|(_, eval)| eval.fire_tick <= tick)
            .map(|(pos, _)| *pos)
            .collect();

        let mut drained: Vec<ScheduledEval> = due
            .into_iter()
            .filter_map(|pos| self.pending_delays.remove(&pos))
            .collect();
        drained.sort_by_key(|eval| (eval.seq, eval.pos.x, eval.pos.y, eval.pos.z));
        drained
    }

    pub fn pending_delays_in_chunk(
        &self,
        chunk: stagcrest_protocol::ChunkPos,
    ) -> Vec<ScheduledEval> {
        self.pending_delays
            .values()
            .filter(|eval| eval.pos.chunk_pos() == chunk)
            .copied()
            .collect()
    }
}

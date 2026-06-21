use stagcrest_protocol::BlockPos;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitEvent {
    Evaluate(BlockPos),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledEval {
    pub fire_tick: u64,
    pub pos: BlockPos,
    pub output: u8,
}

#[derive(Debug, Clone, Default)]
pub struct EventQueue {
    pub events: VecDeque<CircuitEvent>,
    pub pending: HashSet<BlockPos>,
    pending_delays: HashMap<BlockPos, ScheduledEval>,
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
        self.pending_delays.insert(
            pos,
            ScheduledEval {
                fire_tick,
                pos,
                output,
            },
        );
    }

    pub fn cancel_delay(&mut self, pos: BlockPos) {
        self.pending_delays.remove(&pos);
    }

    pub fn drain_due_delays(&mut self, tick: u64) -> Vec<ScheduledEval> {
        let due: Vec<BlockPos> = self
            .pending_delays
            .iter()
            .filter(|(_, eval)| eval.fire_tick <= tick)
            .map(|(pos, _)| *pos)
            .collect();

        due.into_iter()
            .filter_map(|pos| self.pending_delays.remove(&pos))
            .collect()
    }
}

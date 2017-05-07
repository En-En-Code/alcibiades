//! Implements `StdHashTableEntry`.

use value::*;
use depth::*;
use hash_table::*;
use moves::MoveDigest;


/// Implements the `HashTableEntry` trait.
#[derive(Copy, Clone, Debug)]
pub struct StdHashTableEntry {
    value: Value,
    bound: BoundType,
    depth: Depth,
    move_digest: MoveDigest,
    static_eval: Value,
}

impl HashTableEntry for StdHashTableEntry {
    #[inline]
    fn new(value: Value, bound: BoundType, depth: Depth) -> StdHashTableEntry {
        debug_assert!(VALUE_MIN <= value && value <= VALUE_MAX);
        debug_assert!(bound <= 0b11);
        debug_assert!(DEPTH_MIN <= depth && depth <= DEPTH_MAX);
        StdHashTableEntry {
            value: value,
            bound: bound,
            depth: depth,
            move_digest: MoveDigest::invalid(),
            static_eval: VALUE_UNKNOWN,
        }
    }

    #[inline]
    fn value(&self) -> Value {
        self.value
    }

    #[inline]
    fn bound(&self) -> BoundType {
        self.bound
    }

    #[inline]
    fn depth(&self) -> Depth {
        self.depth
    }

    #[inline]
    fn move_digest(&self) -> MoveDigest {
        self.move_digest
    }

    #[inline]
    fn static_eval(&self) -> Value {
        self.static_eval
    }

    #[inline]
    fn set_move_digest(self, move_digest: MoveDigest) -> Self {
        Self {
            move_digest: move_digest,
            ..self
        }
    }

    /// Consumes the instance and returns a new instance with updated
    /// static evaluation.
    #[inline]
    fn set_static_eval(self, static_eval: Value) -> Self {
        Self {
            static_eval: static_eval,
            ..self
        }
    }
}

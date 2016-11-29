//! Implements various static position evaluators.

use std::hash::{Hasher, SipHasher};
use chesstypes::*;
use uci::SetOption;
use super::{PositionBoard, BoardEvaluator};
use super::bitsets::*;


/// A simple evaluator that considers only the material available on
/// the board.
#[derive(Clone)]
pub struct MaterialEvaluator;

impl SetOption for MaterialEvaluator {}

impl BoardEvaluator for MaterialEvaluator {
    #[allow(unused_variables)]
    fn new(board: &PositionBoard<MaterialEvaluator>) -> MaterialEvaluator {
        MaterialEvaluator
    }

    #[allow(unused_variables)]
    fn evaluate(&self, board: &PositionBoard<MaterialEvaluator>, halfmove_clock: u8) -> Value {
        use board::bitsets::*;
        const PIECE_VALUES: [Value; 8] = [10000, 975, 500, 325, 325, 100, 0, 0];
        let piece_type = board.pieces().piece_type;
        let color = board.pieces().color;
        let us = board.to_move();
        let them = 1 ^ us;
        let mut result = 0;
        for piece in QUEEN..NO_PIECE {
            result += PIECE_VALUES[piece] *
                      (pop_count(piece_type[piece] & color[us]) as i16 -
                       pop_count(piece_type[piece] & color[them]) as i16);
        }
        result
    }

    #[inline]
    fn is_zugzwangy(&self) -> bool {
        false
    }
}


/// A simple evaluator that adds a random number to the available
/// material.
#[derive(Clone)]
pub struct RandomEvaluator;

impl SetOption for RandomEvaluator {}

impl BoardEvaluator for RandomEvaluator {
    #[allow(unused_variables)]
    fn new(board: &PositionBoard<RandomEvaluator>) -> RandomEvaluator {
        RandomEvaluator
    }

    #[allow(unused_variables)]
    fn evaluate(&self, board: &PositionBoard<RandomEvaluator>, halfmove_clock: u8) -> Value {
        const PIECE_VALUES: [Value; 8] = [10000, 975, 500, 325, 325, 100, 0, 0];
        let piece_type = board.pieces().piece_type;
        let color = board.pieces().color;
        let us = board.to_move();
        let them = 1 ^ us;
        let mut result = 0;
        for piece in QUEEN..NO_PIECE {
            result += PIECE_VALUES[piece] *
                      (pop_count(piece_type[piece] & color[us]) as i16 -
                       pop_count(piece_type[piece] & color[them]) as i16);
        }
        let mut hasher = SipHasher::new();
        hasher.write_u64(board.occupied());
        result + (hasher.finish() >> 59) as i16
    }

    #[inline]
    fn is_zugzwangy(&self) -> bool {
        false
    }
}

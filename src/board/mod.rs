//! Facilities for implementing static position evaluation and move
//! generation.
//!
//! # Static position evaluation
//!
//! An evaluation function is used to heuristically determine the
//! relative value of a position, i.e. the chances of winning. If we
//! could see to the end of the game in every line, the evaluation
//! would only have values of "loss", "draw", and "win". In practice,
//! however, we do not know the exact value of a position, so we must
//! make an approximation. Beginning chess players learn to do this
//! starting with the value of the pieces themselves. Computer
//! evaluation functions also use the value of the material as the
//! most significant aspect and then add other considerations.
//!
//! Static evaluation is an evaluation that considers only the static
//! material and positional properties of the current position,
//! without analyzing any tactical variations. Therefore, if the
//! position has pending tactical threats, the static evaluation will
//! be grossly incorrect.
//!
//! Writing a new static evaluator is as simple as defining a type
//! that implements the `BoardEvaluator` trait.
//!
//! # Move generation
//!
//! Generation of moves is at the heart of every chess
//! engine. `Generator` provides a very fast move generator.  Writing
//! a good move generator is not easy. Nevertheless, if you decide to
//! do so, you should define your own type that implements the
//! `MoveGenerator` trait.

pub mod tables;
pub mod bitsets;
pub mod evaluators;
pub mod notation;
mod generator;

use std::mem::uninitialized;
use std::cmp::max;
use chesstypes::*;
use uci::SetOption;
use self::bitsets::*;
use self::notation::*;
use self::tables::*;

pub use self::generator::Generator;


/// Holds a chess position.
#[derive(Clone)]
pub struct Board {
    /// The placement of the pieces on the board.
    pub pieces: PiecesPlacement,

    /// The side to move.
    pub to_move: Color,

    /// The castling rights for both players.
    pub castling_rights: CastlingRights,

    /// If the previous move was a double pawn push, contains pushed
    /// pawn's file (a value between 0 and 7). Otherwise contains `8`.
    pub enpassant_file: usize,

    /// The set of all occupied squares on the board.
    ///
    /// Always equals `self.pieces.color[WHITE] |
    /// self.pieces.color[BLACK]`. Deserves a field on its own because
    /// it is very frequently needed.
    pub occupied: Bitboard,
}

impl Board {
    /// Creates a new instance from a Forsyth–Edwards Notation (FEN)
    /// string.
    pub fn from_fen(fen: &str) -> Result<Board, NotationError> {
        parse_fen(fen).map(|x| x.0)
    }
}


/// A trait for adding moves to move containers.
pub trait AddMove {
    /// Adds a move to the move container.
    fn add_move(&mut self, m: Move);
}

impl AddMove for Vec<Move> {
    #[inline(always)]
    fn add_move(&mut self, m: Move) {
        self.push(m);
    }
}


/// A trait used to statically evaluate positions.
pub trait BoardEvaluator: Clone + Send + SetOption {
    /// Creates a new instance and binds it to a given position.
    ///
    /// When a new instance is created, it is bound to a particular
    /// chess position (given by the `board` parameter). And for a
    /// moment, this is the only position that can be correctly
    /// evaluated. The instance then can be re-bound to the next (or
    /// the previous) position in the line of play by issuing calls to
    /// `will_do_move` and `done_move` methods (or respectively,
    /// `will_undo_move` and `undone_move` methods) .
    fn new(board: &Board) -> Self;

    /// Evaluates the the position to which the instance is bound.
    ///
    /// `board` points to the position to which the instance is bound.
    /// `halfmove_clock` gives the number of half-moves since the last
    /// piece capture or pawn advance.
    ///
    /// The returned value must be between `VALUE_EVAL_MIN` and
    /// `VALUE_EVAL_MAX`.
    fn evaluate(&self, board: &Board, halfmove_clock: u8) -> Value;

    /// Returns whether the position is zugzwangy.
    ///
    /// In many endgame positions there is a relatively high
    /// probability of zugzwang occurring. For such positions, this
    /// method returns `true`.
    fn is_zugzwangy(&self, board: &Board, halfmove_clock: u8) -> bool;

    /// Updates evaluator's state to keep up with a move that will be
    /// played.
    ///
    /// `board` points to the position to which the instance is bound.
    ///
    /// `m` is a legal move, or (if not in check) a "null move".
    #[inline]
    #[allow(unused_variables)]
    fn will_do_move(&mut self, board: &Board, m: Move) {}

    /// Updates evaluator's state to keep up with a move that was
    /// played.
    ///
    /// `board` points to the new position to which the instance is
    /// bound.
    #[inline]
    #[allow(unused_variables)]
    fn done_move(&mut self, board: &Board, m: Move) {}

    /// Updates evaluator's state to keep up with a move that will be
    /// taken back.
    ///
    /// `board` points to the position to which the instance is bound.
    #[inline]
    #[allow(unused_variables)]
    fn will_undo_move(&mut self, board: &Board, m: Move) {}

    /// Updates evaluator's state in accordance with a move that was
    /// taken back.
    ///
    /// `board` points to the new position to which the instance is
    /// bound.
    #[inline]
    #[allow(unused_variables)]
    fn undone_move(&mut self, board: &Board, m: Move) {}
}


/// A trait for move generators.
///
/// A `MoveGenerator` holds a chess position, can generate all legal
/// moves, play a selected move and take it back. It provides a
/// position evaluator, and can calculate Zobrist hashes.
///
/// **Important note:** `MoveGenerator` is unaware of repeating
/// positions and the fifty-move rule.
pub trait MoveGenerator: Sized + Send + Clone + SetOption {
    /// The type of static evaluator that the implementation works
    /// with.
    type BoardEvaluator: BoardEvaluator;

    /// Creates a new instance, consuming the supplied `Board`
    /// instance.
    ///
    /// Returns `None` if the position is illegal.
    fn from_board(board: Board) -> Option<Self>;

    /// Returns the Zobrist hash value for the underlying `Board`
    /// instance.
    ///
    /// Zobrist hashing is a technique to transform a board position
    /// into a number of a fixed length, with an equal distribution
    /// over all possible numbers, invented by Albert Zobrist. The key
    /// property of this method is that two similar positions generate
    /// entirely different hash numbers.
    ///
    /// **Important note:** This method calculates the hash value
    /// "from scratch", which can be too slow for some use cases. (See
    /// `do_move`.)
    fn hash(&self) -> u64;

    /// Returns a reference to the underlying `Board` instance.
    #[inline(always)]
    fn board(&self) -> &Board;

    /// Returns a bitboard with all pieces of color `us` that attack
    /// `square`.
    fn attacks_to(&self, us: Color, square: Square) -> Bitboard;

    /// Returns a bitboard with all enemy pieces that attack the king.
    ///
    /// **Important note:** The bitboard of checkers is calculated on
    /// the first call to `checkers`, and is stored in case another
    /// call is made before doing/undoing any moves. In that case
    /// `checkers` returns the saved bitboard instead of
    /// re-calculating it, thus saving time.
    fn checkers(&self) -> Bitboard;

    /// Returns a reference to a static evaluator bound to the current
    /// position.
    fn evaluator(&self) -> &Self::BoardEvaluator;

    /// Generates all legal moves, possibly including some
    /// pseudo-legal moves too.
    ///
    /// The moves are added to `moves`. All generated moves with
    /// pieces other than the king will be legal. Some of the
    /// generated king's moves may be illegal because the destination
    /// square is under attack. This arrangement has two important
    /// advantages:
    ///
    /// * `do_move` can do its work without knowing the set of
    ///   checkers and pinned pieces, so there is no need to keep
    ///   those around.
    ///
    /// * A beta cut-off may make the verification that king's
    ///   destination square is not under attack unnecessary, thus
    ///   saving time.
    ///
    /// **Note:** A pseudo-legal move is a move that is otherwise
    /// legal, except it might leave the king in check.
    fn generate_all<T: AddMove>(&self, moves: &mut T);

    /// Generates moves for the quiescence search.
    ///
    /// The moves are added to `moves`. This method always generates a
    /// **subset** of the moves generated by `generate_all`:
    ///
    /// * If the king is in check, all legal moves are included.
    ///
    /// * Captures and pawn promotions to queen are always included.
    ///
    /// * If `generate_checks` is `true`, moves that give check are
    ///   included too. Discovered checks and checks given by castling
    ///   can be omitted for speed.
    fn generate_forcing<T: AddMove>(&self, generate_checks: bool, moves: &mut T);

    /// Checks if `move_digest` represents a pseudo-legal move.
    ///
    /// If a move `m` exists that would be generated by
    /// `generate_all` if called for the current position on the
    /// board, and for that move `m.digest() == move_digest`, this
    /// method will return `Some(m)`. Otherwise it will return
    /// `None`. This is useful when playing moves from the
    /// transposition table, without calling `generate_all`.
    fn try_move_digest(&self, move_digest: MoveDigest) -> Option<Move>;

    /// Returns a null move.
    ///
    /// "Null move" is a pseudo-move that changes only the side to
    /// move. It is sometimes useful to include a speculative null
    /// move in the search tree so as to achieve more aggressive
    /// pruning. Null moves are represented as king's moves for which
    /// the origin and destination squares are the same.
    fn null_move(&self) -> Move;

    /// Plays a move on the board.
    ///
    /// It verifies if the move is legal. If the move is legal, the
    /// board is updated and an `u64` value is returned, which should
    /// be XOR-ed with old board's hash value to obtain new board's
    /// hash value. If the move is illegal, `None` is returned without
    /// updating the board. The move passed to this method **must**
    /// have been generated by `generate_all`, `generate_forcing`,
    /// `try_move_digest`, or `null_move` methods for the current
    /// position on the board.
    ///
    /// The moves generated by the `null_move` method are
    /// exceptions. For them `do_move` will return `None` if and only
    /// if the king is in check.
    fn do_move(&mut self, m: Move) -> Option<u64>;

    /// Takes back last played move.
    ///
    /// The move passed to this method **must** be the last move passed
    /// to `do_move`.
    fn undo_move(&mut self, m: Move);

    /// Calculates the static exchange evaluation (SEE) value for a
    /// move.
    ///
    /// This method returns the likely evaluation change (material) to
    /// be lost or gained as a result of a given move. It examines the
    /// consequence of a series of exchanges on the destination square
    /// after a given move. The result is calculated without actually
    /// doing any moves on the board.
    fn calc_see(&self, m: Move) -> Value {
        debug_assert!(m.played_piece() < NO_PIECE);
        debug_assert!(m.captured_piece() <= NO_PIECE);
        const PIECE_VALUES: [Value; 7] = [10000, 975, 500, 325, 325, 100, 0];

        let dest_square = m.dest_square();  // the exchange square
        let occupied = self.board().occupied;
        let geometry = BoardGeometry::get();
        let behind_blocker: &[Bitboard; 64] = &geometry.squares_behind_blocker[dest_square];
        let piece_type: &[Bitboard; 6] = &self.board().pieces.piece_type;
        let color: &[Bitboard; 2] = &self.board().pieces.color;

        // These variables will be updated on each capture:
        let mut us = self.board().to_move;
        let mut depth = 0;
        let mut piece = m.played_piece();
        let mut orig_square_bb = 1 << m.orig_square();
        let mut attackers_and_defenders = self.attacks_to(WHITE, dest_square) |
                                          self.attacks_to(BLACK, dest_square);

        // `may_xray` holds the set of pieces that may block attacks
        // from other pieces, and therefore we must consider adding
        // new attackers/defenders every time a piece from the
        // `may_xray` set makes a capture.
        let may_xray = piece_type[PAWN] | piece_type[BISHOP] | piece_type[ROOK] | piece_type[QUEEN];

        // The `gain` array will hold the total material gained at
        // each `depth`, from the viewpoint of the side that made the
        // last capture (`us`).
        let mut gain: [Value; 34] = unsafe { uninitialized() };
        let captured_piece_value = PIECE_VALUES[m.captured_piece()];
        gain[depth] = if m.move_type() == MOVE_PROMOTION {
            // Adding `1` guarantees that SEE will be greater than
            // zero if the promoted pawn is protected.
            captured_piece_value + 1
        } else {
            captured_piece_value
        };

        // Examine the possible exchanges, fill the `gain` array.
        'exchange: while orig_square_bb != 0 {
            // Store a speculative value that will be used if the
            // captured piece happens to be defended.
            gain[depth + 1] = PIECE_VALUES[piece] - gain[depth];

            if max(-gain[depth], gain[depth + 1]) < 0 {
                // The side that made the last capture wins even if
                // the captured piece happens to be defended. This is
                // good enough for our purposes, so we stop here.
                break;
            }

            // Register that `orig_square_bb` is now vacant.
            attackers_and_defenders &= !orig_square_bb;

            // Consider adding new attackers/defenders, now that
            // `orig_square_bb` is vacant.
            if orig_square_bb & may_xray != 0 {
                attackers_and_defenders |= {
                    let candidates = occupied & behind_blocker[bitscan_forward(orig_square_bb)];
                    let bb = geometry.attacks_from(ROOK, dest_square, candidates) & candidates &
                             (piece_type[QUEEN] | piece_type[ROOK]);
                    if bb != 0 {
                        // a straight slider
                        bb
                    } else {
                        // a diagonal slider
                        geometry.attacks_from(BISHOP, dest_square, candidates) & candidates &
                        (piece_type[QUEEN] | piece_type[BISHOP])
                    }
                };
            }

            // Change the side to move.
            us ^= 1;

            // Find the next piece to enter the exchange. (The least
            // valuable piece belonging to the side to move.)
            let candidates = attackers_and_defenders & color[us];
            for p in (KING..NO_PIECE).rev() {
                let bb = candidates & piece_type[p];
                if bb != 0 {
                    depth += 1;
                    piece = p;
                    orig_square_bb = ls1b(bb);
                    continue 'exchange;
                }
            }
            break 'exchange;
        }

        // Negamax the `gain` array for the final static exchange
        // evaluation. (The `gain` array actually represents an unary
        // tree, at each node of which the player can either continue
        // the exchange or back off.)
        unsafe {
            while depth > 0 {
                *gain.get_unchecked_mut(depth - 1) = -max(-*gain.get_unchecked(depth - 1),
                                                          *gain.get_unchecked(depth));
                depth -= 1;
            }
        }
        gain[0]
    }
}

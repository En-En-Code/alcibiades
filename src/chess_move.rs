//! Defines data structures related to chess moves.

use std;
use std::slice;
use basetypes::*;
use castling_rights::*;


/// Encodes the minimum needed information that unambiguously
/// describes a move.
///
/// It is laid out the following way:
///
///  ```text
///   15                                                           0
///  +---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+
///  |   |   |   |   |   |   |   |   |   |   |   |   |   |   |   |   |
///  | Move  |    Origin square      |   Destination square  | Aux   |
///  | type  |       6 bits          |        6 bits         | data  |
///  | 2 bits|   |   |   |   |   |   |   |   |   |   |   |   | 2 bits|
///  |   |   |   |   |   |   |   |   |   |   |   |   |   |   |       |
///  +---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+
///  ```
///  
/// There are 4 "move type"s: `0`) en-passant capture; `1`) pawn
/// promotion; `2`) castling; `3`) normal move. "Aux data" encodes the
/// type of the promoted piece if the move type is pawn promotion,
/// otherwise it is zero.  For valid moves the digest value will never
/// be `0`.
pub type MoveDigest = u16;


/// Represents a move on the chessboard.
///
/// `Move` is a `u64` number. It contains 3 types of information:
///
/// 1. Information about the played move itself.
///
/// 2. Information needed so as to be able to undo the move and
///    restore the board into the exact same state as before.
///
/// 3. Move ordering info -- moves with higher value are tried first.
///
/// Bits 0-15 contain the whole information about the move itself
/// (type 1). This is called **"move digest"** and is laid out the
/// following way:
///
///  ```text
///   15                                                           0
///  +---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+
///  |   |   |   |   |   |   |   |   |   |   |   |   |   |   |   |   |
///  | Move  |    Origin square      |   Destination square  | Aux   |
///  | type  |       6 bits          |        6 bits         | data  |
///  | 2 bits|   |   |   |   |   |   |   |   |   |   |   |   | 2 bits|
///  |   |   |   |   |   |   |   |   |   |   |   |   |   |   |       |
///  +---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+
///  ```
///
/// There are 4 "move type"s: `0`) en-passant capture; `1`) pawn
/// promotion; `2`) castling; `3`) normal move. "Aux data" encodes the
/// type of the promoted piece if the move type is pawn promotion,
/// otherwise it is zero.
///
/// Bits 16-31 contain the information needed to undo the move:
///
///  ```text
///   31                                                          16
///  +---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+
///  |   |   |   |   |   |   |   |   |   |   |   |   |   |   |   |   |
///  |   |   |  Captured |  Played   |   Castling    |   En-passant  |
///  | 0 | 0 |  piece    |  piece    |    rights     |      file     |
///  |   |   |  3 bits   |  3 bits   |    4 bits     |     4 bits    |
///  |   |   |   |   |   |   |   |   |   |   |   |       |   |   |   |   |
///  +---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+
///  ```
///
/// "En-passant file" tells on what vertical line on the board there
/// was a passing pawn before the move was played (a value between 0
/// and 7). If there was no passing pawn, "en-passant file" will be
/// between 8 and 15. "Castling rights" holds the castling rights
/// before the move was played. When "Captured piece" is stored, its
/// bits are inverted, so that MVV-LVA (Most valuable victim -- least
/// valuable aggressor) ordering of the moves is preserved, even when
/// the other fields stay the same.
///
/// Bits 32-63 contain the move ordering info (the "move score"):
///
///  ```text
///  63                                                            32
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///  | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | |
///  |                          Move score                           |
///  |                           32 bits                             |
///  | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | |
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///  ```
///
/// The "Move score" field (32-bits) is used to influence move
/// ordering. Ideally the best move should have the highest vaule.
#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialOrd, Ord, PartialEq, Eq)]
pub struct Move(u64);


impl Move {
    /// Creates a new instance.
    ///
    /// `us` is the side that makes the move. `castling` are the
    /// castling rights before the move was played. `en_passant_file`
    /// is the file on which there were a passing pawn before the move
    /// was played (a value between 0 and 7), or a value between 8 and
    /// 15 if there was no passing pawn. `promoted_piece_code` should
    /// be a number between 0 and 3 and is used only when the
    /// `move_type` is a pawn promotion, otherwise it is ignored.
    ///
    /// The initial move score for the new move will be:
    ///
    /// * `MOVE_SCORE_MAX - 1` for pawn promotions to pieces other
    ///   than queen.
    ///
    /// * `MOVE_SCORE_MAX` for captures and pawn promotions to queen.
    /// 
    /// * `0` for all other moves.
    #[inline(always)]
    pub fn new(us: Color,
               move_type: MoveType,
               piece: PieceType,
               orig_square: Square,
               dest_square: Square,
               captured_piece: PieceType,
               en_passant_file: usize,
               castling: CastlingRights,
               promoted_piece_code: usize)
               -> Move {
        assert!(us <= 1);
        assert!(move_type <= 0x11);
        assert!(piece < NO_PIECE);
        assert!(orig_square <= 63);
        assert!(dest_square <= 63);
        assert!(captured_piece != KING && captured_piece <= NO_PIECE);
        assert!(en_passant_file <= 0b1111);
        assert!(promoted_piece_code <= 0b11);

        // Captures are treated differently than quiet moves.
        let mut score_shifted = if captured_piece == NO_PIECE {
            0 << 32
        } else {
            (MOVE_SCORE_MAX as u64) << 32
        };

        // Figure out what `aux_data` should contain. In the mean
        // time, make sure pawn promotions get appropriate move
        // scores.
        let aux_data = if move_type == MOVE_PROMOTION {
            score_shifted = if promoted_piece_code == 0 {
                (MOVE_SCORE_MAX as u64) << 32
            } else {
                ((MOVE_SCORE_MAX - 1) as u64) << 32
            };
            promoted_piece_code
        } else {
            0
        };

        Move(score_shifted |
             ((!captured_piece & 0b111) << M_SHIFT_CAPTURED_PIECE | piece << M_SHIFT_PIECE |
              castling.value() << M_SHIFT_CASTLING_DATA |
              en_passant_file << M_SHIFT_ENPASSANT_FILE |
              move_type << M_SHIFT_MOVE_TYPE | orig_square << M_SHIFT_ORIG_SQUARE |
              dest_square << M_SHIFT_DEST_SQUARE |
              aux_data << M_SHIFT_AUX_DATA) as u64)
    }

    /// Creates a new instance with all fields set to `0` (an invalid
    /// move).
    #[inline(always)]
    pub fn invalid() -> Move {
        Move(0)
    }

    /// Assigns a new score for the move (between 0 and `MOVE_SCORE_MAX`).
    #[inline(always)]
    pub fn set_score(&mut self, score: u32) {
        self.0 &= (!0u32) as u64;
        self.0 |= (score as u64) << 32;
    }

    /// Returns the assigned move score.
    #[inline(always)]
    pub fn score(&self) -> u32 {
        (self.0 >> 32) as u32
    }

    /// Returns the move type.
    #[inline(always)]
    pub fn move_type(&self) -> MoveType {
        (self.0 as usize & M_MASK_MOVE_TYPE) >> M_SHIFT_MOVE_TYPE
    }

    /// Returns the played piece type.
    ///
    /// Castling is considered as a king's move.
    #[inline(always)]
    pub fn piece(&self) -> PieceType {
        (self.0 as usize & M_MASK_PIECE) >> M_SHIFT_PIECE
    }

    /// Returns the origin square of the played piece.
    #[inline(always)]
    pub fn orig_square(&self) -> Square {
        (self.0 as usize & M_MASK_ORIG_SQUARE) >> M_SHIFT_ORIG_SQUARE
    }

    /// Returns the destination square for the played piece.
    #[inline(always)]
    pub fn dest_square(&self) -> Square {
        (self.0 as usize & M_MASK_DEST_SQUARE) >> M_SHIFT_DEST_SQUARE
    }

    /// Returns the captured piece type.
    #[inline(always)]
    pub fn captured_piece(&self) -> PieceType {
        (!(self.0 as usize) & M_MASK_CAPTURED_PIECE) >> M_SHIFT_CAPTURED_PIECE
    }

    /// Returns the file on which there were a passing pawn before the
    /// move was played (a value between 0 and 7), or a value between
    /// 8 and 15 if there was no passing pawn.
    #[inline(always)]
    pub fn en_passant_file(&self) -> usize {
        (self.0 as usize & M_MASK_ENPASSANT_FILE) >> M_SHIFT_ENPASSANT_FILE
    }

    /// Returns the castling rights as they were before the move was
    /// played.
    #[inline(always)]
    pub fn castling(&self) -> CastlingRights {
        CastlingRights::new((self.0 as usize) >> M_SHIFT_CASTLING_DATA)
    }

    /// Returns a value between 0 and 3 representing the auxiliary
    /// data.
    ///
    /// When the move type is pawn promotion, "aux data" encodes the
    /// promoted piece type. For all other move types "aux data" is
    /// zero.
    #[inline(always)]
    pub fn aux_data(&self) -> usize {
        (self.0 as usize & M_MASK_AUX_DATA) >> M_SHIFT_AUX_DATA
    }

    /// Returns `true` if the move is a pawn advance or a capture,
    /// `false` otherwise.
    #[inline]
    pub fn is_pawn_advance_or_capure(&self) -> bool {
        // We use clever bit manipulations to avoid branches.
        const P: usize = (!PAWN & 0b111) << M_SHIFT_PIECE;
        const C: usize = (!NO_PIECE & 0b111) << M_SHIFT_CAPTURED_PIECE;
        (self.0 as usize & M_MASK_PIECE | C) ^ (self.0 as usize & M_MASK_CAPTURED_PIECE | P) >=
        M_MASK_PIECE
    }

    /// Returns the least significant 16 bits of the raw move value.
    #[inline(always)]
    pub fn digest(&self) -> MoveDigest {
        self.0 as MoveDigest
    }

    /// Returns the algebraic notation of the move.
    ///
    /// Examples: `e2e4`, `e7e5`, `e1g1` (white short castling),
    /// `e7e8q` (for promotion).
    pub fn notation(&self) -> String {
        format!("{}{}{}",
                notation(self.orig_square()),
                notation(self.dest_square()),
                match self.move_type() {
                    MOVE_PROMOTION => ["q", "r", "b", "n"][self.aux_data()],
                    _ => "",
                })
    }

    /// Decodes the promoted piece type from the raw value returned by
    /// `m.aux_data()`.
    ///
    /// The interpretation of the raw value is: `0` -- queen, `1` --
    /// rook, `2` -- bishop, `3` -- knight.
    #[inline]
    pub fn piece_from_aux_data(pp_code: usize) -> PieceType {
        assert!(pp_code <= 3);
        match pp_code {
            0 => QUEEN,
            1 => ROOK,
            2 => BISHOP,
            _ => KNIGHT,
        }
    }
}


/// Extracts the move type from a move digest.
#[inline(always)]
pub fn move_type(move_digest: MoveDigest) -> MoveType {
    ((move_digest & M_MASK_MOVE_TYPE as u16) >> M_SHIFT_MOVE_TYPE) as MoveType
}


/// Extracts the origin square from a move digest.
#[inline(always)]
pub fn orig_square(move_digest: MoveDigest) -> Square {
    ((move_digest & M_MASK_ORIG_SQUARE as u16) >> M_SHIFT_ORIG_SQUARE) as Square
}


/// Extracts the destination square from a move digest.
#[inline(always)]
pub fn dest_square(move_digest: MoveDigest) -> Square {
    ((move_digest & M_MASK_DEST_SQUARE as u16) >> M_SHIFT_DEST_SQUARE) as Square
}


/// Extracts the auxiliary data from a move digest.
#[inline(always)]
pub fn aux_data(move_digest: MoveDigest) -> usize {
    ((move_digest & M_MASK_AUX_DATA as u16) >> M_SHIFT_AUX_DATA) as usize
}


/// Stores a list of moves for each position in a given line of play.
pub struct MoveStack {
    moves: Vec<Move>,
    savepoints: Vec<usize>,
    first_move_index: usize,
}


impl MoveStack {
    /// Creates a new (empty) instance.
    pub fn new() -> MoveStack {
        MoveStack {
            moves: Vec::with_capacity(32 * 64),
            savepoints: Vec::with_capacity(32),
            first_move_index: 0,
        }
    }

    /// Clears the stack of states, removing all saved move lists.
    #[inline]
    pub fn clear(&mut self) {
        self.moves.clear();
        self.savepoints.clear();
        self.first_move_index = 0;
    }

    /// Clears the current move list, saving it so that it can be
    /// restored.
    ///
    /// This method can be called many times. At each call the current
    /// move list will be cleared, and saved to the stack of states
    /// that can later be restored.
    #[inline]
    pub fn save(&mut self) {
        self.savepoints.push(self.first_move_index);
        self.first_move_index = self.moves.len();
    }

    /// Restores the last saved move list.
    ///
    /// The current move list is permanently lost.
    #[inline]
    pub fn restore(&mut self) {
        self.moves.truncate(self.first_move_index);
        self.first_move_index = self.savepoints.pop().unwrap();
    }

    /// Returns the number of saved move lists.
    ///
    /// The number of saved move lists starts at zero. It is
    /// incremented on each call to `save`, and decremented on each
    /// call to `restore`.
    #[inline]
    pub fn ply(&self) -> usize {
        self.savepoints.len()
    }

    /// Returns the length of the current move list.
    #[inline]
    pub fn len(&self) -> usize {
        assert!(self.moves.len() >= self.first_move_index);
        self.moves.len() - self.first_move_index
    }

    /// Appends a move to the end of the current move list.
    #[inline]
    pub fn push(&mut self, m: Move) {
        assert!(self.moves.len() >= self.first_move_index);
        self.moves.push(m);
    }

    /// Removes the last element from the current move list and
    /// returns it, or `None` if empty.
    #[inline]
    pub fn pop(&mut self) -> Option<Move> {
        assert!(self.moves.len() >= self.first_move_index);
        self.moves.pop()
    }

    /// Removes a specific move from the current move list and returns
    /// it.
    ///
    /// This method tries to find a move `m` for which `m.digest() ==
    /// move_digest`. Then it removes it from the current move list,
    /// and returns it. If such move is not found, `None` is returned.
    #[inline]
    pub fn remove_move(&mut self, move_digest: MoveDigest) -> Option<Move> {
        assert!(self.moves.len() >= self.first_move_index);
        let last_move = if let Some(last) = self.moves.last() {
            *last
        } else {
            return None;
        };
        let m;
        'moves: loop {
            for curr in self.iter_mut() {
                if curr.digest() == move_digest {
                    m = *curr;
                    *curr = last_move;
                    break 'moves;
                }
            }
            return None;
        }
        assert!(!self.moves.is_empty());
        self.moves.pop();
        Some(m)
    }

    /// Removes the move with the highest value from the current move
    /// list and returns it.
    ///
    /// Returns `None` if the current move list is empty.
    #[inline]
    pub fn remove_best_move(&mut self) -> Option<Move> {
        assert!(self.moves.len() >= self.first_move_index);
        let moves = &mut self.moves;
        if moves.len() > self.first_move_index {
            let i = moves.len() - 1;
            unsafe {
                let mut m = *moves.get_unchecked(i);
                let mut j = i;
                loop {
                    // The current best move candidate is moved to index
                    // `i` (swapped with the previous candidate).
                    if *moves.get_unchecked(j) > m {
                        *moves.get_unchecked_mut(i) = *moves.get_unchecked_mut(j);
                        *moves.get_unchecked_mut(j) = m;
                        m = *moves.get_unchecked(i);
                    }
                    if j == self.first_move_index {
                        break;
                    }
                    j -= 1;
                }
                moves.pop();
                return Some(m);
            }
        }
        return None;
    }

    /// Returns an iterator over each move in the current move list.
    #[inline]
    pub fn iter(&self) -> slice::Iter<Move> {
        self.moves[self.first_move_index..].iter()
    }

    /// Returns an iterator that allows modifying each move in the
    /// current move list.
    #[inline]
    pub fn iter_mut(&mut self) -> slice::IterMut<Move> {
        self.moves[self.first_move_index..].iter_mut()
    }
}


/// The maximum possible move score.
pub const MOVE_SCORE_MAX: u32 = std::u32::MAX;


/// `MOVE_ENPASSANT`, `MOVE_PROMOTION`, `MOVE_CASTLING`, or
/// `MOVE_NORMAL`.
pub type MoveType = usize;

/// En-passant capture move type.
pub const MOVE_ENPASSANT: MoveType = 0;

/// Pawn promotion move type.
pub const MOVE_PROMOTION: MoveType = 1;

/// Castling move type.
pub const MOVE_CASTLING: MoveType = 2;

/// Normal move type.
pub const MOVE_NORMAL: MoveType = 3;


// Field shifts
const M_SHIFT_CAPTURED_PIECE: usize = 27;
const M_SHIFT_PIECE: usize = 24;
const M_SHIFT_CASTLING_DATA: usize = 20;
const M_SHIFT_ENPASSANT_FILE: usize = 16;
const M_SHIFT_MOVE_TYPE: usize = 14;
const M_SHIFT_ORIG_SQUARE: usize = 8;
const M_SHIFT_DEST_SQUARE: usize = 2;
const M_SHIFT_AUX_DATA: usize = 0;

// Field masks
const M_MASK_CAPTURED_PIECE: usize = 0b111 << M_SHIFT_CAPTURED_PIECE;
const M_MASK_PIECE: usize = 0b111 << M_SHIFT_PIECE;
#[allow(dead_code)]
const M_MASK_CASTLING_DATA: usize = 0b1111 << M_SHIFT_CASTLING_DATA;
const M_MASK_ENPASSANT_FILE: usize = 0b1111 << M_SHIFT_ENPASSANT_FILE;
const M_MASK_MOVE_TYPE: usize = 0b11 << M_SHIFT_MOVE_TYPE;
const M_MASK_ORIG_SQUARE: usize = 0b111111 << M_SHIFT_ORIG_SQUARE;
const M_MASK_DEST_SQUARE: usize = 0b111111 << M_SHIFT_DEST_SQUARE;
const M_MASK_AUX_DATA: usize = 0b11 << M_SHIFT_AUX_DATA;


#[cfg(test)]
mod tests {
    use super::*;
    use basetypes::*;
    use castling_rights::CastlingRights;
    const NO_ENPASSANT_FILE: usize = 8;

    #[test]
    fn test_move() {
        let cr = CastlingRights::new(0b1011);
        let mut m = Move::new(WHITE,
                              MOVE_NORMAL,
                              PAWN,
                              E2,
                              E4,
                              NO_PIECE,
                              NO_ENPASSANT_FILE,
                              cr,
                              0);
        let n1 = Move::new(WHITE,
                           MOVE_NORMAL,
                           PAWN,
                           F3,
                           E4,
                           KNIGHT,
                           NO_ENPASSANT_FILE,
                           CastlingRights::new(0),
                           0);
        let n2 = Move::new(WHITE,
                           MOVE_NORMAL,
                           KING,
                           F3,
                           E4,
                           NO_PIECE,
                           NO_ENPASSANT_FILE,
                           CastlingRights::new(0),
                           0);
        let n3 = Move::new(BLACK,
                           MOVE_PROMOTION,
                           PAWN,
                           F2,
                           F1,
                           NO_PIECE,
                           NO_ENPASSANT_FILE,
                           CastlingRights::new(0),
                           1);
        let n4 = Move::new(WHITE,
                           MOVE_NORMAL,
                           BISHOP,
                           F2,
                           E3,
                           KNIGHT,
                           NO_ENPASSANT_FILE,
                           CastlingRights::new(0),
                           0);
        let n5 = Move::new(WHITE,
                           MOVE_NORMAL,
                           PAWN,
                           F2,
                           E3,
                           KNIGHT,
                           NO_ENPASSANT_FILE,
                           CastlingRights::new(0),
                           0);
        assert!(n1 > m);
        assert!(n2 < m);
        assert_eq!(m.piece(), PAWN);
        assert_eq!(m.captured_piece(), NO_PIECE);
        assert_eq!(m.orig_square(), E2);
        assert_eq!(m.dest_square(), E4);
        assert_eq!(m.en_passant_file(), 8);
        assert_eq!(m.aux_data(), 0);
        assert_eq!(m.castling().value(), 0b1011);
        let m2 = m;
        assert_eq!(m, m2);
        m.set_score(MOVE_SCORE_MAX);
        assert_eq!(m.score(), MOVE_SCORE_MAX);
        m.set_score(3);
        assert_eq!(m.score(), 3);
        assert!(m > m2);
        m.set_score(0);
        assert_eq!(m.score(), 0);
        assert_eq!(n3.aux_data(), 1);
        assert_eq!(n1.digest(), (n1.0 & 0xffff) as MoveDigest);
        assert!(m.is_pawn_advance_or_capure());
        assert!(!n2.is_pawn_advance_or_capure());
        assert!(n4.is_pawn_advance_or_capure());
        assert!(n5.is_pawn_advance_or_capure());
    }

    #[test]
    fn test_move_stack() {
        let m = Move::new(WHITE,
                          MOVE_NORMAL,
                          PAWN,
                          E2,
                          E4,
                          NO_PIECE,
                          NO_ENPASSANT_FILE,
                          CastlingRights::new(0),
                          0);
        let mut s = MoveStack::new();
        assert!(s.remove_best_move().is_none());
        s.save();
        s.push(m);
        assert_eq!(s.remove_best_move().unwrap(), m);
        assert!(s.remove_best_move().is_none());
        s.restore();
        assert!(s.remove_best_move().is_none());
        s.push(m);
        s.push(m);
        s.save();
        s.push(m);
        s.restore();
        assert_eq!(s.remove_best_move().unwrap(), m);
        assert_eq!(s.remove_best_move().unwrap(), m);
        assert!(s.remove_best_move().is_none());
    }
}

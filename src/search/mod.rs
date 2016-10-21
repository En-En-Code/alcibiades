//! Implements alpha-beta searching with iterative deepening,
//! aspiration windows, and multi-PV.
//!
//! The alpha-beta algorithm is an enhancement to the minimax search
//! algorithm. It maintains two values, alpha and beta. They represent
//! the minimum score that the maximizing player is assured of (lower
//! bound) and the maximum score that the minimizing player is assured
//! of (upper bound) respectively.
//!
//! Iterative deepening works as follows: the program starts with a
//! one ply search, then increments the search depth and does another
//! search. This process is repeated until the time allocated for the
//! search is exhausted or the maximum search depth is reached. In
//! case of an unfinished search, the program can always fall back to
//! the move selected in the last iteration of the search.
//!
//! Aspiration windows are a way to reduce the search space in the
//! search. The way it works is that we get the value from the last
//! search iteration, calculate a window around it, and use this as
//! alpha-beta bounds for the next search. Because the window is
//! narrower, more beta cutoffs are achieved, and the search takes a
//! shorter time. The drawback is that if the true score is outside
//! this window, then a costly re-search must be made. But then most
//! probably the re-search will be much faster, because many positions
//! will be remembered from the transposition table.
//!
//! In multi-PV mode the engine calculates and sends to the GUI
//! several principal variations (PV), each one starting with a
//! different first move. This mode makes the search slower, but is
//! very useful for chess analysis.

pub mod alpha_beta;
pub mod threading;

use std::cmp::{min, max};
use std::time::Duration;
use std::thread;
use std::sync::{Arc, Mutex, Condvar};
use std::sync::mpsc::{channel, Sender, Receiver, TryRecvError};
use basetypes::*;
use moves::*;
use tt::*;
use position::*;
use self::threading::*;


/// The maximum search depth in half-moves.
pub const MAX_DEPTH: u8 = 63; // Should be less than 127.


/// Parameters describing a new search.
#[derive(Clone)]
pub struct SearchParams {
    /// A number identifying the new search.
    pub search_id: usize,

    /// The root position for the new search.
    pub position: Position,

    /// The requested search depth.
    pub depth: u8,

    /// The lower bound for the new search (alpha).
    pub lower_bound: Value,

    /// The upper bound for the new search (beta).
    pub upper_bound: Value,

    /// The evaluation of the root position so far, or `VALUE_UNKNOWN`
    /// if not available.
    pub value: Value,

    /// Restricts the analysis to the supplied list of moves only.
    ///
    /// The behavior of the search is undefined if `searchmoves` is
    /// empty, but the supplied root position is not final.
    pub searchmoves: Vec<Move>,

    /// Specifies how many best lines of play to calculate (for the
    /// multi-PV mode).
    ///
    /// Must be greater than zero.
    pub variation_count: usize,
}


/// A progress report from a search.
#[derive(Clone)]
pub struct Report {
    /// The ID assigned to search.
    pub search_id: usize,

    /// The number of positions searched so far.
    pub searched_nodes: NodeCount,

    /// The search depth completed so far.
    pub depth: u8,

    /// The evaluation of the root position so far, or `VALUE_UNKNOWN`
    /// if not available.
    pub value: Value,

    /// The best moves found so far (sorted by descending strength),
    /// or an empty list if not available.
    pub best_moves: Vec<Move>,

    /// `true` if the search is done, `false` otherwise.
    pub done: bool,
}


/// The `SearchExecutor` trait is used to execute consecutive searches
/// in different starting positions.
pub trait SearchExecutor {
    /// Starts a new search.
    ///
    /// After calling `start_search`, `try_recv_report` must be called
    /// periodically until the returned report indicates that the
    /// search is done. A new search must not be started until the
    /// previous search is done.
    fn start_search(&mut self, params: SearchParams);

    /// Attempts to return a search progress report without blocking.
    fn try_recv_report(&mut self) -> Result<Report, TryRecvError>;

    /// Waits until a search progress report is available, timing out
    /// after a specified duration or earlier.
    fn wait_report(&self, duration: Duration);

    /// Requests the termination of the current search.
    ///
    /// After calling `terminate`, `try_recv_report` must continue to
    /// be called periodically until the returned report indicates
    /// that the search is done.
    fn terminate_search(&mut self);
}


/// Executes bare alpha-beta searches.
///
/// **Important note:** `SimpleSearcher` always considers all legal
/// moves in the root position, and supplies no `best_moves` in its
/// progress reports.
struct SimpleSearcher {
    thread_join_handle: Option<thread::JoinHandle<()>>,
    thread_commands: Sender<Command>,
    thread_reports: Receiver<Report>,
    has_reports_condition: Arc<(Mutex<bool>, Condvar)>,
}

impl SimpleSearcher {
    /// Creates a new instance.
    pub fn new(tt: Arc<Tt>) -> SimpleSearcher {
        let (commands_tx, commands_rx) = channel();
        let (reports_tx, reports_rx) = channel();
        let has_reports_condition = Arc::new((Mutex::new(false), Condvar::new()));
        SimpleSearcher {
            thread_commands: commands_tx,
            thread_reports: reports_rx,
            has_reports_condition: has_reports_condition.clone(),

            // Spawn a thread that will do the real work.
            thread_join_handle: Some(thread::spawn(move || {
                serve_simple(tt, commands_rx, reports_tx, has_reports_condition);
            })),
        }
    }
}

impl SearchExecutor for SimpleSearcher {
    fn start_search(&mut self, params: SearchParams) {
        assert!(params.searchmoves.is_empty(),
                "SimpleSearcher can not handle non-empty searchmoves");
        debug_assert!(params.depth <= MAX_DEPTH);
        debug_assert!(params.lower_bound < params.upper_bound);
        debug_assert!(params.lower_bound != VALUE_UNKNOWN);
        debug_assert!(params.variation_count != 0);
        self.thread_commands.send(Command::Start(params)).unwrap();
    }

    fn try_recv_report(&mut self) -> Result<Report, TryRecvError> {
        let result = self.thread_reports.try_recv();
        if result.is_err() {
            *self.has_reports_condition.0.lock().unwrap() = false;
        }
        result
    }

    fn wait_report(&self, duration: Duration) {
        let &(ref has_reports, ref condition) = &*self.has_reports_condition;
        let has_reports = has_reports.lock().unwrap();
        if !*has_reports {
            condition.wait_timeout(has_reports, duration).unwrap();
        }
    }

    fn terminate_search(&mut self) {
        self.thread_commands.send(Command::Terminate).unwrap();
    }
}

impl Drop for SimpleSearcher {
    fn drop(&mut self) {
        self.thread_commands.send(Command::Exit).unwrap();
        self.thread_join_handle.take().unwrap().join().unwrap();
    }
}


/// Executes multi-PV searches.
struct MultipvSearcher {
    params: SearchParams,

    // `true` if the current search has been terminated.
    search_is_terminated: bool,

    // The number of positions analyzed during previous sub-searches.
    previously_searched_nodes: NodeCount,

    // The evaluation of the root position so far.
    value: Value,

    // The real work will be handed over to `SimpleSearcher`.
    searcher: SimpleSearcher,
}

impl MultipvSearcher {
    /// Creates a new instance.
    pub fn new(tt: Arc<Tt>) -> MultipvSearcher {
        MultipvSearcher {
            params: bogus_params(),
            search_is_terminated: false,
            previously_searched_nodes: 0,
            value: VALUE_UNKNOWN,
            searcher: SimpleSearcher::new(tt),
        }
    }
}

impl SearchExecutor for MultipvSearcher {
    fn start_search(&mut self, params: SearchParams) {
        debug_assert!(params.depth <= MAX_DEPTH);
        debug_assert!(params.lower_bound < params.upper_bound);
        debug_assert!(params.lower_bound != VALUE_UNKNOWN);
        debug_assert!(params.variation_count != 0);
        self.params = params;
        self.search_is_terminated = false;
        self.previously_searched_nodes = 0;
        self.value = self.params.value;

        self.searcher.start_search(SearchParams { searchmoves: vec![], ..self.params.clone() });
    }

    fn try_recv_report(&mut self) -> Result<Report, TryRecvError> {
        self.searcher.try_recv_report()
    }

    fn wait_report(&self, duration: Duration) {
        self.searcher.wait_report(duration);
    }

    fn terminate_search(&mut self) {
        self.search_is_terminated = true;
        self.searcher.terminate_search();
    }
}


/// Executes multi-PV searches with aspiration windows.
struct AspirationSearcher {
    params: SearchParams,
    search_is_terminated: bool,
    previously_searched_nodes: NodeCount,
    value: Value,

    // The aspiration window will be widened by this value if the
    // search fails. (We use `isize` to avoid overflows.)
    delta: isize,

    // The lower bound of the aspiration window.
    alpha: Value,

    // The upper bound of the aspiration window.
    beta: Value,

    // The real work will be handed over to `MultipvSearcher`.
    searcher: MultipvSearcher,
}

impl AspirationSearcher {
    /// Creates a new instance.
    pub fn new(tt: Arc<Tt>) -> AspirationSearcher {
        AspirationSearcher {
            params: bogus_params(),
            search_is_terminated: false,
            previously_searched_nodes: 0,
            value: VALUE_UNKNOWN,
            delta: 0,
            alpha: VALUE_MIN,
            beta: VALUE_MAX,
            searcher: MultipvSearcher::new(tt),
        }
    }

    /// A helper method. It tells the multi-PV searcher to run a new
    /// search.
    fn start_aspirated_search(&mut self) {
        self.searcher.start_search(SearchParams {
            search_id: 0,
            lower_bound: self.alpha,
            upper_bound: self.beta,
            value: self.value,
            ..self.params.clone()
        });
    }

    /// A helper method. It multiplies `self.delta` by a constant.
    fn increase_delta(&mut self) {
        self.delta += 3 * self.delta / 8;
        if self.delta > 1500 {
            self.delta = 1_000_000;
        }
    }

    /// A helper method. It widens the aspiration window if necessary.
    fn widen_aspiration_window(&mut self) -> bool {
        let SearchParams { lower_bound, upper_bound, .. } = self.params;
        let v = self.value;
        if lower_bound < self.alpha && lower_bound < v && v <= self.alpha {
            // Set smaller `self.alpha`.
            self.alpha = max(v as isize - self.delta, lower_bound as isize) as Value;
            self.increase_delta();
            return true;
        } else if self.beta < upper_bound && self.beta <= v && v < upper_bound {
            // Set bigger `self.beta`.
            self.beta = min(v as isize + self.delta, upper_bound as isize) as Value;
            self.increase_delta();
            return true;
        }
        false
    }
}

impl SearchExecutor for AspirationSearcher {
    fn start_search(&mut self, params: SearchParams) {
        debug_assert!(params.depth <= MAX_DEPTH);
        debug_assert!(params.lower_bound < params.upper_bound);
        debug_assert!(params.lower_bound != VALUE_UNKNOWN);
        debug_assert!(params.variation_count != 0);
        self.params = params;
        self.search_is_terminated = false;
        self.previously_searched_nodes = 0;
        self.value = self.params.value;

        // This is the half-width of the initial aspiration window.
        self.delta = 17; // TODO: make this `16`?

        // Set the initial aspiration window (`self.alpha`, `self.beta`).
        let SearchParams { value, lower_bound, upper_bound, .. } = self.params;
        let (a, b) = if value == VALUE_UNKNOWN || value < lower_bound || value > upper_bound {
            (lower_bound, upper_bound)
        } else {
            let v = value as isize;
            (max(v - self.delta, lower_bound as isize) as Value,
             min(v + self.delta, upper_bound as isize) as Value)
        };
        self.alpha = a;
        self.beta = b;

        self.start_aspirated_search();
    }

    fn try_recv_report(&mut self) -> Result<Report, TryRecvError> {
        let Report { searched_nodes, depth, value, best_moves, mut done, .. } =
            try!(self.searcher.try_recv_report());
        if value != VALUE_UNKNOWN {
            self.value = value;
        }
        let searched_nodes = self.previously_searched_nodes + searched_nodes;
        let completed_depth = if done && !self.search_is_terminated {
            debug_assert_eq!(depth, self.params.depth);
            self.previously_searched_nodes = searched_nodes;
            if self.widen_aspiration_window() {
                // A re-search is necessary.
                self.start_aspirated_search();
                done = false;
                0
            } else {
                depth
            }
        } else {
            0
        };

        return Ok(Report {
            search_id: self.params.search_id,
            searched_nodes: searched_nodes,
            depth: completed_depth,
            value: self.value,
            best_moves: best_moves,
            done: done,
        });
    }

    fn wait_report(&self, duration: Duration) {
        self.searcher.wait_report(duration);
    }

    fn terminate_search(&mut self) {
        self.search_is_terminated = true;
        self.searcher.terminate_search();
    }
}


/// Executes multi-PV searches with aspiration windows and iterative
/// deepening.
pub struct DeepeningSearcher {
    params: SearchParams,
    search_is_terminated: bool,
    previously_searched_nodes: NodeCount,
    value: Value,

    // The depth of the currently executing search.
    depth: u8,

    // The real work will be handed over to `AspirationSearcher`.
    searcher: AspirationSearcher,
}

impl DeepeningSearcher {
    /// Creates a new instance.
    pub fn new(tt: Arc<Tt>) -> DeepeningSearcher {
        DeepeningSearcher {
            params: bogus_params(),
            search_is_terminated: false,
            previously_searched_nodes: 0,
            value: VALUE_UNKNOWN,
            depth: 0,
            searcher: AspirationSearcher::new(tt),
        }
    }

    /// A helper method. It tells the aspiration searcher to run a new
    /// search.
    fn start_deeper_search(&mut self) {
        self.depth += 1;
        let value = if self.depth < 5 {
            // During the first few iterations the evaluation is
            // unreliable, and therefore we should not pass it to the
            // aspiration searcher.
            VALUE_UNKNOWN
        } else {
            self.value
        };
        self.searcher.start_search(SearchParams {
            search_id: 0,
            depth: self.depth,
            value: value,
            ..self.params.clone()
        });
    }
}

impl SearchExecutor for DeepeningSearcher {
    fn start_search(&mut self, params: SearchParams) {
        debug_assert!(params.depth <= MAX_DEPTH);
        debug_assert!(params.lower_bound < params.upper_bound);
        debug_assert!(params.lower_bound != VALUE_UNKNOWN);
        debug_assert!(params.variation_count != 0);
        self.params = params;
        self.search_is_terminated = false;
        self.previously_searched_nodes = 0;
        self.value = self.params.value;
        self.depth = 0;

        self.start_deeper_search();
    }

    fn try_recv_report(&mut self) -> Result<Report, TryRecvError> {
        let Report { searched_nodes, depth, value, best_moves, mut done, .. } =
            try!(self.searcher.try_recv_report());
        if value != VALUE_UNKNOWN {
            self.value = value;
        }
        let searched_nodes = self.previously_searched_nodes + searched_nodes;
        let completed_depth = if done && !self.search_is_terminated {
            debug_assert_eq!(depth, self.depth);
            self.previously_searched_nodes = searched_nodes;
            if self.depth < self.params.depth {
                self.start_deeper_search();
                done = false;
            }
            depth
        } else {
            self.depth - 1
        };

        return Ok(Report {
            search_id: self.params.search_id,
            searched_nodes: searched_nodes,
            depth: completed_depth,
            value: self.value,
            best_moves: best_moves,
            done: done,
        });
    }

    fn wait_report(&self, duration: Duration) {
        self.searcher.wait_report(duration);
    }

    fn terminate_search(&mut self) {
        self.search_is_terminated = true;
        self.searcher.terminate_search();
    }
}


/// A helper function. It returns bogus search parameters.
fn bogus_params() -> SearchParams {
    SearchParams {
        search_id: 0,
        position: Position::from_fen(START_POSITION_FEN).ok().unwrap(),
        depth: 0,
        lower_bound: VALUE_MIN,
        upper_bound: VALUE_MAX,
        value: VALUE_UNKNOWN,
        searchmoves: vec![],
        variation_count: 1,
    }
}

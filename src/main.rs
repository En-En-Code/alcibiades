#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate rand;

pub mod chesstypes;
pub mod board;
pub mod search;
pub mod uci;
pub mod tt;
pub mod engine;

use std::process::exit;
use uci::Server;
use engine::Engine;

fn main() {
    if let Ok(mut uci_loop) = Server::<Engine>::wait_for_hanshake() {
        match uci_loop.serve() {
            Ok(_) => {
                exit(0);
            }
            Err(_) => {
                exit(1);
            }
        }
    }
    exit(2);
}

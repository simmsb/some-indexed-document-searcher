use std::sync::{Arc, atomic::AtomicUsize};

pub mod widgets;

use self::widgets::main::Main;
use super::searcher::Searcher;

use relm::Widget;

pub fn spawn(searcher: Searcher, indexed_files: Arc<AtomicUsize>) {
    Main::run((searcher, indexed_files)).unwrap();
}

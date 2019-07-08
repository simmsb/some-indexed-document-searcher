pub mod widgets;

use self::widgets::main::Main;
use super::searcher::Searcher;

use relm::Widget;

pub fn spawn(searcher: Searcher) {
    Main::run(searcher).unwrap();
}

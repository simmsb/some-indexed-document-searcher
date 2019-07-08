pub mod widgets;

use self::widgets::main::Main;

use relm::Widget;

pub fn spawn() {
    Main::run(()).unwrap();
}

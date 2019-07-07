use relm::{Relm, Update, Widget, connect, connect_stream};
use relm_derive::{Msg};
use gtk::prelude::*;
use gtk::{Window, Inhibit, WindowType};

struct SearchResult {
    filename: String,
}

struct MainModel {
    search: String,
    results: Vec<SearchResult>,
}

#[derive(Msg)]
enum Msg {
    Quit,
    Search(String),
}

struct Win {
    model: MainModel,
    window: Window,
}

impl Update for Win {
    type Model = MainModel;
    type ModelParam = ();
    type Msg = Msg;

    fn model(_: &Relm<Self>, _: ()) -> MainModel {
        MainModel {
            search: "".to_owned(),
            results: Vec::new(),
        }
    }

    fn update(&mut self, event: Msg) {
        match event {
            Msg::Quit => gtk::main_quit(),
            Msg::Search(s) => ()
        }
    }
}

impl Widget for Win {
    type Root = Window;

    fn root(&self) -> Self::Root {
        self.window.clone()
    }

    fn view(relm: &Relm<Self>, model: Self::Model) -> Self {
        let window = Window::new(WindowType::Toplevel);

        connect!(relm, window, connect_delete_event(_, _), return (Some(Msg::Quit), Inhibit(false)));

        window.show_all();

        Win {
            model,
            window,
        }
    }
}

pub fn spawn() {
    Win::run(()).unwrap();
}

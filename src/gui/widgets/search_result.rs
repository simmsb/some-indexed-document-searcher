use gtk::prelude::*;
use relm::{connect, Widget};
use relm_attributes::widget;
use relm_derive::Msg;

#[derive(Msg)]
pub enum Msg {
    Open,
}

pub struct Model {
    file_path: String,
    snippet: String,
}

#[widget]
impl Widget for SearchResult {
    fn init_view(&mut self) {
        self.file_path_label.set_text(&self.model.file_path);
        self.snippet_label.set_markup(&self.model.snippet);
    }

    fn model((file_path, snippet): (String, String)) -> Model {
        Model { file_path, snippet: snippet.replace('\n', " ") }
    }

    fn update(&mut self, event: Msg) {
        match event {
            Msg::Open => println!("Result opened: {}", self.model.file_path),
        }
    }

    view! {
        gtk::Box {
            orientation: gtk::Orientation::Horizontal,
            child: {
                expand: true,
                fill: true,
            },
            gtk::Box {
                orientation: gtk::Orientation::Vertical,
                child: {
                    expand: true,
                    fill: true,
                },
                #[name="file_path_label"]
                gtk::Label {
                    child: {
                        expand: true,
                        fill: true,
                    },
                },
                #[name="snippet_label"]
                gtk::Label {
                    selectable: true,
                    line_wrap: true,
                }
            },
            gtk::Button {
                clicked => Msg::Open,
                label: "Open",
            },
        },
    }
}

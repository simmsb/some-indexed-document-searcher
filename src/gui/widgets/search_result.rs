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
}

#[widget]
impl Widget for SearchResult {
    fn init_view(&mut self) {
        self.file_path_label.set_text(&self.model.file_path);
    }

    fn model(file_path: String) -> Model {
        Model { file_path }
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
            #[name="file_path_label"]
            gtk::Label {
                child: {
                    expand: true,
                    fill: true,
                },
            },
            gtk::Button {
                clicked => Msg::Open,
                label: "Open",
            },
        },
    }
}

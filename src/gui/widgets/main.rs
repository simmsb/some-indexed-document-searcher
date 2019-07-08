use gtk::prelude::*;
use relm::{connect, Widget, ContainerWidget};
use relm_attributes::widget;
use relm_derive::Msg;

#[derive(Msg)]
pub enum Msg {
    Quit,
    Search(String),
}

pub struct Model {
    results: Vec<relm::Component<super::SearchResult>>,
}

impl Main {
    fn update_results(&mut self, results: &[String]) {
        self.clear();

        for result in results {
            let child = self
                .results_list
                .add_widget::<super::SearchResult>(result.clone());

            self.model.results.push(child);
        }
    }

    fn clear(&mut self) {
        for child in self.results_list.get_children() {
            self.results_list.remove(&child);
        }

        self.model.results.clear();
    }
}

#[widget]
impl Widget for Main {
    fn model() -> Model {
        Model {
            results: Vec::new(),
        }
    }

    fn update(&mut self, event: Msg) {
        match event {
            Msg::Quit => gtk::main_quit(),
            Msg::Search(s) => println!("Searching for: {}", s),
        }
    }

    view! {
        gtk::Window {
            gtk::Box {
                orientation: gtk::Orientation::Vertical,
                gtk::SearchEntry {
                    changed(entry) => Msg::Search(entry.get_text().unwrap().to_string()),
                    placeholder_text: Some("Search"),
                },
                #[name="results_list"]
                gtk::ListBox {
                    child: {
                        fill: true,
                        expand: true,
                    },
                },
            },
        },
    }
}

use gtk::prelude::*;
use relm::{connect, connect_stream, interval, ContainerWidget, Relm, Widget};
use relm_attributes::widget;
use relm_derive::Msg;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use crate::searcher::{SearchResult, Searcher};

#[derive(Msg)]
pub enum Msg {
    Quit,
    Tick,
    Search(String),
}

pub struct Model {
    searcher: Searcher,
    indexed_files: Arc<AtomicUsize>,
    results: Vec<relm::Component<super::SearchResult>>,
}

impl Main {
    fn update_results(&mut self, results: Vec<SearchResult>) {
        self.clear();

        for result in results {
            let child = self
                .results_list
                .add_widget::<super::SearchResult>((result.path, result.snippet));

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
    fn model((searcher, indexed_files): (Searcher, Arc<AtomicUsize>)) -> Model {
        Model {
            searcher,
            indexed_files,
            results: Vec::new(),
        }
    }

    fn subscriptions(&mut self, relm: &Relm<Self>) {
        interval(relm.stream(), 1000, || Msg::Tick);
    }

    fn update(&mut self, event: Msg) {
        match event {
            Msg::Quit => gtk::main_quit(),
            Msg::Tick => {
                self.stats_label.set_text(&format!(
                    "{} indexed files",
                    self.model.indexed_files.load(Ordering::Relaxed)
                ));
            }
            Msg::Search(s) => {
                if let Some(results) = self.model.searcher.search(&s) {
                    self.update_results(results);
                }
            }
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
                gtk::Box {
                    orientation: gtk::Orientation::Vertical,
                    child: {
                        fill: true,
                        expand: true,
                    },
                },
                #[name="stats_label"]
                gtk::Label {},
            },
            delete_event(_, _) => (Msg::Quit, Inhibit(false)),
        },
    }
}

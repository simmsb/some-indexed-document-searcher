// stuff for indexing files

use crossbeam_channel::{Receiver, Sender};
use failure::{Compat, Fail}; // oh no
use num_cpus;
use snafu::{ResultExt, Snafu};
use std::{
    fs,
    io::{BufReader, Read},
};
use tantivy::{self, doc, schema::*};

use super::config;
use super::file_collector::FileEntry;
use super::once_every;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Could not open index directory: {}", source))]
    IndexDirError {
        source: tantivy::directory::error::OpenDirectoryError,
    },
    #[snafu(display("Something went wrong with Tantivy: {}", source))]
    IndexTantivyError {
        #[snafu(source(from(tantivy::TantivyError, tantivy::TantivyError::compat)))]
        source: Compat<tantivy::TantivyError>,
    },
}

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Clone)]
pub struct DocSchema {
    full_path: Field,
    filename: Field,
    content: Field,
    schema: Schema,
}

impl DocSchema {
    pub fn full_path(&self) -> Field {
        self.full_path
    }

    pub fn filename(&self) -> Field {
        self.filename
    }

    pub fn content(&self) -> Field {
        self.content
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }
}

pub struct DocIndexer {
    schema: DocSchema,
    indexer: tantivy::Index,
    indexer_threads: Option<IndexerThreads>,
}

impl DocIndexer {
    pub fn new(config: &config::Config) -> Result<DocIndexer> {
        let mut schema_builder = Schema::builder();

        let full_path = schema_builder.add_text_field("full_path", STORED);
        let filename = schema_builder.add_text_field("filename", STRING | STORED);
        let content = schema_builder.add_text_field("content", TEXT | STORED);

        let schema = schema_builder.build();

        let mut indexer = Self::create_indexer(&schema, config)?;
        indexer.set_default_multithread_executor();

        Ok(DocIndexer {
            schema: DocSchema {
                full_path,
                filename,
                content,
                schema,
            },
            indexer,
            indexer_threads: None,
        })
    }

    pub fn schema(&self) -> &DocSchema {
        &self.schema
    }

    pub fn indexer(&self) -> &tantivy::Index {
        &self.indexer
    }

    fn create_indexer(schema: &Schema, config: &config::Config) -> Result<tantivy::Index> {
        let index_folder = config.index_location.join("index");
        std::fs::create_dir_all(&index_folder).unwrap();
        let dir = tantivy::directory::MmapDirectory::open(&index_folder).context(IndexDirError)?;

        let index =
            tantivy::Index::open_or_create(dir, schema.clone()).context(IndexTantivyError)?;

        Ok(index)
    }

    pub fn spawn_workers(&mut self) -> Result<()> {
        self.indexer_threads = Some(IndexerThreads::new(&self.schema, &self.indexer)?);

        Ok(())
    }

    pub fn close(&mut self) {
        if let Some(workers) = self.indexer_threads.take() {
            workers.join();
        }
    }

    pub fn add_job(&self, req: IndexRequest) {
        self.indexer_threads
            .as_ref()
            .expect("DocIndexer has no threads")
            .add_job(req);
    }
}

#[derive(Debug)]
pub enum IndexAction {
    /// File exists in the db already
    ReIndex,
    /// File does not exist yet
    Index,
}

#[derive(Debug)]
pub struct IndexRequest(pub FileEntry, pub IndexAction);

//  the system looks a bit like this
//
//
//                      / reader thread 0 \
//  file paths to index - ............... - tantivy indexer
//                      \ reader thread n /
//
//
//

struct IndexerWorker {
    i_recv: Receiver<IndexRequest>,
    d_send: Sender<(Option<Term>, Document)>,
    schema: DocSchema,
}

impl IndexerWorker {
    fn go(self) {
        for IndexRequest(doc, action) in &self.i_recv {
            if let Some(content) = match doc.file_ext() {
                "txt" | "org" | "md" | "rst" => self.index_text_doc(&doc),
                _ext => {
                    // eprintln!("Unknown ext: {}", ext);
                    continue;
                }
            } {
                let revoke_doc = match action {
                    IndexAction::ReIndex => Some(Term::from_field_text(
                        self.schema.full_path,
                        doc.full_path(),
                    )),
                    IndexAction::Index => None,
                };

                let doc = doc!(
                    self.schema.full_path => doc.full_path(),
                    self.schema.filename => doc.file_name(),
                    self.schema.content => content,
                );

                let _ = self.d_send.send((revoke_doc, doc));
            }
        }
    }

    fn index_text_doc(&self, doc: &FileEntry) -> Option<String> {
        // TODO: eventually keep track of errors
        let f = fs::File::open(&doc.full_path).ok()?;

        // TODO: don't read the file if it's over some size

        let mut buf_reader = BufReader::new(f);
        let mut content = String::new();
        buf_reader.read_to_string(&mut content).ok()?;

        Some(content)
    }
}

pub struct IndexerThreads {
    doc_processor_threads: Vec<std::thread::JoinHandle<()>>,
    doc_consumer_thread: std::thread::JoinHandle<()>,
    index_sender: Sender<IndexRequest>,
}

impl IndexerThreads {
    pub fn new(schema: &DocSchema, indexer: &tantivy::Index) -> Result<Self> {
        // TODO: make this configurable

        let num_cpus = num_cpus::get();
        let (index_send, index_recv) = crossbeam_channel::bounded(num_cpus);
        let (doc_send, doc_recv) = crossbeam_channel::bounded(num_cpus);

        let doc_processor_threads = (0..num_cpus)
            .map(|_| {
                let i_recv = index_recv.clone();
                let d_send = doc_send.clone();
                let t_schema = schema.clone();

                Ok(std::thread::spawn(move || {
                    let worker = IndexerWorker {
                        i_recv,
                        d_send,
                        schema: t_schema,
                    };

                    worker.go()
                }))
            })
            .collect::<Result<_>>()?;

        let doc_writer = indexer.writer(500_000_000).context(IndexTantivyError)?;

        let doc_consumer_thread = std::thread::spawn(move || {
            Self::do_doc_writes(doc_writer, doc_recv);
        });

        Ok(IndexerThreads {
            doc_processor_threads,
            doc_consumer_thread,
            index_sender: index_send,
        })
    }

    fn do_doc_writes(mut writer: tantivy::IndexWriter, d_recv: Receiver<(Option<Term>, Document)>) {
        for ((revoke_doc, doc), should_commit) in
            d_recv.iter().zip(once_every::OnceEvery::new(1000))
        {
            if let Some(revoke_doc) = revoke_doc {
                writer.delete_term(revoke_doc);
            }

            writer.add_document(doc);

            if should_commit {
                let _ = writer.commit();
            }
        }

        let _ = writer.commit();
    }

    pub fn join(self) {
        drop(self.index_sender);

        for t in self.doc_processor_threads {
            t.join().expect("Failed to join thread.");
        }

        self.doc_consumer_thread
            .join()
            .expect("Failed to join thread");
    }

    pub fn add_job(&self, req: IndexRequest) {
        self.index_sender.send(req).expect("Failed adding job");
    }
}

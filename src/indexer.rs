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

#[derive(Debug, Snafu)]
pub enum IndexerError {
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

type Result<T, E = IndexerError> = std::result::Result<T, E>;

#[derive(Clone)]
pub struct DocSchema {
    pub full_path: Field,
    pub filename: Field,
    pub content: Field,
    pub modified: Field,
    pub schema: Schema,
}

pub struct DocIndexer {
    pub schema: DocSchema,
    pub indexer: tantivy::Index,
    pub indexer_threads: Option<IndexerThreads>,
}

impl DocIndexer {
    pub fn new(config: &config::Config) -> Result<DocIndexer> {
        let mut schema_builder = Schema::builder();

        let full_path = schema_builder.add_text_field("full_path", STORED);
        let filename = schema_builder.add_text_field("filename", STRING | STORED);
        let content = schema_builder.add_text_field("content", TEXT | STORED);
        let modified = schema_builder.add_u64_field("last_modified", FAST | INDEXED | STORED);

        let schema = schema_builder.build();

        let indexer = Self::create_indexer(&schema, config)?;

        Ok(DocIndexer {
            schema: DocSchema {
                full_path,
                filename,
                content,
                modified,
                schema,
            },
            indexer,
            indexer_threads: None,
        })
    }

    fn create_indexer(schema: &Schema, config: &config::Config) -> Result<tantivy::Index> {
        std::fs::create_dir_all(&config.index_location).unwrap();
        let dir = tantivy::directory::MmapDirectory::open(&config.index_location)
            .context(IndexDirError)?;

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
pub struct IndexRequest(pub FileEntry);

struct IndexerWorker {
    i_recv: Receiver<IndexRequest>,
    d_send: Sender<Document>,
    schema: DocSchema,
}

impl IndexerWorker {
    fn go(self) {
        for IndexRequest(doc) in &self.i_recv {
            if let Some((modified, content)) = match doc.file_ext() {
                "txt" | "org" | "md" => self.index_text_doc(&doc),
                ext => {
                    eprintln!("Unknown ext: {}", ext);
                    continue;
                }
            } {
                let _ = self.d_send.send(doc!(
                    self.schema.full_path => doc.full_path(),
                    self.schema.filename => doc.file_name(),
                    self.schema.modified => modified,
                    self.schema.content => content,
                ));
            }
        }
    }

    fn index_text_doc(&self, doc: &FileEntry) -> Option<(u64, String)> {
        // TODO: eventually keep track of errors
        let f = fs::File::open(&doc.full_path).ok()?;

        let modified: u64 = f
            .metadata()
            .ok()?
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();

        let mut buf_reader = BufReader::new(f);
        let mut content = String::new();
        buf_reader.read_to_string(&mut content).ok()?;

        Some((modified, content))
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
            .into_iter()
            .map(|_| {
                let i_recv = index_recv.clone();
                let d_send = doc_send.clone();
                // 400 mb per thread seems alright :^)
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

        let doc_writer = indexer.writer(5_000_000).context(IndexTantivyError)?;

        let doc_consumer_thread = std::thread::spawn(move || {
            Self::do_doc_writes(doc_writer, doc_recv);
        });

        Ok(IndexerThreads {
            doc_processor_threads,
            doc_consumer_thread,
            index_sender: index_send,
        })
    }

    fn do_doc_writes(mut writer: tantivy::IndexWriter, d_recv: Receiver<Document>) {
        for doc in &d_recv {
            writer.add_document(doc);
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

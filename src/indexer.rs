// stuff for indexing files

use failure::{Compat, Fail}; // oh no
use snafu::{ResultExt, Snafu};
use tantivy;
use tantivy::schema::*;

use super::config;

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

pub struct DocIndexer {
    pub full_path: Field,
    pub filename: Field,
    pub content: Field,
    pub modified: Field,
    pub schema: Schema,
    pub indexer: Option<tantivy::Index>,
}

impl DocIndexer {
    pub fn new() -> DocIndexer {
        let mut schema_builder = Schema::builder();

        let full_path = schema_builder.add_text_field("full_path", STORED);
        let filename = schema_builder.add_text_field("filename", STRING | STORED);
        let content = schema_builder.add_text_field("content", TEXT | STORED);
        let modified = schema_builder.add_u64_field("last_modified", FAST | INDEXED | STORED);

        let schema = schema_builder.build();

        DocIndexer {
            full_path,
            filename,
            content,
            modified,
            schema,
            indexer: None,
        }
    }

    pub fn create_indexer(&mut self, config: &config::Config) -> Result<()> {
        std::fs::create_dir_all(&config.index_location).unwrap();
        let dir = tantivy::directory::MmapDirectory::open(&config.index_location)
            .context(IndexDirError)?;

        let index =
            tantivy::Index::open_or_create(dir, self.schema.clone()).context(IndexTantivyError)?;

        self.indexer = Some(index);

        Ok(())
    }
}

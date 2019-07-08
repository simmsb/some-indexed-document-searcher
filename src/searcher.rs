use tantivy::{collector::TopDocs, query::QueryParser, DocAddress, Index, IndexReader, Score};

pub struct Searcher {
    schema: super::indexer::DocSchema,
    index: Index,
    index_reader: IndexReader,
}

impl Searcher {
    pub fn new(schema: super::indexer::DocSchema, index: Index) -> Option<Self> {
        let index_reader = index.reader().ok()?;

        Some(Searcher {
            schema,
            index,
            index_reader,
        })
    }

    pub fn search(&self, search: &str) -> Option<Vec<String>> {
        let searcher = self.index_reader.searcher();
        let qp = QueryParser::for_index(&self.index, vec![self.schema.content()]);
        let q = qp.parse_query(search).ok()?;

        let top_docs: Vec<(Score, DocAddress)> =
            searcher.search(&q, &TopDocs::with_limit(10)).ok()?;

        top_docs
            .into_iter()
            .map(|(_, addr)| {
                let rd = searcher.doc(addr).ok()?;
                Some(rd.get_first(self.schema.full_path())?.text()?.to_owned())
            })
            .collect()
    }
}

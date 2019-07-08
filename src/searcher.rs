use tantivy::{collector::TopDocs, query::QueryParser, DocAddress, Index, IndexReader, Score, SnippetGenerator};

pub struct SearchResult {
    pub path: String,
    pub snippet: String,
}

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

    pub fn search(&self, search: &str) -> Option<Vec<SearchResult>> {
        let searcher = self.index_reader.searcher();
        let qp = QueryParser::for_index(&self.index, vec![self.schema.content()]);
        let q = qp.parse_query(search).ok()?;

        let top_docs: Vec<(Score, DocAddress)> =
            searcher.search(&q, &TopDocs::with_limit(10)).ok()?;

        let mut snippet_generator = SnippetGenerator::create(&searcher, &*q, self.schema.content()).ok()?;
        snippet_generator.set_max_num_chars(100);

        top_docs
            .into_iter()
            .map(|(_, addr)| {
                let doc = searcher.doc(addr).ok()?;
                let snippet = snippet_generator.snippet_from_doc(&doc);
                let snippet_html = snippet.to_html();
                let path = doc.get_first(self.schema.full_path())?.text()?.to_owned();
                Some(SearchResult { path, snippet: snippet_html })
            })
            .collect()
    }
}

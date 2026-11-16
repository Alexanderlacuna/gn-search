//! Document construction, mirroring `index_genes` / `index_phenotypes`
//! in `~/project/genenetwork3/scripts/index-genenetwork` call for call.
//!
//! The whole point of this module is *not* to improve on the indexer:
//! the TermGenerator configuration (English stemmer, default stemming
//! strategy — never set, `STOP_ALL` with an empty `SimpleStopper`),
//! the order of the `index_text` / `index_text_without_positions`
//! calls, the prefixes, the value slots, the popped-before-storage
//! fields and the identity term are reproduced exactly, because the
//! twin gate compares term sets, wdf, positions, slots and docdata
//! byte for byte.

use xapian_rs::{Document, SimpleStopper, Stem, StopStrategy, TermGenerator};

use super::cache::RdfCaches;
use super::pyjson::{JsonObject, JsonValue};
use super::rows::{GeneRow, PhenotypeRow};
use super::schema::{self, prefix, slot};
use super::sha256::hex_digest;

/// A document ready to write, its identity term and its source hash.
pub struct BuiltDocument {
    pub idterm: String,
    pub document: Document,
    pub source_hash: String,
}

/// The writer's TermGenerator, configured exactly as the indexer.
fn writer_termgen() -> TermGenerator {
    let mut termgen = TermGenerator::default();
    termgen.set_stemmer(Stem::for_language("en"));
    termgen.set_stopper_strategy(StopStrategy::All);
    termgen.set_stopper(SimpleStopper::new());
    termgen
}

/// Canonical encoder for a document's complete source, used for the
/// incremental change hash. Deterministic and delimiter-safe.
#[derive(Default)]
struct Canon {
    buffer: Vec<u8>,
}

impl Canon {
    fn field(&mut self, key: &str, value: &str) {
        self.buffer.extend_from_slice(key.as_bytes());
        self.buffer.push(b'=');
        self.buffer
            .extend_from_slice(value.len().to_string().as_bytes());
        self.buffer.push(b':');
        self.buffer.extend_from_slice(value.as_bytes());
        self.buffer.push(b'\n');
    }

    fn opt_str(&mut self, key: &str, value: &Option<String>) {
        match value {
            Some(text) => self.field(key, text),
            None => {
                self.buffer.extend_from_slice(key.as_bytes());
                self.buffer.extend_from_slice(b"=N\n");
            }
        }
    }

    fn opt_f64(&mut self, key: &str, value: &Option<f64>) {
        match value {
            Some(number) => self.field(key, &format!("{:016x}", number.to_bits())),
            None => {
                self.buffer.extend_from_slice(key.as_bytes());
                self.buffer.extend_from_slice(b"=N\n");
            }
        }
    }

    fn opt_i64(&mut self, key: &str, value: &Option<i64>) {
        match value {
            Some(number) => self.field(key, &number.to_string()),
            None => {
                self.buffer.extend_from_slice(key.as_bytes());
                self.buffer.extend_from_slice(b"=N\n");
            }
        }
    }
}

/// The stable identity term of a gene row, or `None` when name or
/// dataset is absent (the crawl then produces no document).
///
/// This is the cheap identity pre-pass: it computes exactly the
/// `name:dataset` term `build_gene` would write, without building a
/// document, so identity uniqueness can be validated before any write.
pub fn gene_idterm(row: &GeneRow) -> Option<String> {
    let (name, dataset) = (row.name.as_ref()?, row.dataset.as_ref()?);
    Some(schema::identity_term(schema::DOCTYPE_GENE, name, dataset))
}

/// The stable identity term of a phenotype row, or `None` when name or
/// dataset is absent. The integer name is stringified exactly as the
/// document builder does.
pub fn phenotype_idterm(row: &PhenotypeRow) -> Option<String> {
    let (name, dataset) = (row.name.as_ref()?, row.dataset.as_ref()?);
    Some(schema::identity_term(
        schema::DOCTYPE_PHENOTYPE,
        &name.to_string(),
        dataset,
    ))
}

/// Build a gene document. `None` when name or dataset is absent (the
/// indexer then writes no document at all).
pub fn build_gene(row: &GeneRow, caches: &RdfCaches) -> Option<BuiltDocument> {
    let idterm = gene_idterm(row)?;

    let mut doc = Document::default();
    {
        let mut termgen = writer_termgen();
        termgen.set_document(&doc);

        // Values (slots 0,1,2,3,4).
        if let Some(mean) = row.mean {
            doc.set_value(slot::MEAN, mean);
        }
        if let Some(lrs) = row.lrs {
            doc.set_value(slot::PEAK, lrs);
        }
        if let Some(mb) = row.mb {
            doc.set_value(slot::POSITION, mb);
        }
        if let Some(geno_mb) = row.geno_mb {
            doc.set_value(slot::PEAK_MB, geno_mb);
        }
        if let Some(additive) = row.additive {
            doc.set_value(slot::ADDITIVE, additive);
        }

        // Free text WITH positions.
        for text in [&row.description, &row.tissue, &row.dataset] {
            if let Some(text) = text {
                termgen.index_text::<&str>(text, None, None);
                termgen.increase_termpos(None);
            }
        }
        if let Some(text) = &row.probe_target_description {
            termgen.index_text::<&str>(text, None, None);
            termgen.increase_termpos(None);
        }
        // Free text WITHOUT positions, unprefixed.
        for text in [&row.name, &row.symbol, &row.species, &row.group] {
            if let Some(text) = text {
                termgen.index_text_without_positions::<&str>(text, None, None);
            }
        }
        for text in [&row.alias, &row.genbankid, &row.unigeneid] {
            if let Some(text) = text {
                termgen.index_text_without_positions::<&str>(text, None, None);
            }
        }
        // Prefixed: WITH positions for XI/XDS, without for the rest.
        if let Some(text) = &row.species {
            termgen.index_text_without_positions::<&str>(text, Some(0u32), Some(prefix::SPECIES));
        }
        if let Some(text) = &row.group {
            termgen.index_text_without_positions::<&str>(text, Some(0u32), Some(prefix::GROUP));
        }
        if let Some(text) = &row.tissue {
            termgen.index_text::<&str>(text, Some(0u32), Some(prefix::TISSUE));
        }
        if let Some(text) = &row.dataset {
            termgen.index_text::<&str>(text, Some(0u32), Some(prefix::DATASET));
        }
        if let Some(text) = &row.symbol {
            termgen.index_text_without_positions::<&str>(text, Some(0u32), Some(prefix::SYMBOL));
        }
        if let Some(text) = &row.chr {
            termgen.index_text_without_positions::<&str>(text, Some(0u32), Some(prefix::CHR));
        }
        if let Some(text) = &row.geno_chr {
            termgen.index_text_without_positions::<&str>(text, Some(0u32), Some(prefix::PEAK_CHR));
        }
        // RDF dictionaries (XRF / XWK), keyed on (species, symbol).
        if let (Some(species), Some(symbol)) = (&row.species, &row.symbol) {
            if let Some(text) = caches.rif.get(species, symbol) {
                termgen.index_text_without_positions::<&str>(text, Some(0u32), Some(prefix::RIF));
            }
            if let Some(text) = caches.wiki.get(species, symbol) {
                termgen.index_text_without_positions::<&str>(text, Some(0u32), Some(prefix::WIKI));
            }
        }
    }

    // Stored data: insertion order minus the popped fields.
    let mut data = JsonObject::new();
    push_opt_str(&mut data, "name", &row.name);
    push_opt_str(&mut data, "symbol", &row.symbol);
    push_opt_str(&mut data, "description", &row.description);
    push_opt_str(&mut data, "chr", &row.chr);
    push_opt_f64(&mut data, "mb", &row.mb);
    push_opt_str(&mut data, "dataset", &row.dataset);
    push_opt_str(&mut data, "dataset_fullname", &row.dataset_fullname);
    push_opt_str(&mut data, "species", &row.species);
    push_opt_str(&mut data, "group", &row.group);
    push_opt_str(&mut data, "tissue", &row.tissue);
    push_opt_f64(&mut data, "mean", &row.mean);
    push_opt_f64(&mut data, "lrs", &row.lrs);
    push_opt_f64(&mut data, "additive", &row.additive);
    push_opt_str(&mut data, "geno_chr", &row.geno_chr);
    push_opt_f64(&mut data, "geno_mb", &row.geno_mb);
    doc.set_data(data.to_string());

    doc.add_boolean_term(schema::type_term(schema::DOCTYPE_GENE));
    doc.add_boolean_term(&idterm);

    // Source hash: full row + the monolithic cache hashes.
    let mut canon = Canon::default();
    canon.opt_str("name", &row.name);
    canon.opt_str("symbol", &row.symbol);
    canon.opt_str("description", &row.description);
    canon.opt_str("chr", &row.chr);
    canon.opt_f64("mb", &row.mb);
    canon.opt_str("alias", &row.alias);
    canon.opt_str("genbankid", &row.genbankid);
    canon.opt_str("unigeneid", &row.unigeneid);
    canon.opt_str("probe_target_description", &row.probe_target_description);
    canon.opt_str("dataset", &row.dataset);
    canon.opt_str("dataset_fullname", &row.dataset_fullname);
    canon.opt_str("species", &row.species);
    canon.opt_str("group", &row.group);
    canon.opt_str("tissue", &row.tissue);
    canon.opt_f64("mean", &row.mean);
    canon.opt_f64("lrs", &row.lrs);
    canon.opt_f64("additive", &row.additive);
    canon.opt_str("geno_chr", &row.geno_chr);
    canon.opt_f64("geno_mb", &row.geno_mb);
    canon.field("rif-cache", &caches.rif.hash);
    canon.field("wiki-cache", &caches.wiki.hash);

    Some(BuiltDocument {
        idterm,
        document: doc,
        source_hash: hex_digest(&canon.buffer),
    })
}

/// Build a phenotype document.
pub fn build_phenotype(row: &PhenotypeRow) -> Option<BuiltDocument> {
    // Python converts the integer name to str for the document (and for
    // the identity term's identifier).
    let name_text = row.name.as_ref()?.to_string();
    let idterm = phenotype_idterm(row)?;

    // The raw comma-separated authors string is indexed before storage.
    let authors_raw = row.authors.clone();
    let authors_list: Option<Vec<String>> = authors_raw
        .as_ref()
        .map(|text| text.split(',').map(|a| a.trim().to_string()).collect());

    let mut doc = Document::default();
    {
        let mut termgen = writer_termgen();
        termgen.set_document(&doc);

        // Values (slots 0,1,3,4,5,6 — no position slot).
        if let Some(mean) = row.mean {
            doc.set_value(slot::MEAN, mean);
        }
        if let Some(lrs) = row.lrs {
            doc.set_value(slot::PEAK, lrs);
        }
        if let Some(geno_mb) = row.geno_mb {
            doc.set_value(slot::PEAK_MB, geno_mb);
        }
        if let Some(additive) = row.additive {
            doc.set_value(slot::ADDITIVE, additive);
        }
        if let Some(year) = row.year {
            doc.set_value(slot::YEAR, year as f64);
        }
        if let Some(nsamples) = row.nsamples {
            doc.set_value(slot::NSAMPLES, nsamples as f64);
        }

        // Free text WITH positions.
        for text in [&row.description, &authors_raw, &row.dataset] {
            if let Some(text) = text {
                termgen.index_text::<&str>(text, None, None);
                termgen.increase_termpos(None);
            }
        }
        for text in [&row.abstract_, &row.title] {
            if let Some(text) = text {
                termgen.index_text::<&str>(text, None, None);
                termgen.increase_termpos(None);
            }
        }
        // Free text WITHOUT positions, unprefixed.
        for text in [&row.species, &row.group, &row.inbredsetcode] {
            if let Some(text) = text {
                termgen.index_text_without_positions::<&str>(text, None, None);
            }
        }
        for text in [&row.abbreviation, &row.lab_code] {
            if let Some(text) = text {
                termgen.index_text_without_positions::<&str>(text, None, None);
            }
        }
        // Prefixed: WITH positions for A/XDS.
        if let Some(text) = &row.species {
            termgen.index_text_without_positions::<&str>(text, Some(0u32), Some(prefix::SPECIES));
        }
        if let Some(text) = &row.group {
            termgen.index_text_without_positions::<&str>(text, Some(0u32), Some(prefix::GROUP));
        }
        if let Some(text) = &authors_raw {
            termgen.index_text::<&str>(text, Some(0u32), Some(prefix::AUTHOR));
        }
        if let Some(text) = &row.geno_chr {
            termgen.index_text_without_positions::<&str>(text, Some(0u32), Some(prefix::PEAK_CHR));
        }
        if let Some(text) = &row.dataset {
            termgen.index_text::<&str>(text, Some(0u32), Some(prefix::DATASET));
        }
    }

    let mut data = JsonObject::new();
    push_opt_str(&mut data, "species", &row.species);
    push_opt_str(&mut data, "group", &row.group);
    push_opt_str(&mut data, "dataset", &row.dataset);
    push_opt_str(&mut data, "dataset_fullname", &row.dataset_fullname);
    data.push("name", JsonValue::Str(name_text.clone()));
    push_opt_str(&mut data, "description", &row.description);
    if let Some(authors) = &authors_list {
        data.push("authors", JsonValue::StrList(authors.clone()));
    }
    push_opt_i64(&mut data, "year", &row.year);
    push_opt_i64(&mut data, "pubmed_id", &row.pubmed_id);
    push_opt_f64(&mut data, "lrs", &row.lrs);
    push_opt_f64(&mut data, "additive", &row.additive);
    push_opt_str(&mut data, "inbredsetcode", &row.inbredsetcode);
    push_opt_f64(&mut data, "mean", &row.mean);
    push_opt_i64(&mut data, "nsamples", &row.nsamples);
    push_opt_str(&mut data, "geno_chr", &row.geno_chr);
    push_opt_f64(&mut data, "geno_mb", &row.geno_mb);
    doc.set_data(data.to_string());

    doc.add_boolean_term(schema::type_term(schema::DOCTYPE_PHENOTYPE));
    doc.add_boolean_term(&idterm);

    let mut canon = Canon::default();
    canon.opt_str("species", &row.species);
    canon.opt_str("group", &row.group);
    canon.opt_str("dataset", &row.dataset);
    canon.opt_str("dataset_fullname", &row.dataset_fullname);
    canon.opt_i64("name", &row.name);
    canon.opt_str("abbreviation", &row.abbreviation);
    canon.opt_str("description", &row.description);
    canon.opt_str("Lab_code", &row.lab_code);
    canon.opt_str("Abstract", &row.abstract_);
    canon.opt_str("Title", &row.title);
    canon.opt_str("authors", &row.authors);
    canon.opt_i64("year", &row.year);
    canon.opt_i64("pubmed_id", &row.pubmed_id);
    canon.opt_f64("lrs", &row.lrs);
    canon.opt_f64("additive", &row.additive);
    canon.opt_str("inbredsetcode", &row.inbredsetcode);
    canon.opt_f64("mean", &row.mean);
    canon.opt_i64("nsamples", &row.nsamples);
    canon.opt_str("geno_chr", &row.geno_chr);
    canon.opt_f64("geno_mb", &row.geno_mb);

    Some(BuiltDocument {
        idterm,
        document: doc,
        source_hash: hex_digest(&canon.buffer),
    })
}

fn push_opt_str(object: &mut JsonObject, key: &str, value: &Option<String>) {
    if let Some(text) = value {
        object.push(key, JsonValue::Str(text.clone()));
    }
}

fn push_opt_f64(object: &mut JsonObject, key: &str, value: &Option<f64>) {
    if let Some(number) = value {
        object.push(key, JsonValue::Float(*number));
    }
}

fn push_opt_i64(object: &mut JsonObject, key: &str, value: &Option<i64>) {
    if let Some(number) = value {
        object.push(key, JsonValue::Int(*number));
    }
}

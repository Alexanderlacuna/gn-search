//! The crawl: the SQL queries and typed row structs the document
//! builder consumes, streamed and grouped by dataset.
//!
//! The queries and joins are the indexer's own (`genes_query` and
//! `phenotypes_query` in index-genenetwork), rendered by its
//! `serialize_sql`. The one deliberate difference is the phenotype
//! filter: the Python query has NO `WHERE` clause, which is the leak
//! this lane closes by construction — both queries now carry
//! `confidentiality < 1 AND public > 0`.
//!
//! The phenotype join fans out on `InbredSetId` only, so a group's two
//! PublishFreezes (e.g. BXDPublish and GITrMetPublish) each index every
//! trait under their own dataset label. This cross-product is INTENTIONAL
//! reference semantics and is reproduced, not "cleaned up".
//!
//! # Streaming, dataset-grouped crawl
//!
//! [`stream_dataset_groups`] never materialises the crawl. Each query is
//! executed in its own producer thread with `ORDER BY dataset COLLATE
//! utf8mb4_bin`; the merge consumes the two sorted streams and hands the
//! caller **one dataset at a time** through a [`DatasetSink`], whose
//! `begin_dataset` / `row` / `end_dataset` callbacks see rows one at a
//! time. The only per-dataset state the caller keeps is bounded by the
//! largest dataset, not by the whole crawl.
//!
//! The `Name` columns are `utf8mb4_general_ci`, which is *case-insensitive*.
//! Ordering by that collation can interleave the rows of two labels that
//! differ only by case, breaking the "each label's rows are contiguous"
//! assumption the byte-order merge below relies on. Pinning
//! `COLLATE utf8mb4_bin` makes the SQL order match the merge's byte order.

use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, Sender};

use mysql::prelude::Queryable;
use mysql::{Conn, Opts, OptsBuilder, Row};

use super::delta;

/// The gene filter, fully qualified against the (resolved) ProbeSetFreeze.
fn gene_public_filter() -> String {
    let psf = delta::table_name("ProbeSetFreeze");
    format!("{psf}.confidentiality < 1 AND {psf}.public > 0")
}

/// The phenotype filter, fully qualified against the (resolved)
/// PublishFreeze.
fn phenotype_public_filter() -> String {
    let pf = delta::table_name("PublishFreeze");
    format!("{pf}.confidentiality < 1 AND {pf}.public > 0")
}

/// One expression (gene) crawl row; each field is absent when SQL
/// returned NULL (the indexer's `MonadicDict` drops NULLs).
#[derive(Default, Clone)]
pub struct GeneRow {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub description: Option<String>,
    pub chr: Option<String>,
    pub mb: Option<f64>,
    pub alias: Option<String>,
    pub genbankid: Option<String>,
    pub unigeneid: Option<String>,
    pub probe_target_description: Option<String>,
    pub dataset: Option<String>,
    pub dataset_fullname: Option<String>,
    pub species: Option<String>,
    pub group: Option<String>,
    pub tissue: Option<String>,
    pub mean: Option<f64>,
    pub lrs: Option<f64>,
    pub additive: Option<f64>,
    pub geno_chr: Option<String>,
    pub geno_mb: Option<f64>,
}

/// One published (phenotype) crawl row.
#[derive(Default, Clone)]
pub struct PhenotypeRow {
    pub species: Option<String>,
    pub group: Option<String>,
    pub dataset: Option<String>,
    pub dataset_fullname: Option<String>,
    pub name: Option<i64>,
    pub abbreviation: Option<String>,
    pub description: Option<String>,
    pub lab_code: Option<String>,
    pub abstract_: Option<String>,
    pub title: Option<String>,
    pub authors: Option<String>,
    pub year: Option<i64>,
    pub pubmed_id: Option<i64>,
    pub lrs: Option<f64>,
    pub additive: Option<f64>,
    pub inbredsetcode: Option<String>,
    pub mean: Option<f64>,
    pub nsamples: Option<i64>,
    pub geno_chr: Option<String>,
    pub geno_mb: Option<f64>,
}

/// One crawled row, tagged with its document type. The dataset label is
/// the row's dataset column — never parsed back out of the identity
/// term, whose `name` part may itself contain a colon.
pub enum CrawlRow {
    Gene(GeneRow),
    Phenotype(PhenotypeRow),
}

impl CrawlRow {
    /// The row's dataset label (the shard key).
    pub fn dataset(&self) -> Option<&str> {
        match self {
            CrawlRow::Gene(row) => row.dataset.as_deref(),
            CrawlRow::Phenotype(row) => row.dataset.as_deref(),
        }
    }
}

/// Receive one dataset group at a time from [`stream_dataset_groups`].
pub trait DatasetSink {
    /// Called once when a dataset group starts, before any of its rows.
    fn begin_dataset(&mut self, label: &str) -> Result<(), String>;
    /// Called for each row of the current dataset, in arrival order.
    fn row(&mut self, row: CrawlRow) -> Result<(), String>;
    /// Called once after the last row of the dataset.
    fn end_dataset(&mut self, label: &str) -> Result<(), String>;
}

pub(crate) fn connect(sql_uri: &str) -> Result<Conn, String> {
    let opts = Opts::from_url(sql_uri).map_err(|e| format!("bad SQL_URI: {e}"))?;
    Conn::new(OptsBuilder::from_opts(opts)).map_err(|e| format!("cannot connect: {e}"))
}

fn text(row: &Row, column: &str) -> Result<Option<String>, String> {
    match row.get_opt::<Option<String>, _>(column) {
        Some(Ok(value)) => Ok(value),
        Some(Err(e)) => Err(format!("column {column}: {e}")),
        None => Err(format!("column {column} not in result set")),
    }
}

fn float(row: &Row, column: &str) -> Result<Option<f64>, String> {
    match row.get_opt::<Option<f64>, _>(column) {
        Some(Ok(value)) => Ok(value),
        Some(Err(e)) => Err(format!("column {column}: {e}")),
        None => Err(format!("column {column} not in result set")),
    }
}

fn integer(row: &Row, column: &str) -> Result<Option<i64>, String> {
    match row.get_opt::<Option<i64>, _>(column) {
        Some(Ok(value)) => Ok(value),
        Some(Err(e)) => Err(format!("column {column}: {e}")),
        None => Err(format!("column {column} not in result set")),
    }
}

fn genes_sql(datasets: Option<&BTreeSet<String>>) -> String {
    let species = delta::table_name("Species");
    let inbredset = delta::table_name("InbredSet");
    // ProbeFreeze and Tissue are contributing but NOT versioned, so they
    // are never prefixed (the joins by `Id` resolve against the real tables).
    let probefreeze = "ProbeFreeze";
    let tissue = "Tissue";
    let psf = delta::table_name("ProbeSetFreeze");
    let psx = delta::table_name("ProbeSetXRef");
    let probeset = delta::table_name("ProbeSet");
    // The scoped filter (delta path): restrict the crawl to the affected
    // dataset labels, compared with the same binary collation the stream
    // groups by. `None` is the unscoped FULL/INCREMENTAL crawl.
    let scope = match datasets {
        Some(labels) => format!(
            " AND {}",
            delta::label_in_clause(&format!("{psf}.Name COLLATE utf8mb4_bin"), labels)
        ),
        None => String::new(),
    };
    let public_filter = gene_public_filter();
    format!(
        "SELECT {probeset}.Name AS name, {probeset}.Symbol AS symbol, \
         {probeset}.description AS description, {probeset}.Chr AS chr, {probeset}.Mb as mb, \
         {probeset}.alias AS alias, {probeset}.GenbankId AS genbankid, \
         {probeset}.UniGeneId AS unigeneid, \
         {probeset}.Probe_Target_Description AS probe_target_description, \
         {psf}.Name AS dataset, {psf}.FullName AS dataset_fullname, \
         {species}.Name AS species, {inbredset}.Name AS `group`, {tissue}.Name AS tissue, \
         {psx}.Mean AS mean, {psx}.LRS AS lrs, \
         {psx}.additive AS additive, Geno.Chr AS geno_chr, Geno.Mb as geno_mb \
         FROM {species} \
         INNER JOIN {inbredset} ON {inbredset}.SpeciesId = {species}.Id \
         INNER JOIN {probefreeze} ON {probefreeze}.InbredSetId = {inbredset}.Id \
         INNER JOIN {tissue} ON {probefreeze}.TissueId = {tissue}.Id \
         INNER JOIN {psf} ON {psf}.ProbeFreezeId = {probefreeze}.Id \
         INNER JOIN {psx} ON {psx}.ProbeSetFreezeId = {psf}.Id \
         INNER JOIN {probeset} ON {probeset}.Id = {psx}.ProbeSetId \
         LEFT JOIN Geno ON {psx}.Locus = Geno.Name AND Geno.SpeciesId = {species}.Id \
         WHERE {public_filter}{scope} \
         ORDER BY dataset COLLATE utf8mb4_bin"
    )
}

fn phenotypes_sql(filtered: bool, datasets: Option<&BTreeSet<String>>) -> String {
    let species = delta::table_name("Species");
    let inbredset = delta::table_name("InbredSet");
    let pf = delta::table_name("PublishFreeze");
    let pubxref = delta::table_name("PublishXRef");
    let scope = match datasets {
        Some(labels) => format!(
            " AND {}",
            delta::label_in_clause(&format!("{pf}.Name COLLATE utf8mb4_bin"), labels)
        ),
        None => String::new(),
    };
    let where_clause = if filtered {
        format!(" WHERE {}", phenotype_public_filter())
    } else {
        String::new()
    };
    format!(
        "SELECT {species}.Name AS species, {inbredset}.Name AS `group`, \
         {pf}.Name AS dataset, {pf}.FullName AS dataset_fullname, \
         {pubxref}.Id AS name, \
         COALESCE(Phenotype.Post_publication_abbreviation, \
                  Phenotype.Pre_publication_abbreviation) AS abbreviation, \
         COALESCE(Phenotype.Post_publication_description, \
                  Phenotype.Pre_publication_description) AS description, \
         Phenotype.Lab_code, Publication.Abstract, Publication.Title, \
         Publication.Authors AS authors, \
         IF(CONVERT(Publication.Year, UNSIGNED)=0, NULL, \
            CONVERT(Publication.Year, UNSIGNED)) AS year, \
         Publication.PubMed_ID AS pubmed_id, {pubxref}.LRS as lrs, \
         {pubxref}.additive, {inbredset}.InbredSetCode AS inbredsetcode, \
         {pubxref}.mean, {pubxref}.NSamples AS nsamples, \
         Geno.Chr as geno_chr, Geno.Mb as geno_mb \
         FROM {species} \
         INNER JOIN {inbredset} ON {inbredset}.SpeciesId = {species}.Id \
         INNER JOIN {pf} ON {pf}.InbredSetId = {inbredset}.Id \
         INNER JOIN {pubxref} ON {pubxref}.InbredSetId = {inbredset}.Id \
         INNER JOIN Phenotype ON {pubxref}.PhenotypeId = Phenotype.Id \
         INNER JOIN Publication ON {pubxref}.PublicationId = Publication.Id \
         LEFT JOIN Geno ON {pubxref}.Locus = Geno.Name AND Geno.SpeciesId = {species}.Id\
         {where_clause}{scope} \
         ORDER BY dataset COLLATE utf8mb4_bin"
    )
}

type Item = Result<(String, CrawlRow), String>;

/// Producer thread for the gene crawl: stream rows ordered by dataset and
/// send each as `(dataset, CrawlRow)`. Rows with no dataset produce no
/// document and are skipped.
fn produce_genes(
    sql_uri: &str,
    datasets: Option<BTreeSet<String>>,
    tx: &Sender<Item>,
) -> Result<(), String> {
    let mut conn = connect(sql_uri)?;
    let result = conn
        .query_iter(genes_sql(datasets.as_ref()))
        .map_err(|e| format!("gene crawl failed: {e}"))?;
    for row in result {
        let row = row.map_err(|e| format!("gene crawl failed: {e}"))?;
        let Some(dataset) = text(&row, "dataset")? else {
            continue;
        };
        let row = GeneRow {
            name: text(&row, "name")?,
            symbol: text(&row, "symbol")?,
            description: text(&row, "description")?,
            chr: text(&row, "chr")?,
            mb: float(&row, "mb")?,
            alias: text(&row, "alias")?,
            genbankid: text(&row, "genbankid")?,
            unigeneid: text(&row, "unigeneid")?,
            probe_target_description: text(&row, "probe_target_description")?,
            dataset: Some(dataset.clone()),
            dataset_fullname: text(&row, "dataset_fullname")?,
            species: text(&row, "species")?,
            group: text(&row, "group")?,
            tissue: text(&row, "tissue")?,
            mean: float(&row, "mean")?,
            lrs: float(&row, "lrs")?,
            additive: float(&row, "additive")?,
            geno_chr: text(&row, "geno_chr")?,
            geno_mb: float(&row, "geno_mb")?,
        };
        if tx.send(Ok((dataset, CrawlRow::Gene(row)))).is_err() {
            return Ok(());
        }
    }
    Ok(())
}

/// Producer thread for the phenotype crawl.
fn produce_phenotypes(
    sql_uri: &str,
    filtered: bool,
    datasets: Option<BTreeSet<String>>,
    tx: &Sender<Item>,
) -> Result<(), String> {
    let mut conn = connect(sql_uri)?;
    let result = conn
        .query_iter(phenotypes_sql(filtered, datasets.as_ref()))
        .map_err(|e| format!("phenotype crawl failed: {e}"))?;
    for row in result {
        let row = row.map_err(|e| format!("phenotype crawl failed: {e}"))?;
        let Some(dataset) = text(&row, "dataset")? else {
            continue;
        };
        let row = PhenotypeRow {
            species: text(&row, "species")?,
            group: text(&row, "group")?,
            dataset: Some(dataset.clone()),
            dataset_fullname: text(&row, "dataset_fullname")?,
            name: integer(&row, "name")?,
            abbreviation: text(&row, "abbreviation")?,
            description: text(&row, "description")?,
            lab_code: text(&row, "Lab_code")?,
            abstract_: text(&row, "Abstract")?,
            title: text(&row, "Title")?,
            authors: text(&row, "authors")?,
            year: integer(&row, "year")?,
            pubmed_id: integer(&row, "pubmed_id")?,
            lrs: float(&row, "lrs")?,
            additive: float(&row, "additive")?,
            inbredsetcode: text(&row, "inbredsetcode")?,
            mean: float(&row, "mean")?,
            nsamples: integer(&row, "nsamples")?,
            geno_chr: text(&row, "geno_chr")?,
            geno_mb: float(&row, "geno_mb")?,
        };
        if tx.send(Ok((dataset, CrawlRow::Phenotype(row)))).is_err() {
            return Ok(());
        }
    }
    Ok(())
}

fn spawn_producer<F>(tx: Sender<Item>, body: F) -> std::thread::JoinHandle<()>
where
    F: FnOnce(&Sender<Item>) -> Result<(), String> + Send + 'static,
{
    std::thread::spawn(move || {
        if let Err(error) = body(&tx) {
            let _ = tx.send(Err(error));
        }
    })
}

/// The dataset label at the head of a channel, propagating a producer
/// error. `Ok(None)` means the channel is exhausted.
fn head_label(head: &Option<Item>) -> Result<Option<&str>, String> {
    match head {
        None => Ok(None),
        Some(Ok((label, _))) => Ok(Some(label.as_str())),
        Some(Err(error)) => Err(error.clone()),
    }
}

/// Stream the crawl as dataset groups. Two producer threads (genes and
/// phenotypes) each return rows ordered by dataset; this merge consumes
/// them in dataset order and invokes `sink` once per dataset. No whole-
/// crawl buffer exists: at most one pending row per source is held.
pub fn stream_dataset_groups(
    sql_uri: &str,
    filtered: bool,
    sink: &mut dyn DatasetSink,
) -> Result<(), String> {
    stream_dataset_groups_scoped(sql_uri, filtered, None, sink)
}

/// The dataset-scoped crawl: like [`stream_dataset_groups`], but the SQL
/// restricts both producers to the affected `datasets` (compared with
/// `COLLATE utf8mb4_bin`), so labels outside the set cannot appear. This
/// is the DELTA write path's crawl; `None` reproduces the unscoped FULL/
/// INCREMENTAL crawl exactly.
pub fn stream_dataset_groups_scoped(
    sql_uri: &str,
    filtered: bool,
    datasets: Option<&BTreeSet<String>>,
    sink: &mut dyn DatasetSink,
) -> Result<(), String> {
    // Unbounded channels: the producers stream the two SQL result sets in
    // their own threads and the merge consumes them; this is the fast
    // path (a bounded channel was tried and made the scaled run several
    // times slower, so it was reverted).
    let (gene_tx, gene_rx) = std::sync::mpsc::channel::<Item>();
    let (phen_tx, phen_rx) = std::sync::mpsc::channel::<Item>();
    let gene_uri = sql_uri.to_string();
    let phen_uri = sql_uri.to_string();
    let gene_datasets = datasets.cloned();
    let phen_datasets = datasets.cloned();
    let genes = spawn_producer(gene_tx, move |tx| {
        produce_genes(&gene_uri, gene_datasets, tx)
    });
    let phenotypes = spawn_producer(phen_tx, move |tx| {
        produce_phenotypes(&phen_uri, filtered, phen_datasets, tx)
    });

    let outcome = merge_streams(&gene_rx, &phen_rx, sink);

    // Producers end when the receivers drop, but join for a clean exit.
    let _ = genes.join();
    let _ = phenotypes.join();
    outcome
}

fn merge_streams(
    gene_rx: &Receiver<Item>,
    phen_rx: &Receiver<Item>,
    sink: &mut dyn DatasetSink,
) -> Result<(), String> {
    let mut gene_head = gene_rx.recv().ok();
    let mut phen_head = phen_rx.recv().ok();
    loop {
        let gene_label = head_label(&gene_head)?;
        let phen_label = head_label(&phen_head)?;
        let label = match (gene_label, phen_label) {
            (None, None) => break,
            (Some(label), None) => label.to_string(),
            (None, Some(label)) => label.to_string(),
            (Some(gene), Some(phen)) => {
                if gene <= phen {
                    gene.to_string()
                } else {
                    phen.to_string()
                }
            }
        };
        sink.begin_dataset(&label)?;
        drain_label(&mut gene_head, gene_rx, &label, sink)?;
        drain_label(&mut phen_head, phen_rx, &label, sink)?;
        sink.end_dataset(&label)?;
    }
    Ok(())
}

/// Consume every head row whose dataset equals `label`, advancing the
/// channel. Rows for later datasets stay as the new head.
fn drain_label(
    head: &mut Option<Item>,
    rx: &Receiver<Item>,
    label: &str,
    sink: &mut dyn DatasetSink,
) -> Result<(), String> {
    loop {
        match head {
            Some(Ok((row_label, _))) if row_label == label => {
                let Some(Ok((_, row))) = head.take() else {
                    unreachable!("head was checked Some(Ok)")
                };
                sink.row(row)?;
                *head = rx.recv().ok();
            }
            Some(Err(error)) => return Err(error.clone()),
            _ => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::delta;

    /// The stream order must match the byte order `merge_streams` compares
    /// with. The `Name` columns are `utf8mb4_general_ci`; without the
    /// binary collation a case-variant label could interleave and split a
    /// dataset group across two `begin_dataset` calls.
    #[test]
    fn crawl_orders_datasets_bin_collated() {
        for sql in [
            genes_sql(None),
            phenotypes_sql(true, None),
            phenotypes_sql(false, None),
        ] {
            assert!(
                sql.contains("ORDER BY dataset COLLATE utf8mb4_bin"),
                "crawl must order by the binary collation: {sql}"
            );
        }
    }

    /// Unscoped SQL (the production FULL/INCREMENTAL shape) names the base
    /// tables and carries no prefix or dataset filter.
    #[test]
    fn unscoped_crawl_has_no_prefix_and_no_scope_filter() {
        delta::set_table_prefix_override(None);
        for sql in [
            genes_sql(None),
            phenotypes_sql(true, None),
            phenotypes_sql(false, None),
        ] {
            assert!(
                !sql.contains("vdelta_"),
                "unscoped SQL must not be prefixed: {sql}"
            );
            assert!(
                sql.contains("FROM Species"),
                "unscoped SQL uses base names: {sql}"
            );
            assert!(
                !sql.contains("COLLATE utf8mb4_bin IN ("),
                "unscoped SQL has no dataset IN filter: {sql}"
            );
        }
    }

    /// The scoped crawl appends the affected-label filter with the binary
    /// collation, on the *dataset-owner* table, and honours the prefix seam
    /// for the versioned tables (A2/A4/A5).
    #[test]
    fn scoped_crawl_filters_by_dataset_with_binary_collation() {
        delta::set_table_prefix_override(None);
        let labels: BTreeSet<String> = ["BXDPublish".to_string(), "O'Brien".to_string()]
            .into_iter()
            .collect();

        let genes = genes_sql(Some(&labels));
        assert!(
            genes.contains("ProbeSetFreeze.Name COLLATE utf8mb4_bin IN ("),
            "gene scope filter: {genes}"
        );
        assert!(genes.contains("'O''Brien'"), "label escaping (A5): {genes}");
        assert!(
            genes.contains("WHERE ProbeSetFreeze.confidentiality"),
            "{genes}"
        );

        let phen = phenotypes_sql(true, Some(&labels));
        assert!(
            phen.contains("PublishFreeze.Name COLLATE utf8mb4_bin IN ("),
            "phenotype scope filter: {phen}"
        );
        assert!(phen.contains(" AND PublishFreeze.Name COLLATE"), "{phen}");

        // Prefix seam: both the FROM tables and the dataset filter move to
        // the vdelta_ namespace, so delta and crawl agree (A4).
        delta::set_table_prefix_override(Some("vdelta_".to_string()));
        let genes = genes_sql(Some(&labels));
        assert!(genes.contains("FROM vdelta_Species"), "{genes}");
        assert!(
            genes.contains("INNER JOIN vdelta_ProbeSetFreeze"),
            "{genes}"
        );
        assert!(
            genes.contains("vdelta_ProbeSetFreeze.Name COLLATE utf8mb4_bin IN ("),
            "{genes}"
        );
        assert!(
            genes.contains("WHERE vdelta_ProbeSetFreeze.confidentiality"),
            "{genes}"
        );
        // Non-versioned contributing tables stay unprefixed.
        assert!(genes.contains("LEFT JOIN Geno ON"), "{genes}");
        delta::set_table_prefix_override(None);
    }
}

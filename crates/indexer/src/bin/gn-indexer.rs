//! `gn-indexer`: the GeneNetwork4 native search-index builder.
//!
//! Usage:
//!   gn-indexer full        [--index PATH] [--rdf-cache PATH] [--no-publish-filter]
//!   gn-indexer incremental [--index PATH] [--rdf-cache PATH] [--no-publish-filter]
//!   gn-indexer delta       [--index PATH] [--rdf-cache PATH] [--no-publish-filter]
//!
//! The target index comes from `--index`, else `XAPIAN_DB_PATH` (the
//! same key the search engine reads), and is written IN PLACE. The RDF
//! caches come from `--rdf-cache`, else `GN4_RDF_CACHE`, else
//! `/tmp/gn4-rdf-cache.json`. `SQL_URI` comes from the crate's normal
//! layered configuration.
//!
//! `delta` names changed rows via system versioning and re-diffs only the
//! affected datasets; it falls back to a FULL rebuild on any doubt. The
//! periodic `incremental` sweep remains the correctness bound.
//!
//! Test-only RED controls for the incremental gates are read from
//! `GN4_INDEXER_RED` (values `hash`, `delete`, `commit`); they sabotage
//! one guarantee each and are never set in a normal run.

use std::path::PathBuf;
use std::process::ExitCode;

use gn_search_indexer::config::Config;
use gn_search_indexer::{RunConfig, run_delta, run_full, run_incremental};

fn usage() -> ExitCode {
    eprintln!(
        "usage: gn-indexer <full|incremental|delta> [--index PATH] [--rdf-cache PATH] \
         [--no-publish-filter]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(mode) = args.next() else {
        return usage();
    };
    let mut index_arg: Option<PathBuf> = None;
    let mut rdf_arg: Option<PathBuf> = None;
    let mut publish_filter = true;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--index" => index_arg = args.next().map(PathBuf::from),
            "--rdf-cache" => rdf_arg = args.next().map(PathBuf::from),
            "--no-publish-filter" => publish_filter = false,
            other => {
                eprintln!("unknown argument: {other}");
                return usage();
            }
        }
    }

    let config = Config::load();
    let index_path = index_arg.unwrap_or_else(|| PathBuf::from(&config.xapian_db_path));
    if index_path.as_os_str().is_empty() {
        eprintln!("no index path: pass --index or set XAPIAN_DB_PATH");
        return ExitCode::from(1);
    }
    let rdf_cache = rdf_arg.unwrap_or_else(|| {
        PathBuf::from(
            std::env::var("GN4_RDF_CACHE").unwrap_or_else(|_| "/tmp/gn4-rdf-cache.json".into()),
        )
    });

    let run_config = RunConfig {
        sql_uri: config.sql_uri.clone(),
        index_path,
        rdf_cache,
        publish_filter,
    };

    // Fail loudly BEFORE touching the index when the database is
    // unreachable: the crawl needs it, and a run must never build an
    // empty index. The message names SQL_URI/GN4_DB_MODE so the cause is
    // unambiguous. (The crawl itself also fails, but this makes the
    // reason explicit and keeps the index untouched by construction.)
    if let Err(error) = gn_search_indexer::db::probe(&config.sql_uri) {
        eprintln!(
            "gn-indexer: database unreachable (SQL_URI={}); refusing to build an index: {error}",
              config.sql_uri
          );
        return ExitCode::from(1);
    }

    let result = match mode.as_str() {
        "full" => run_full(&run_config),
        "incremental" => run_incremental(&run_config),
        "delta" => run_delta(&run_config),
        _ => return usage(),
    };

    match result {
        Ok(stats) => {
            println!("mode={}", stats.mode);
            println!("built={}", stats.built);
            println!("replaced={}", stats.replaced);
            println!("deleted={}", stats.deleted);
            println!("unchanged={}", stats.unchanged);
            println!("index_doc_count={}", stats.index_doc_count);
            println!("max_live_documents={}", stats.max_live_documents);
            println!("shards_written={}", stats.shards_written);
            println!("shards_deleted={}", stats.shards_deleted);
            println!(
                "max_resident_sidecar_entries={}",
                stats.max_resident_sidecar_entries
            );
            println!(
                "max_resident_dataset_terms={}",
                stats.max_resident_dataset_terms
            );
            println!("peak_open_shards={}", stats.peak_open_shards);
            println!("shard_bytes_read={}", stats.shard_bytes_read);
            println!("commit_chunks={}", stats.commit_chunks);
            println!("chunk_rss_peak_kib={}", stats.chunk_rss_peak_kib);
            println!("startup={}", stats.startup);
            for (table, count) in &stats.candidates {
                println!("candidates_{table}={count}");
            }
            println!("affected_datasets={}", stats.affected_datasets);
            println!(
                "marker_from={}",
                stats.marker_from.as_deref().unwrap_or("-")
            );
            println!("marker_to={}", stats.marker_to.as_deref().unwrap_or("-"));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("gn-indexer: {error}");
            ExitCode::from(1)
        }
    }
}

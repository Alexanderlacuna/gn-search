//! A development HTTP server over the library.
//!
//! SCOUT sections implemented: SCOUT-NATIVE §1 (the request's parameters),
//! §5 (limits and paging), §6 (the status each error means) and
//! SCOUT-PROXY §7.2 (a bare JSON array of hits — gn3's shape, not GN4's
//! envelope), plus SCOUT-INDEXER §8 for the `/index` freshness read.
//!
//! This is NOT the production surface. GN4 owns the `/search` route, the
//! `Envelope`, `lod`, `locus`/`abbreviation` enrichment, `sort`/`order` and
//! the auth gate (SCOUT-NATIVE §8, SYNTHESIS §3); this binary exists so the
//! crate can be exercised end to end (`curl`) without GN4 and without a
//! web framework. It is plain `std::net`: no axum, no tokio, no
//! dependencies, and no hardening (one thread per connection, no request
//! size cap beyond the read loop, no TLS, no auth).
//!
//! Routes:
//!
//! * `GET /search?query=&type=&limit=&page=` — the hits, as a bare JSON
//!   array; `type` defaults to `phenotype`, `limit` to 20 and is capped at
//!   100, `page` to 1. A failure is never an empty array: an empty array
//!   means "no hits", and conflating the two hides a broken index
//!   (SCOUT-NATIVE §5, §6). Two classes differ, and the docs must not blur
//!   them: `Unreachable` — an index this build cannot open — is **503
//!   `index_unavailable`** (DECISIONS row 28), while `Internal` — no
//!   backend compiled for the configured path, the F1 refusal in
//!   `src/index/fixture.rs`, or `XAPIAN_DB_PATH` unset — is **500
//!   `internal_error`** carrying the message that names the fix (DECISIONS
//!   row 32: `Internal` is 500 on GN4's own table, and no status is
//!   invented here).
//! * `GET /index` — the sidecar manifest plus the manifest's age in
//!   seconds (SCOUT-INDEXER §8); 503 `manifest_unavailable` when there is
//!   no manifest this reader can use.
//! * `GET /healthz` — `ok`, plain text, no index access.

fn main() {
    #[cfg(feature = "serve")]
    {
        serve::run();
    }
    #[cfg(not(feature = "serve"))]
    {
        eprintln!(
            "gn-search: built without the `serve` feature; rebuild with `--features serve` \
             for the development HTTP server"
        );
        std::process::exit(2);
    }
}

#[cfg(feature = "serve")]
mod serve {
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    use gn_search_engine::manifest::{self, Manifest};
    use gn_search_engine::SearchError;
    use serde::Serialize;

    /// The index directory, under the key GN4 reads (SCOUT-NATIVE §2).
    const INDEX_ENV: &str = "XAPIAN_DB_PATH";
    /// The listen address.
    const ADDR_ENV: &str = "GN_SEARCH_ADDR";
    /// The default listen address: loopback only, since this server has no
    /// auth.
    const DEFAULT_ADDR: &str = "127.0.0.1:8080";
    const JSON: &str = "application/json";
    const TEXT: &str = "text/plain; charset=utf-8";

    pub fn run() {
        let addr = std::env::var(ADDR_ENV).unwrap_or_else(|_| DEFAULT_ADDR.to_string());
        let index_path = std::env::var(INDEX_ENV).unwrap_or_default();
        let listener = match TcpListener::bind(&addr) {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("gn-search: cannot listen on {addr}: {error}");
                std::process::exit(1);
            }
        };
        eprintln!("gn-search: listening on {addr}, index {index_path:?}");
        for incoming in listener.incoming() {
            match incoming {
                Ok(stream) => {
                    let index_path = index_path.clone();
                    thread::spawn(move || handle(stream, &index_path));
                }
                Err(error) => eprintln!("gn-search: accept failed: {error}"),
            }
        }
    }

    /// Answer one connection, then close it.
    fn handle(mut stream: TcpStream, index_path: &str) {
        let request_stream = match stream.try_clone() {
            Ok(clone) => clone,
            Err(error) => {
                eprintln!("gn-search: cannot clone the connection: {error}");
                return;
            }
        };
        let mut reader = BufReader::new(request_stream);
        let mut request_line = String::new();
        match reader.read_line(&mut request_line) {
            Ok(0) => return,
            Ok(_) => {}
            Err(error) => {
                eprintln!("gn-search: cannot read the request: {error}");
                return;
            }
        }
        // Drain the headers: none of them are read here.
        loop {
            let mut header = String::new();
            match reader.read_line(&mut header) {
                Ok(0) => break,
                Ok(_) => {
                    if header == "\r\n" || header == "\n" {
                        break;
                    }
                }
                Err(_) => return,
            }
        }

        let mut line = request_line.split_whitespace();
        let method = line.next().unwrap_or("");
        let target = line.next().unwrap_or("");
        if method != "GET" {
            let body = error_body("method_not_allowed", "this server answers GET only");
            let _ = respond(&mut stream, 405, JSON, &body);
            return;
        }
        let (path, params) = split_target(target);
        match path.as_str() {
            "/healthz" => {
                let _ = respond(&mut stream, 200, TEXT, b"ok\n");
            }
            "/search" => handle_search(&mut stream, &params, index_path),
            "/index" => handle_index(&mut stream, index_path),
            other => {
                let body = error_body("not_found", &format!("no route for {other}"));
                let _ = respond(&mut stream, 404, JSON, &body);
            }
        }
    }

    /// `GET /search`: the hits, as the bare JSON array gn3 serves
    /// (SCOUT-PROXY §7.2). The bounds GN4's route applies before it calls the
    /// engine are mirrored for `query` (non-empty), `limit` (numeric, capped
    /// at 100) and `page` (numeric, at least 1) — SCOUT-NATIVE §1, §5.
    fn handle_search(stream: &mut TcpStream, params: &[(String, String)], index_path: &str) {
        let query = match param(params, "query") {
            Some(query) if !query.trim().is_empty() => query,
            Some(_) => {
                let body = error_body("invalid_parameter", "query must not be empty");
                let _ = respond(stream, 400, JSON, &body);
                return;
            }
            None => {
                let body = error_body("invalid_parameter", "query is required");
                let _ = respond(stream, 400, JSON, &body);
                return;
            }
        };
        // NOT validated: `type` defaults to `phenotype` and any other value
        // goes through to the `XT{type}` filter unchanged, so `?type=banana`
        // answers 200 with an empty array where GN4 answers 400
        // `invalid_parameter`. The value domain is the route's, not this
        // crate's (`src/lib.rs`, "what this crate deliberately is not"), so
        // the server does not reject it either.
        let type_ = param(params, "type").unwrap_or("phenotype");
        // `limit` defaults to 20 and is capped at 100, exactly as the route
        // caps it (SCOUT-NATIVE §5). `limit=0` is accepted and truncates to
        // an empty page, which is the route's current behaviour.
        let limit = match param(params, "limit") {
            Some(value) => match value.parse::<u64>() {
                Ok(limit) => limit.min(100),
                Err(_) => {
                    let body = error_body("invalid_parameter", "limit must be a number");
                    let _ = respond(stream, 400, JSON, &body);
                    return;
                }
            },
            None => 20,
        };
        let page = match param(params, "page") {
            Some(value) => match value.parse::<u64>() {
                Ok(page) if page >= 1 => page,
                _ => {
                    let body = error_body("invalid_parameter", "page must be at least 1");
                    let _ = respond(stream, 400, JSON, &body);
                    return;
                }
            },
            None => 1,
        };
        match gn_search_engine::search(index_path, query, type_, limit, page) {
            // The engine returns the whole match set from the page's offset
            // (`Index::search`); truncating to one page is the caller's job,
            // exactly as it is the route's on the production path.
            Ok(mut hits) => {
                hits.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
                match serde_json::to_vec(&hits) {
                    Ok(body) => {
                        let _ = respond(stream, 200, JSON, &body);
                    }
                    Err(error) => {
                        let body = error_body(
                            "internal_error",
                            &format!("cannot encode the hits as JSON: {error}"),
                        );
                        let _ = respond(stream, 500, JSON, &body);
                    }
                }
            }
            Err(error) => {
                let (status, code) = status_for(&error);
                let body = error_body(code, error.message());
                let _ = respond(stream, status, JSON, &body);
            }
        }
    }

    /// The `/index` body: the manifest and how old it is.
    #[derive(Serialize)]
    struct IndexBody {
        manifest: Manifest,
        /// Seconds since the sidecar was written; absent when the clock is
        /// behind its mtime (see [`manifest::age_seconds`]).
        age_seconds: Option<u64>,
    }

    /// `GET /index`: the manifest and its age (SCOUT-INDEXER §8).
    fn handle_index(stream: &mut TcpStream, index_path: &str) {
        if index_path.trim().is_empty() {
            let body = error_body(
                "index_unavailable",
                "XAPIAN_DB_PATH is not set, so there is no index (and no manifest) to report",
            );
            let _ = respond(stream, 503, JSON, &body);
            return;
        }
        match manifest::read(index_path) {
            Ok(Some(manifest)) => {
                let body = IndexBody {
                    manifest,
                    age_seconds: manifest::age_seconds(index_path),
                };
                match serde_json::to_vec(&body) {
                    Ok(json) => {
                        let _ = respond(stream, 200, JSON, &json);
                    }
                    Err(error) => {
                        let body = error_body(
                            "internal_error",
                            &format!("cannot encode the manifest as JSON: {error}"),
                        );
                        let _ = respond(stream, 500, JSON, &body);
                    }
                }
            }
            Ok(None) => {
                let detail = format!(
                    "no usable sidecar manifest at {} (missing, or a format version this \
                     reader does not understand)",
                    manifest::sidecar_path(index_path)
                );
                let body = error_body("manifest_unavailable", &detail);
                let _ = respond(stream, 503, JSON, &body);
            }
            Err(error) => {
                let (status, code) = status_for(&error);
                let body = error_body(code, error.message());
                let _ = respond(stream, status, JSON, &body);
            }
        }
    }

    /// The status and machine-readable code this server gives each library
    /// error.
    ///
    /// GN4's own mapping is 422/502/502/502/500 (SCOUT-NATIVE §6) and stays
    /// in GN4's route. This server differs in exactly one place: an index it
    /// cannot open is this process's own local state, not an upstream, so it
    /// answers 503 with a named reason — and never an empty array, which
    /// would be indistinguishable from "no hits".
    fn status_for(error: &SearchError) -> (u16, &'static str) {
        match error {
            SearchError::InvalidQuery { .. } => (422, "invalid_query"),
            SearchError::Unreachable(_) => (503, "index_unavailable"),
            SearchError::Misbehaving(_) => (502, "backend_misbehaving"),
            SearchError::Backend(_) => (502, "backend_error"),
            SearchError::Internal(_) => (500, "internal_error"),
        }
    }

    /// Split `target` into its path and its decoded query parameters.
    fn split_target(target: &str) -> (String, Vec<(String, String)>) {
        let (path, query) = match target.split_once('?') {
            Some((path, query)) => (path, query),
            None => (target, ""),
        };
        let params = query
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| match pair.split_once('=') {
                Some((name, value)) => (percent_decode(name), percent_decode(value)),
                None => (percent_decode(pair), String::new()),
            })
            .collect();
        (percent_decode(path), params)
    }

    /// The first value for `name`.
    fn param<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
        params
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Decode `%XX` escapes and `+` as a space; malformed escapes are kept
    /// verbatim.
    fn percent_decode(value: &str) -> String {
        let bytes = value.as_bytes();
        let mut decoded = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'+' => {
                    decoded.push(b' ');
                    index += 1;
                }
                b'%' if index + 2 < bytes.len() => {
                    let high = (bytes[index + 1] as char).to_digit(16);
                    let low = (bytes[index + 2] as char).to_digit(16);
                    match (high, low) {
                        (Some(high), Some(low)) => {
                            decoded.push((high * 16 + low) as u8);
                            index += 3;
                        }
                        _ => {
                            decoded.push(bytes[index]);
                            index += 1;
                        }
                    }
                }
                byte => {
                    decoded.push(byte);
                    index += 1;
                }
            }
        }
        String::from_utf8_lossy(&decoded).into_owned()
    }

    /// A `{"error": code, "detail": detail}` body, escaped by serde rather
    /// than by hand.
    fn error_body(code: &str, detail: &str) -> Vec<u8> {
        let mut map = serde_json::Map::new();
        map.insert(
            "error".to_string(),
            serde_json::Value::String(code.to_string()),
        );
        map.insert(
            "detail".to_string(),
            serde_json::Value::String(detail.to_string()),
        );
        serde_json::to_vec(&serde_json::Value::Object(map))
            .unwrap_or_else(|_| b"{\"error\":\"encoding_failed\"}".to_vec())
    }

    /// Write one response and close the connection.
    fn respond(
        stream: &mut TcpStream,
        status: u16,
        content_type: &str,
        body: &[u8],
    ) -> std::io::Result<()> {
        let mut response = Vec::new();
        response.extend_from_slice(format!("HTTP/1.1 {status} {}\r\n", reason(status)).as_bytes());
        response.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
        response.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
        response.extend_from_slice(b"Connection: close\r\n\r\n");
        response.extend_from_slice(body);
        stream.write_all(&response)?;
        stream.flush()
    }

    fn reason(status: u16) -> &'static str {
        match status {
            200 => "OK",
            400 => "Bad Request",
            404 => "Not Found",
            405 => "Method Not Allowed",
            422 => "Unprocessable Entity",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            _ => "Unknown",
        }
    }
}

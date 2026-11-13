//! Record network traffic to disk so a page load can be replayed later.
//!
//! Set `AURORA_RECORD=<dir>` and every HTTP(S) exchange that reaches the
//! network is written to `<dir>`:
//!
//! - `index.jsonl`: one JSON object per exchange, in completion order. Each
//!   line has `v` (format version), `seq`, `method`, `url`, `request_body`
//!   (a body path or null), `request_headers`, and either `status`,
//!   `status_text`, `headers`, `body` and `body_len`, or `error` for a
//!   transport failure.
//! - `bodies/<hash>`: request and response bodies, stored once per distinct
//!   content. The hash is FNV-1a 64, which is stable across builds, so a
//!   replayer can key requests by `method`, `url` and `request_body`.
//!
//! `data:` and `file:` URLs never reach the network layer and are not
//! recorded. Recordings can contain whatever the server sent, including
//! `set-cookie` headers, so keep them out of the repository.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, PoisonError};

use serde_json::{Value, json};

use super::FetchError;
use super::http::HttpResponse;

const FORMAT_VERSION: u32 = 1;

static RECORDER: LazyLock<Option<Recorder>> = LazyLock::new(|| {
    let dir = std::env::var_os("AURORA_RECORD")?;
    match Recorder::open(Path::new(&dir)) {
        Ok(recorder) => {
            log::info!("recording network traffic to {}", dir.to_string_lossy());
            Some(recorder)
        }
        Err(error) => {
            eprintln!(
                "Aurora: AURORA_RECORD={} could not be opened, not recording: {error}",
                dir.to_string_lossy()
            );
            None
        }
    }
});

/// Record one exchange if `AURORA_RECORD` is set. Never fails the fetch: a
/// recording error is reported and the exchange is skipped.
pub(super) fn record(
    method: &str,
    url: &str,
    request_body: Option<&str>,
    request_headers: &[(String, String)],
    result: &Result<HttpResponse, FetchError>,
) {
    if let Some(recorder) = RECORDER.as_ref()
        && let Err(error) = recorder.record(method, url, request_body, request_headers, result)
    {
        eprintln!("Aurora: failed to record {method} {url}: {error}");
    }
}

pub(super) struct Recorder {
    dir: PathBuf,
    state: Mutex<RecorderState>,
}

struct RecorderState {
    index: File,
    next_seq: u64,
}

impl Recorder {
    pub(super) fn open(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir.join("bodies"))?;
        let index_path = dir.join("index.jsonl");
        // Appending to an existing recording continues its sequence numbers.
        let next_seq = match fs::read_to_string(&index_path) {
            Ok(existing) => existing.lines().filter(|l| !l.trim().is_empty()).count() as u64,
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error),
        };
        let index = OpenOptions::new()
            .create(true)
            .append(true)
            .open(index_path)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            state: Mutex::new(RecorderState { index, next_seq }),
        })
    }

    pub(super) fn record(
        &self,
        method: &str,
        url: &str,
        request_body: Option<&str>,
        request_headers: &[(String, String)],
        result: &Result<HttpResponse, FetchError>,
    ) -> io::Result<()> {
        let request_body = request_body
            .map(|body| self.store_body(body.as_bytes()))
            .transpose()?;
        let mut entry = json!({
            "v": FORMAT_VERSION,
            "method": method,
            "url": url,
            "request_body": request_body,
            "request_headers": pairs(request_headers),
        });
        match result {
            Ok(response) => {
                entry["status"] = json!(response.status);
                entry["status_text"] = json!(response.status_text);
                entry["headers"] = pairs(&response.headers);
                entry["body"] = json!(self.store_body(&response.body)?);
                entry["body_len"] = json!(response.body.len());
            }
            Err(error) => entry["error"] = json!(error.to_string()),
        }

        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        entry["seq"] = json!(state.next_seq);
        let mut line = serde_json::to_vec(&entry)?;
        line.push(b'\n');
        state.index.write_all(&line)?;
        state.next_seq += 1;
        Ok(())
    }

    /// Store a body under its content hash; returns its path relative to the
    /// recording directory. Identical bodies are written once.
    fn store_body(&self, body: &[u8]) -> io::Result<String> {
        let name = body_name(body);
        let path = self.dir.join(&name);
        if !path.exists() {
            // Several fetch threads can store the same body at once; write
            // to a unique temporary file and rename it into place.
            let tmp = self.dir.join(format!(
                "{name}.tmp-{:?}",
                std::thread::current().id()
            ));
            fs::write(&tmp, body)?;
            fs::rename(&tmp, &path)?;
        }
        Ok(name)
    }
}

/// The path, relative to a recording directory, a body is stored under.
pub(super) fn body_name(body: &[u8]) -> String {
    format!("bodies/{:016x}", fnv1a_64(body))
}

fn pairs(headers: &[(String, String)]) -> Value {
    Value::Array(
        headers
            .iter()
            .map(|(name, value)| json!([name, value]))
            .collect(),
    )
}

/// FNV-1a, 64-bit. Chosen over `std`'s hasher because recordings are keyed
/// by it, so its output must never change between builds or Rust versions.
pub(super) fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aurora-recording-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            status_text: "OK".to_string(),
            headers: vec![("content-type".to_string(), "text/html".to_string())],
            body: body.as_bytes().to_vec(),
        }
    }

    fn lines(dir: &Path) -> Vec<Value> {
        fs::read_to_string(dir.join("index.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn fnv1a_64_matches_reference_vectors() {
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn records_responses_errors_and_request_bodies_in_order() {
        let dir = temp_dir("order");
        let recorder = Recorder::open(&dir).unwrap();
        recorder
            .record("GET", "https://a.test/", None, &[], &Ok(response(200, "<html>")))
            .unwrap();
        let headers = vec![("x-client".to_string(), "1".to_string())];
        recorder
            .record(
                "POST",
                "https://a.test/api",
                Some("{\"q\":1}"),
                &headers,
                &Ok(response(404, "missing")),
            )
            .unwrap();
        recorder
            .record(
                "GET",
                "https://down.test/",
                None,
                &[],
                &Err(FetchError::Network("connection refused".to_string())),
            )
            .unwrap();

        let entries = lines(&dir);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0]["seq"], 0);
        assert_eq!(entries[0]["status"], 200);
        assert_eq!(entries[0]["request_body"], Value::Null);
        let body_path = dir.join(entries[0]["body"].as_str().unwrap());
        assert_eq!(fs::read(body_path).unwrap(), b"<html>");

        assert_eq!(entries[1]["seq"], 1);
        assert_eq!(entries[1]["status"], 404);
        assert_eq!(entries[1]["request_headers"], json!([["x-client", "1"]]));
        let request_path = dir.join(entries[1]["request_body"].as_str().unwrap());
        assert_eq!(fs::read(request_path).unwrap(), b"{\"q\":1}");

        assert_eq!(entries[2]["seq"], 2);
        assert!(entries[2]["error"].as_str().unwrap().contains("connection refused"));
        assert!(entries[2].get("status").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn identical_bodies_are_stored_once_and_reopening_continues_the_sequence() {
        let dir = temp_dir("dedup");
        {
            let recorder = Recorder::open(&dir).unwrap();
            for url in ["https://a.test/1", "https://a.test/2"] {
                recorder
                    .record("GET", url, None, &[], &Ok(response(200, "same")))
                    .unwrap();
            }
        }
        let recorder = Recorder::open(&dir).unwrap();
        recorder
            .record("GET", "https://a.test/3", None, &[], &Ok(response(200, "same")))
            .unwrap();

        let entries = lines(&dir);
        assert_eq!(entries[2]["seq"], 2);
        assert_eq!(entries[0]["body"], entries[2]["body"]);
        assert_eq!(fs::read_dir(dir.join("bodies")).unwrap().count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}

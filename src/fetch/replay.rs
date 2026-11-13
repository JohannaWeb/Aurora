//! Serve network traffic from a recording made with `AURORA_RECORD`.
//!
//! Set `AURORA_REPLAY=<dir>` and every HTTP(S) request is answered from
//! `<dir>/index.jsonl` instead of the network. A request matches an entry
//! with the same method, URL and request body. When several entries match
//! (a stylesheet fetched twice, a polled endpoint), they are served in
//! recorded order and the last one repeats once the rest are used up.
//!
//! A request with no match fails with a network error and is reported on
//! stderr; replay never falls back to the network. If the recording cannot
//! be loaded, every request fails the same way. Replayed requests are not
//! re-recorded when `AURORA_RECORD` is also set.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, PoisonError};

use serde_json::Value;

use super::FetchError;
use super::http::HttpResponse;
use super::recording::body_name;

static REPLAYER: LazyLock<Option<Result<Replayer, String>>> = LazyLock::new(|| {
    let dir = std::env::var_os("AURORA_REPLAY")?;
    let dir = PathBuf::from(dir);
    let replayer = Replayer::load(&dir).map_err(|error| {
        let message = format!("AURORA_REPLAY={} could not be loaded: {error}", dir.display());
        eprintln!("Aurora: {message}; every request will fail");
        message
    });
    if let Ok(replayer) = &replayer {
        log::info!(
            "replaying network traffic from {} ({} requests)",
            dir.display(),
            replayer.len()
        );
    }
    Some(replayer)
});

/// The replayed answer to a request, or `None` when replay is off.
pub(super) fn serve(
    method: &str,
    url: &str,
    request_body: Option<&str>,
) -> Option<Result<HttpResponse, FetchError>> {
    let replayer = REPLAYER.as_ref()?;
    Some(match replayer {
        Ok(replayer) => replayer.serve(method, url, request_body),
        Err(message) => Err(FetchError::Network(message.clone())),
    })
}

type Key = (String, String, Option<String>);

pub(super) struct Replayer {
    dir: PathBuf,
    entries: HashMap<Key, Vec<Value>>,
    served: Mutex<HashMap<Key, usize>>,
}

impl Replayer {
    pub(super) fn load(dir: &Path) -> Result<Self, String> {
        let index = fs::read_to_string(dir.join("index.jsonl")).map_err(|e| e.to_string())?;
        let mut entries: HashMap<Key, Vec<Value>> = HashMap::new();
        for (number, line) in index.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: Value = serde_json::from_str(line)
                .map_err(|error| format!("index.jsonl line {}: {error}", number + 1))?;
            let (Some(method), Some(url)) = (entry["method"].as_str(), entry["url"].as_str())
            else {
                return Err(format!("index.jsonl line {}: missing method or url", number + 1));
            };
            let key = key(method, url, entry["request_body"].as_str().map(str::to_string));
            entries.entry(key).or_default().push(entry);
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            entries,
            served: Mutex::new(HashMap::new()),
        })
    }

    fn len(&self) -> usize {
        self.entries.values().map(Vec::len).sum()
    }

    pub(super) fn serve(
        &self,
        method: &str,
        url: &str,
        request_body: Option<&str>,
    ) -> Result<HttpResponse, FetchError> {
        let key = key(method, url, request_body.map(|b| body_name(b.as_bytes())));
        let Some(candidates) = self.entries.get(&key) else {
            eprintln!("Aurora: replay miss: {method} {url}");
            return Err(FetchError::Network(format!(
                "replay miss: {method} {url} is not in {}",
                self.dir.display()
            )));
        };
        let index = {
            let mut served = self.served.lock().unwrap_or_else(PoisonError::into_inner);
            let next = served.entry(key).or_insert(0);
            let index = (*next).min(candidates.len() - 1);
            *next += 1;
            index
        };
        self.response(&candidates[index])
    }

    fn response(&self, entry: &Value) -> Result<HttpResponse, FetchError> {
        if let Some(error) = entry["error"].as_str() {
            return Err(FetchError::Network(error.to_string()));
        }
        let invalid = |what: &str| FetchError::InvalidResponse(format!("replay entry {what}"));
        let status = entry["status"]
            .as_u64()
            .and_then(|s| u16::try_from(s).ok())
            .ok_or_else(|| invalid("has no status"))?;
        let body_path = entry["body"].as_str().ok_or_else(|| invalid("has no body"))?;
        let body = fs::read(self.dir.join(body_path))?;
        let headers = entry["headers"]
            .as_array()
            .map(|pairs| {
                pairs
                    .iter()
                    .filter_map(|pair| {
                        Some((pair[0].as_str()?.to_string(), pair[1].as_str()?.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(HttpResponse {
            status,
            status_text: entry["status_text"].as_str().unwrap_or_default().to_string(),
            headers,
            body,
        })
    }
}

fn key(method: &str, url: &str, request_body: Option<String>) -> Key {
    (method.to_ascii_uppercase(), url.to_string(), request_body)
}

#[cfg(test)]
mod tests {
    use super::super::recording::Recorder;
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aurora-replay-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            status_text: "OK".to_string(),
            headers: vec![("content-type".to_string(), "text/css".to_string())],
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn serves_recorded_responses_by_method_url_and_body() {
        let dir = temp_dir("match");
        let recorder = Recorder::open(&dir).unwrap();
        let ok = |status, body| Ok(response(status, body));
        recorder.record("GET", "https://a.test/x", None, &[], &ok(200, "get")).unwrap();
        recorder.record("POST", "https://a.test/x", Some("q=1"), &[], &ok(201, "one")).unwrap();
        recorder.record("POST", "https://a.test/x", Some("q=2"), &[], &ok(202, "two")).unwrap();
        recorder
            .record(
                "GET",
                "https://down.test/",
                None,
                &[],
                &Err(FetchError::Network("refused".to_string())),
            )
            .unwrap();

        let replayer = Replayer::load(&dir).unwrap();
        let get = replayer.serve("get", "https://a.test/x", None).unwrap();
        assert_eq!((get.status, get.body.as_slice()), (200, b"get".as_slice()));
        assert_eq!(get.headers, vec![("content-type".to_string(), "text/css".to_string())]);
        assert_eq!(replayer.serve("POST", "https://a.test/x", Some("q=2")).unwrap().status, 202);
        assert_eq!(replayer.serve("POST", "https://a.test/x", Some("q=1")).unwrap().status, 201);
        let refused = replayer.serve("GET", "https://down.test/", None).err().unwrap();
        assert!(refused.to_string().contains("refused"));
        let miss = replayer.serve("POST", "https://a.test/x", Some("q=3")).err().unwrap();
        assert!(miss.to_string().contains("replay miss"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn repeated_requests_are_served_in_order_then_the_last_repeats() {
        let dir = temp_dir("order");
        let recorder = Recorder::open(&dir).unwrap();
        for body in ["first", "second"] {
            recorder
                .record("GET", "https://a.test/poll", None, &[], &Ok(response(200, body)))
                .unwrap();
        }

        let replayer = Replayer::load(&dir).unwrap();
        let bodies: Vec<Vec<u8>> = (0..3)
            .map(|_| replayer.serve("GET", "https://a.test/poll", None).unwrap().body)
            .collect();
        assert_eq!(bodies, [b"first".to_vec(), b"second".to_vec(), b"second".to_vec()]);
        let _ = fs::remove_dir_all(&dir);
    }
}

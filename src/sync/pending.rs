//! The operations not yet saved to GitHub, kept in `pending.json` next
//! to the library so that edits made offline survive a restart, as on
//! the phone (`pending_queue.dart`). An operation is added before the
//! local file applies it and removed only after the save that carries it
//! lands. Each operation keeps the summary taken when it was applied,
//! so the commit message names what the user saw.

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::clock::Timestamp;
use crate::repo::{Operation, json as operation_json};
use crate::storage;

#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub id: u64,
    pub operation: Operation,
    pub summary: String,
}

/// A save that was started: which operations it carries, the blob sha of
/// the content sent and the commit message, so that after a crash or a
/// lost answer the app can tell whether it landed instead of saving the
/// same change twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InFlight {
    pub ids: Vec<u64>,
    pub blob_sha: String,
    pub message: String,
    pub started_at: Timestamp,
}

#[derive(Debug)]
pub struct PendingQueue {
    /// `None` keeps the queue in memory only (the tests, the fixture).
    path: Option<PathBuf>,
    operations: Vec<Pending>,
    in_flight: Option<InFlight>,
    next: u64,
}

impl PendingQueue {
    /// The queue in memory only.
    pub fn in_memory() -> PendingQueue {
        PendingQueue {
            path: None,
            operations: Vec::new(),
            in_flight: None,
            next: 1,
        }
    }

    /// Reads the file. A missing file is an empty queue; a file that
    /// cannot be read is renamed aside with an `.unreadable` suffix,
    /// never overwritten, and the queue starts empty.
    pub fn open(path: PathBuf) -> PendingQueue {
        let mut queue = PendingQueue {
            path: Some(path.clone()),
            ..PendingQueue::in_memory()
        };
        let text = match storage::read_if_exists(&path) {
            Ok(Some(text)) => text,
            Ok(None) => return queue,
            Err(error) => {
                crate::trace::line(format_args!("could not read {}: {error}", path.display()));
                return queue;
            }
        };
        match parse(&text) {
            Ok((operations, in_flight, next)) => {
                queue.operations = operations;
                queue.in_flight = in_flight;
                queue.next = next;
            }
            Err(error) => {
                let aside = storage::aside_path(&path);
                let message = format!(
                    "{} could not be read ({error}); moved to {}",
                    path.display(),
                    aside.display()
                );
                eprintln!("libri: {message}");
                crate::trace::line(message);
                let _ = fs::rename(&path, &aside);
            }
        }
        queue
    }

    pub fn operations(&self) -> &[Pending] {
        &self.operations
    }

    pub fn in_flight(&self) -> Option<&InFlight> {
        self.in_flight.as_ref()
    }

    pub fn len(&self) -> usize {
        self.operations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub fn ids(&self) -> Vec<u64> {
        self.operations.iter().map(|o| o.id).collect()
    }

    pub fn add(&mut self, operation: Operation, summary: String) -> &Pending {
        let pending = Pending {
            id: self.next,
            operation,
            summary,
        };
        self.next += 1;
        self.operations.push(pending);
        self.save();
        self.operations.last().expect("just pushed")
    }

    pub fn mark_in_flight(&mut self, in_flight: InFlight) {
        self.in_flight = Some(in_flight);
        self.save();
    }

    pub fn clear_in_flight(&mut self) {
        if self.in_flight.take().is_some() {
            self.save();
        }
    }

    /// Drops the operations with these ids. The in-flight record goes
    /// once none of the operations it carried are queued any more.
    pub fn remove(&mut self, ids: &[u64]) {
        self.operations.retain(|o| !ids.contains(&o.id));
        if let Some(in_flight) = &self.in_flight
            && !in_flight
                .ids
                .iter()
                .any(|id| self.operations.iter().any(|o| o.id == *id))
        {
            self.in_flight = None;
        }
        self.save();
    }

    /// Forgets everything and removes the file (Disconnect).
    pub fn delete(&mut self) {
        self.operations.clear();
        self.in_flight = None;
        self.next = 1;
        if let Some(path) = &self.path
            && let Err(error) = storage::remove_if_exists(path)
        {
            eprintln!("libri: could not remove {}: {error}", path.display());
        }
    }

    pub fn to_json(&self) -> Value {
        let operations: Vec<Value> = self
            .operations
            .iter()
            .map(|o| {
                let mut object = serde_json::Map::new();
                object.insert("id".into(), json!(o.id));
                object.insert("summary".into(), json!(o.summary));
                if let Value::Object(fields) = operation_json::encode(&o.operation) {
                    object.extend(fields);
                }
                Value::Object(object)
            })
            .collect();
        json!({
            "version": 1,
            "next": self.next,
            "inFlight": self.in_flight.as_ref().map(|f| json!({
                "ids": f.ids,
                "blobSha": f.blob_sha,
                "message": f.message,
                "startedAt": f.started_at.to_rfc3339(),
            })),
            "operations": operations,
        })
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let text = format!(
            "{}\n",
            serde_json::to_string_pretty(&self.to_json()).expect("json serializes")
        );
        if let Err(error) = storage::write_atomically(path, &text) {
            let message = format!("could not write {}: {error}", path.display());
            eprintln!("libri: {message}");
            crate::trace::line(message);
        }
    }
}

type Parsed = (Vec<Pending>, Option<InFlight>, u64);

fn parse(text: &str) -> Result<Parsed, String> {
    let json: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let items = json["operations"]
        .as_array()
        .ok_or("no \"operations\" list")?;
    let mut operations = Vec::with_capacity(items.len());
    for item in items {
        let id = item["id"].as_u64().ok_or("an operation without an id")?;
        operations.push(Pending {
            id,
            operation: operation_json::decode(item)?,
            summary: item["summary"].as_str().unwrap_or_default().to_owned(),
        });
    }
    let in_flight = match &json["inFlight"] {
        Value::Object(f) => Some(InFlight {
            ids: f["ids"]
                .as_array()
                .map(|ids| ids.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default(),
            blob_sha: f["blobSha"].as_str().unwrap_or_default().to_owned(),
            message: f["message"].as_str().unwrap_or_default().to_owned(),
            started_at: f["startedAt"]
                .as_str()
                .and_then(Timestamp::parse)
                .ok_or("an in-flight save without a start time")?,
        }),
        _ => None,
    };
    let next = json["next"]
        .as_u64()
        .unwrap_or_else(|| operations.iter().map(|o| o.id).max().unwrap_or(0) + 1);
    Ok((operations, in_flight, next))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EntryStatus;
    use crate::storage::TempDir;

    fn status(id: &str) -> Operation {
        Operation::SetStatus {
            id: id.into(),
            status: EntryStatus::Done,
        }
    }

    #[test]
    fn the_queue_survives_a_restart_with_its_ids_and_the_save_in_flight() {
        let dir = TempDir::new("pending");
        let mut queue = PendingQueue::open(dir.path("pending.json"));
        assert!(queue.is_empty());
        queue.add(status("movies/a"), "Mark A as done".into());
        queue.add(status("movies/b"), "Mark B as done".into());
        queue.mark_in_flight(InFlight {
            ids: vec![1],
            blob_sha: "abc".into(),
            message: "Mark A as done".into(),
            started_at: Timestamp(1_700_000_000),
        });
        let again = PendingQueue::open(dir.path("pending.json"));
        assert_eq!(again.operations(), queue.operations());
        assert_eq!(again.in_flight(), queue.in_flight());
        assert_eq!(again.ids(), vec![1, 2]);
        let text = fs::read_to_string(dir.path("pending.json")).unwrap();
        assert!(text.contains("\"op\": \"setStatus\""));
        assert!(text.contains("\"startedAt\": \"2023-11-14T22:13:20Z\""));
        assert!(text.ends_with("}\n"));
    }

    #[test]
    fn removing_the_carried_operations_drops_the_in_flight_record() {
        let mut queue = PendingQueue::in_memory();
        queue.add(status("movies/a"), "a".into());
        queue.add(status("movies/b"), "b".into());
        queue.mark_in_flight(InFlight {
            ids: vec![1, 2],
            blob_sha: "x".into(),
            message: "m".into(),
            started_at: Timestamp(0),
        });
        queue.remove(&[1]);
        assert!(queue.in_flight().is_some(), "2 is still queued");
        queue.remove(&[2]);
        assert!(queue.in_flight().is_none());
        assert!(queue.is_empty());
        queue.add(status("movies/c"), "c".into());
        assert_eq!(queue.ids(), vec![3], "ids are never reused");
    }

    #[test]
    fn an_unreadable_file_is_moved_aside_and_delete_removes_the_file() {
        let dir = TempDir::new("pending");
        fs::write(dir.path("pending.json"), "{ nope").unwrap();
        let mut queue = PendingQueue::open(dir.path("pending.json"));
        assert!(queue.is_empty());
        assert!(!dir.path("pending.json").exists());
        let names: Vec<String> = fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|n| n.ends_with(".unreadable")));
        queue.add(status("movies/a"), "a".into());
        assert!(dir.path("pending.json").exists());
        queue.delete();
        assert!(!dir.path("pending.json").exists());
        assert!(queue.is_empty());
    }
}

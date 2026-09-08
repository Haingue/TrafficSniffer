use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};

use crate::domain::traffic::TrafficRecord;

#[derive(Clone)]
pub struct TrafficLogger {
    file: Option<Arc<Mutex<File>>>,
    console: bool,
}

impl TrafficLogger {
    pub fn new(file: Option<File>, console: bool) -> Self {
        Self {
            file: file.map(|file| Arc::new(Mutex::new(file))),
            console,
        }
    }

    pub fn record(&self, record: &TrafficRecord) {
        let Ok(mut line) = serde_json::to_string(record) else {
            return;
        };
        line.push('\n');

        if let Some(file) = &self.file {
            if let Ok(mut file) = file.lock() {
                let _ = file.write_all(line.as_bytes());
            }
        }
        if self.console {
            print!("{}", line);
        }
    }
}

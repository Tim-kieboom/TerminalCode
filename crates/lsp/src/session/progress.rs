//! The progress a server reports for long jobs (`$/progress`), as the one line
//! the status bar shows: "Indexing: 12/340 (serde) 3%".

use serde_json::Value;

/// One job that has begun and not ended.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Job {
    token: String,
    title: String,
    message: Option<String>,
    percentage: Option<u64>,
}

/// What a server is busy with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    /// In the order they began; the newest is shown.
    jobs: Vec<Job>,
}

impl Progress {
    /// Takes the `params` of a `$/progress` notification: `{ token, value }`
    /// where `value.kind` is `begin`, `report` or `end`. Anything else is
    /// left alone: a server's reports are not worth failing over.
    pub fn update(&mut self, params: &Value) {
        let Some(token) = params.get("token").map(token_text) else {
            return;
        };
        let Some(value) = params.get("value") else {
            return;
        };
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let percentage = value.get("percentage").and_then(Value::as_u64);
        match value.get("kind").and_then(Value::as_str) {
            Some("begin") => {
                self.jobs.retain(|job| job.token != token);
                self.jobs.push(Job {
                    token,
                    title: value
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("working")
                        .to_owned(),
                    message,
                    percentage,
                });
            }
            Some("report") => {
                if let Some(job) = self.jobs.iter_mut().find(|job| job.token == token) {
                    // A report that leaves a part out keeps what it had.
                    if message.is_some() {
                        job.message = message;
                    }
                    if percentage.is_some() {
                        job.percentage = percentage;
                    }
                }
            }
            Some("end") => self.jobs.retain(|job| job.token != token),
            _ => {}
        }
    }

    /// The newest job that is still going, as a line; `None` when the server is idle.
    pub fn text(&self) -> Option<String> {
        let job = self.jobs.last()?;
        let mut text = job.title.clone();
        if let Some(message) = job.message.as_deref().filter(|message| !message.is_empty()) {
            text.push_str(": ");
            text.push_str(message);
        }
        if let Some(percentage) = job.percentage {
            text.push_str(&format!(" {percentage}%"));
        }
        Some(text)
    }

    #[cfg(test)]
    pub fn is_idle(&self) -> bool {
        self.jobs.is_empty()
    }
}

/// A token is a number or a string; either names the job, and the number 1 is
/// not the text "1".
fn token_text(token: &Value) -> String {
    match token {
        Value::String(text) => format!("s:{text}"),
        other => format!("n:{other}"),
    }
}

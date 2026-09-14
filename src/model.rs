use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Metric {
    pub label: String,
    pub value: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Report {
    pub title: String,
    pub metrics: Vec<Metric>,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub debug: String,
}
impl Report {
    pub fn new(title: impl Into<String>, columns: &[&str]) -> Self {
        Self {
            title: title.into(),
            columns: columns.iter().map(|s| s.to_string()).collect(),
            ..Self::default()
        }
    }
    pub fn metric(&mut self, label: &str, value: impl ToString) {
        self.metrics.push(Metric {
            label: label.into(),
            value: value.to_string(),
        });
    }
    pub fn row(&mut self, values: &[&str]) {
        self.rows
            .push(values.iter().map(|v| v.to_string()).collect());
    }
    pub fn outcome(title: &str, value: String) -> Self {
        let mut report = Self::new(title, &["Status", "Operation", "Result"]);
        for line in value.lines().filter(|s| !s.trim().is_empty()) {
            let lower = line.to_lowercase();
            let status = if lower.contains("failed")
                || lower.contains("error")
                || lower.contains("denied")
                || lower.contains("refused")
            {
                "Failed"
            } else if lower.contains("restart") || lower.contains("require") {
                "Review"
            } else {
                "Complete"
            };
            report.row(&[status, title, line]);
        }
        report.debug = value;
        report
    }
    pub fn error(error: String) -> Self {
        Self::outcome("Request failed", format!("Failed: {error}"))
    }
}

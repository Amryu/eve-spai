//! Wire shapes for the JSON API responses.

use serde::Serialize;

/// Returned by `POST /api/br`.
#[derive(Debug, Serialize)]
pub struct CreateResponse {
    pub id: String,
    pub url: String,
}

/// One row in a report listing — enough for a future card without the full doc.
#[derive(Debug, Serialize)]
pub struct ReportRow {
    pub id: String,
    pub title: Option<String>,
    pub systems: Vec<String>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub kills: i32,
    pub total_isk: f64,
    pub side_names: Vec<String>,
    pub uploader_name: String,
    pub views: i64,
    /// Present in `/mine` (owner view); omitted from the public list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlisted: Option<bool>,
}

/// A page of report rows.
#[derive(Debug, Serialize)]
pub struct ReportPage {
    pub page: i64,
    pub per_page: i64,
    pub reports: Vec<ReportRow>,
}

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CreateResponse {
    pub id: String,
    pub url: String,
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlisted: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ReportPage {
    pub page: i64,
    pub per_page: i64,
    pub reports: Vec<ReportRow>,
}

use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ProjectRecord {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub goal: String,
    pub task_counter: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct TaskRecord {
    pub id: String,
    pub project_id: String,
    pub task_number: i64,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub review_state: String,
    pub sort_order: f64,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SubtaskRecord {
    pub id: String,
    pub task_id: String,
    pub title: String,
    pub done: i64,
    pub sort_order: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct OpenQuestionRecord {
    pub id: String,
    pub task_id: String,
    pub question: String,
    pub context: String,
    pub answer: Option<String>,
    pub status: String,
    pub asked_by: String,
    pub resolved_by: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ProjectQuestionRecord {
    pub id: String,
    pub task_id: String,
    pub task_number: i64,
    pub question: String,
    pub context: String,
    pub answer: Option<String>,
    pub status: String,
    pub asked_by: String,
    pub resolved_by: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SpecSectionRecord {
    pub id: String,
    pub project_id: String,
    pub section: String,
    pub content: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SpecRevisionRecord {
    pub id: String,
    pub project_id: String,
    pub section: String,
    pub content: String,
    pub edited_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AttachmentRecord {
    pub id: String,
    pub task_id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub storage_path: String,
    pub uploaded_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct TaskHistoryRecord {
    pub id: String,
    pub task_id: String,
    pub actor: String,
    pub action: String,
    pub detail: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ProjectActivityRecord {
    pub id: String,
    pub task_id: String,
    pub task_number: i64,
    pub actor: String,
    pub action: String,
    pub detail: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SystemEventRecord {
    pub id: String,
    pub project_slug: String,
    pub task_id: Option<String>,
    pub task_number: Option<i64>,
    pub actor: String,
    pub action: String,
    pub detail: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct WebhookRecord {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub url: String,
    pub platform: String,
    pub events: String,
    pub secret: Option<String>,
    pub active: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectSummary {
    pub project: ProjectRecord,
    pub backlog_count: i64,
    pub ready_count: i64,
    pub in_progress_count: i64,
    pub review_count: i64,
    pub done_count: i64,
    pub open_question_count: i64,
    pub not_ready_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskDetails {
    pub task: TaskRecord,
    pub labels: Vec<String>,
    pub subtasks: Vec<SubtaskRecord>,
    pub open_questions: Vec<OpenQuestionRecord>,
    pub attachments: Vec<AttachmentRecord>,
    pub history: Vec<TaskHistoryRecord>,
}

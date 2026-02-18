use chrono::{SecondsFormat, Utc};
use serde_json::Value;
use sqlx::query_builder::QueryBuilder;
use sqlx::{Any, AnyPool};
use uuid::Uuid;

use crate::db::models::{
    AttachmentRecord, OpenQuestionRecord, ProjectActivityRecord, ProjectQuestionRecord,
    ProjectRecord, ProjectSummary, SpecRevisionRecord, SpecSectionRecord, SubtaskRecord,
    SystemEventRecord, TaskDetails, TaskHistoryRecord, TaskRecord, WebhookRecord,
};
use crate::error::{AppError, AppResult};

const SPEC_SECTIONS: [&str; 6] = [
    "overview",
    "requirements",
    "architecture",
    "technical_design",
    "open_decisions",
    "references",
];

const WEBHOOK_EVENTS: [&str; 9] = [
    "task.created",
    "task.updated",
    "task.moved",
    "task.deleted",
    "task.review_state_changed",
    "question.created",
    "question.resolved",
    "spec.updated",
    "goal.updated",
];

#[derive(Debug, Clone)]
pub struct TaskFilters {
    pub status: Option<String>,
    pub label: Option<String>,
    pub review_state: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewTaskInput {
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub review_state: String,
    pub labels: Vec<String>,
    pub created_by: String,
}

#[derive(Debug, Clone)]
pub struct UpdateTaskInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub review_state: Option<String>,
    pub labels: Option<Vec<String>>,
    pub actor: String,
}

#[derive(Debug, Clone)]
pub struct MoveTaskInput {
    pub status: String,
    pub sort_order: Option<f64>,
    pub actor: String,
    pub mcp_origin: bool,
}

#[derive(Debug, Clone)]
pub struct UpdateSubtaskInput {
    pub title: Option<String>,
    pub done: Option<bool>,
    pub sort_order: Option<f64>,
    pub actor: String,
}

#[derive(Debug, Clone)]
pub struct NewAttachmentInput {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub storage_path: String,
    pub uploaded_by: String,
}

#[derive(Debug, Clone)]
pub struct CreateWebhookInput {
    pub name: String,
    pub url: String,
    pub platform: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct UpdateWebhookInput {
    pub name: Option<String>,
    pub url: Option<String>,
    pub platform: Option<String>,
    pub events: Option<Vec<String>>,
    pub secret: Option<String>,
    pub active: Option<bool>,
}

#[derive(Debug, Clone)]
pub enum TaskRef {
    Uuid(String),
    DisplayKey { slug: String, task_number: i64 },
}

pub fn parse_task_ref(value: &str) -> AppResult<TaskRef> {
    if is_canonical_uuid(value) {
        return Ok(TaskRef::Uuid(value.to_string()));
    }

    if let Some((slug, task_number)) = parse_display_key(value) {
        return Ok(TaskRef::DisplayKey { slug, task_number });
    }

    Err(AppError::BadRequest(format!(
        "invalid task reference '{value}', expected UUID or DISPLAY_KEY"
    )))
}

pub fn display_key(slug: &str, task_number: i64) -> String {
    format!("{slug}-{task_number}")
}

pub fn normalize_slug(slug: &str) -> AppResult<String> {
    let candidate = slug.trim().to_ascii_uppercase();
    if candidate.is_empty() {
        return Err(AppError::BadRequest(
            "project slug cannot be empty".to_string(),
        ));
    }

    if candidate.starts_with('-') || candidate.ends_with('-') || candidate.contains("--") {
        return Err(AppError::BadRequest(
            "project slug must not start/end with '-' or contain consecutive '-'".to_string(),
        ));
    }

    if !candidate.chars().all(|character| {
        character.is_ascii_uppercase() || character.is_ascii_digit() || character == '-'
    }) {
        return Err(AppError::BadRequest(
            "project slug may only include uppercase letters, digits, and '-'".to_string(),
        ));
    }

    Ok(candidate)
}

pub fn validate_spec_section(section: &str) -> AppResult<()> {
    if SPEC_SECTIONS.contains(&section) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "invalid spec section '{section}'"
        )))
    }
}

pub async fn list_projects(
    pool: &AnyPool,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<ProjectSummary>> {
    let projects = sqlx::query_as::<Any, ProjectRecord>(
        r#"
        SELECT id, slug, name, goal, task_counter, created_at, updated_at
        FROM projects
        ORDER BY created_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let mut results = Vec::with_capacity(projects.len());
    for project in projects {
        let project_id = project.id.clone();
        results.push(project_summary_by_id(pool, &project_id, project).await?);
    }

    Ok(results)
}

pub async fn create_project_with_slug(
    pool: &AnyPool,
    name: &str,
    goal: &str,
    slug: &str,
) -> AppResult<ProjectSummary> {
    let normalized_name = name.trim();
    if normalized_name.is_empty() {
        return Err(AppError::BadRequest(
            "project name cannot be empty".to_string(),
        ));
    }

    let normalized_slug = normalize_slug(slug)?;
    create_project_record(pool, normalized_name, goal, &normalized_slug).await
}

async fn create_project_record(
    pool: &AnyPool,
    name: &str,
    goal: &str,
    slug: &str,
) -> AppResult<ProjectSummary> {
    let now = now_timestamp();
    let project_id = Uuid::new_v4().to_string();

    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO projects (id, slug, name, goal, task_counter, created_at, updated_at)
        VALUES (?, ?, ?, ?, 0, ?, ?)
        "#,
    )
    .bind(&project_id)
    .bind(slug)
    .bind(name)
    .bind(goal)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    for section in SPEC_SECTIONS {
        sqlx::query(
            r#"
            INSERT INTO spec_sections (id, project_id, section, content, updated_at)
            VALUES (?, ?, ?, '', ?)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&project_id)
        .bind(section)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let project = sqlx::query_as::<Any, ProjectRecord>(
        r#"
        SELECT id, slug, name, goal, task_counter, created_at, updated_at
        FROM projects
        WHERE id = ?
        "#,
    )
    .bind(&project_id)
    .fetch_one(pool)
    .await?;

    project_summary_by_id(pool, &project_id, project).await
}

pub async fn get_project(pool: &AnyPool, slug: &str) -> AppResult<ProjectSummary> {
    let project = sqlx::query_as::<Any, ProjectRecord>(
        r#"
        SELECT id, slug, name, goal, task_counter, created_at, updated_at
        FROM projects
        WHERE slug = ?
        "#,
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("project '{slug}' not found")))?;

    let project_id = project.id.clone();
    project_summary_by_id(pool, &project_id, project).await
}

pub async fn update_project(
    pool: &AnyPool,
    slug: &str,
    name: Option<String>,
    goal: Option<String>,
    actor: &str,
) -> AppResult<ProjectSummary> {
    let existing = sqlx::query_as::<Any, ProjectRecord>(
        r#"
        SELECT id, slug, name, goal, task_counter, created_at, updated_at
        FROM projects
        WHERE slug = ?
        "#,
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("project '{slug}' not found")))?;

    let updated_name = match name {
        Some(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                return Err(AppError::BadRequest(
                    "project name cannot be empty".to_string(),
                ));
            }
            trimmed
        }
        None => existing.name,
    };

    let previous_goal = existing.goal.clone();
    let updated_goal = goal.unwrap_or(existing.goal);
    let now = now_timestamp();
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        UPDATE projects
        SET name = ?, goal = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&updated_name)
    .bind(&updated_goal)
    .bind(&now)
    .bind(&existing.id)
    .execute(&mut *tx)
    .await?;

    if updated_goal != previous_goal {
        insert_project_event(
            &mut tx,
            &existing.id,
            actor,
            "goal.updated",
            serde_json::json!({
                "from_goal": previous_goal,
                "to_goal": updated_goal,
            }),
        )
        .await?;
    }

    tx.commit().await?;
    get_project(pool, slug).await
}

pub async fn delete_project(pool: &AnyPool, slug: &str) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM projects WHERE slug = ?")
        .bind(slug)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("project '{slug}' not found")));
    }

    Ok(())
}

pub async fn list_project_webhooks(
    pool: &AnyPool,
    project_slug: &str,
) -> AppResult<Vec<WebhookRecord>> {
    let project_id = project_id_by_slug(pool, project_slug).await?;

    let webhooks = sqlx::query_as::<Any, WebhookRecord>(
        r#"
        SELECT
            id,
            project_id,
            name,
            url,
            platform,
            events,
            secret,
            active,
            created_at,
            updated_at
        FROM webhooks
        WHERE project_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    Ok(webhooks)
}

pub async fn get_project_webhook(
    pool: &AnyPool,
    project_slug: &str,
    webhook_id: &str,
) -> AppResult<WebhookRecord> {
    let project_id = project_id_by_slug(pool, project_slug).await?;

    let webhook = sqlx::query_as::<Any, WebhookRecord>(
        r#"
        SELECT
            id,
            project_id,
            name,
            url,
            platform,
            events,
            secret,
            active,
            created_at,
            updated_at
        FROM webhooks
        WHERE project_id = ? AND id = ?
        "#,
    )
    .bind(project_id)
    .bind(webhook_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("webhook '{webhook_id}' not found")))?;

    Ok(webhook)
}

pub async fn create_webhook(
    pool: &AnyPool,
    project_slug: &str,
    input: CreateWebhookInput,
) -> AppResult<WebhookRecord> {
    let project_id = project_id_by_slug(pool, project_slug).await?;
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest(
            "webhook name cannot be empty".to_string(),
        ));
    }

    let url = normalize_webhook_url(&input.url)?;
    let platform = normalize_webhook_platform(&input.platform)?;
    let events = normalize_webhook_events(input.events)?;
    let events_json = serde_json::to_string(&events).map_err(|error| {
        tracing::error!(error = ?error, "failed to serialize webhook events");
        AppError::Internal
    })?;
    let secret = normalize_optional_secret(input.secret);

    let webhook_id = Uuid::new_v4().to_string();
    let now = now_timestamp();

    sqlx::query(
        r#"
        INSERT INTO webhooks (
            id,
            project_id,
            name,
            url,
            platform,
            events,
            secret,
            active,
            created_at,
            updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&webhook_id)
    .bind(&project_id)
    .bind(&name)
    .bind(&url)
    .bind(&platform)
    .bind(&events_json)
    .bind(secret)
    .bind(i64::from(input.active))
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    get_project_webhook(pool, project_slug, &webhook_id).await
}

pub async fn update_webhook(
    pool: &AnyPool,
    project_slug: &str,
    webhook_id: &str,
    input: UpdateWebhookInput,
) -> AppResult<WebhookRecord> {
    let existing = get_project_webhook(pool, project_slug, webhook_id).await?;

    let name = match input.name {
        Some(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                return Err(AppError::BadRequest(
                    "webhook name cannot be empty".to_string(),
                ));
            }
            trimmed
        }
        None => existing.name,
    };

    let url = match input.url {
        Some(value) => normalize_webhook_url(&value)?,
        None => existing.url,
    };

    let platform = match input.platform {
        Some(value) => normalize_webhook_platform(&value)?,
        None => existing.platform,
    };

    let events = match input.events {
        Some(value) => {
            let normalized = normalize_webhook_events(value)?;
            serde_json::to_string(&normalized).map_err(|error| {
                tracing::error!(error = ?error, "failed to serialize webhook events");
                AppError::Internal
            })?
        }
        None => existing.events,
    };

    let secret = match input.secret {
        Some(value) => normalize_optional_secret(Some(value)),
        None => existing.secret,
    };

    let active = input.active.unwrap_or(existing.active == 1);
    let now = now_timestamp();

    sqlx::query(
        r#"
        UPDATE webhooks
        SET name = ?, url = ?, platform = ?, events = ?, secret = ?, active = ?, updated_at = ?
        WHERE id = ? AND project_id = ?
        "#,
    )
    .bind(name)
    .bind(url)
    .bind(platform)
    .bind(events)
    .bind(secret)
    .bind(i64::from(active))
    .bind(now)
    .bind(webhook_id)
    .bind(existing.project_id)
    .execute(pool)
    .await?;

    get_project_webhook(pool, project_slug, webhook_id).await
}

pub async fn delete_webhook(pool: &AnyPool, project_slug: &str, webhook_id: &str) -> AppResult<()> {
    let project_id = project_id_by_slug(pool, project_slug).await?;

    let result = sqlx::query("DELETE FROM webhooks WHERE id = ? AND project_id = ?")
        .bind(webhook_id)
        .bind(project_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "webhook '{webhook_id}' not found"
        )));
    }

    Ok(())
}

pub async fn list_active_project_webhooks(
    pool: &AnyPool,
    project_slug: &str,
) -> AppResult<Vec<WebhookRecord>> {
    let project_id = project_id_by_slug(pool, project_slug).await?;

    let webhooks = sqlx::query_as::<Any, WebhookRecord>(
        r#"
        SELECT
            id,
            project_id,
            name,
            url,
            platform,
            events,
            secret,
            active,
            created_at,
            updated_at
        FROM webhooks
        WHERE project_id = ? AND active = 1
        ORDER BY created_at DESC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    Ok(webhooks)
}

pub async fn list_spec_sections(
    pool: &AnyPool,
    project_slug: &str,
) -> AppResult<Vec<SpecSectionRecord>> {
    let project_id = project_id_by_slug(pool, project_slug).await?;

    let sections = sqlx::query_as::<Any, SpecSectionRecord>(
        r#"
        SELECT id, project_id, section, content, updated_at
        FROM spec_sections
        WHERE project_id = ?
        ORDER BY
            CASE section
                WHEN 'overview' THEN 0
                WHEN 'requirements' THEN 1
                WHEN 'architecture' THEN 2
                WHEN 'technical_design' THEN 3
                WHEN 'open_decisions' THEN 4
                WHEN 'references' THEN 5
                ELSE 6
            END
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    Ok(sections)
}

pub async fn get_spec_section(
    pool: &AnyPool,
    project_slug: &str,
    section: &str,
) -> AppResult<SpecSectionRecord> {
    validate_spec_section(section)?;
    let project_id = project_id_by_slug(pool, project_slug).await?;

    let record = sqlx::query_as::<Any, SpecSectionRecord>(
        r#"
        SELECT id, project_id, section, content, updated_at
        FROM spec_sections
        WHERE project_id = ? AND section = ?
        "#,
    )
    .bind(project_id)
    .bind(section)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        AppError::NotFound(format!(
            "spec section '{section}' not found for project '{project_slug}'"
        ))
    })?;

    Ok(record)
}

pub async fn update_spec_section(
    pool: &AnyPool,
    project_slug: &str,
    section: &str,
    content: &str,
    edited_by: &str,
) -> AppResult<SpecSectionRecord> {
    validate_spec_section(section)?;
    let project_id = project_id_by_slug(pool, project_slug).await?;
    let now = now_timestamp();

    let mut tx = pool.begin().await?;
    let updated = sqlx::query(
        r#"
        UPDATE spec_sections
        SET content = ?, updated_at = ?
        WHERE project_id = ? AND section = ?
        "#,
    )
    .bind(content)
    .bind(&now)
    .bind(&project_id)
    .bind(section)
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "spec section '{section}' not found for project '{project_slug}'"
        )));
    }

    sqlx::query(
        r#"
        INSERT INTO spec_revisions (id, project_id, section, content, edited_by, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&project_id)
    .bind(section)
    .bind(content)
    .bind(edited_by)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    insert_project_event(
        &mut tx,
        &project_id,
        edited_by,
        "spec.updated",
        serde_json::json!({ "section": section }),
    )
    .await?;

    tx.commit().await?;

    get_spec_section(pool, project_slug, section).await
}

pub async fn list_spec_history(
    pool: &AnyPool,
    project_slug: &str,
    section: &str,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<SpecRevisionRecord>> {
    validate_spec_section(section)?;
    let project_id = project_id_by_slug(pool, project_slug).await?;

    let revisions = sqlx::query_as::<Any, SpecRevisionRecord>(
        r#"
        SELECT id, project_id, section, content, edited_by, created_at
        FROM spec_revisions
        WHERE project_id = ? AND section = ?
        ORDER BY created_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(project_id)
    .bind(section)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(revisions)
}

pub async fn list_project_open_questions(
    pool: &AnyPool,
    project_slug: &str,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<ProjectQuestionRecord>> {
    let project_id = project_id_by_slug(pool, project_slug).await?;

    let questions = sqlx::query_as::<Any, ProjectQuestionRecord>(
        r#"
        SELECT
            q.id,
            q.task_id,
            t.task_number,
            q.question,
            q.context,
            q.answer,
            q.status,
            q.asked_by,
            q.resolved_by,
            q.created_at,
            q.resolved_at
        FROM open_questions q
        INNER JOIN tasks t ON t.id = q.task_id
        WHERE t.project_id = ? AND q.status = 'open'
        ORDER BY q.created_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(project_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(questions)
}

pub async fn list_recent_project_activity(
    pool: &AnyPool,
    project_slug: &str,
    limit: i64,
) -> AppResult<Vec<ProjectActivityRecord>> {
    if limit <= 0 || limit > 100 {
        return Err(AppError::BadRequest(
            "limit must be between 1 and 100".to_string(),
        ));
    }

    let project_id = project_id_by_slug(pool, project_slug).await?;

    let activity = sqlx::query_as::<Any, ProjectActivityRecord>(
        r#"
        SELECT
            h.id,
            h.task_id,
            t.task_number,
            h.actor,
            h.action,
            h.detail,
            h.created_at
        FROM task_history h
        INNER JOIN tasks t ON t.id = h.task_id
        WHERE t.project_id = ?
        ORDER BY h.created_at DESC
        LIMIT ?
        "#,
    )
    .bind(project_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(activity)
}

pub async fn list_system_events(
    pool: &AnyPool,
    project_slugs: &[String],
    after_created_at: Option<&str>,
    after_id: Option<&str>,
    limit: i64,
) -> AppResult<Vec<SystemEventRecord>> {
    if limit <= 0 || limit > 200 {
        return Err(AppError::BadRequest(
            "limit must be between 1 and 200".to_string(),
        ));
    }

    if after_created_at.is_some() != after_id.is_some() {
        return Err(AppError::BadRequest(
            "after_created_at and after_id must be provided together".to_string(),
        ));
    }

    let mut query = QueryBuilder::<Any>::new(
        r#"
        SELECT
            e.id,
            p.slug AS project_slug,
            e.task_id,
            e.task_number,
            e.actor,
            e.action,
            e.detail,
            e.created_at
        FROM system_events e
        INNER JOIN projects p ON p.id = e.project_id
        WHERE 1 = 1
        "#,
    );

    if !project_slugs.is_empty() {
        query.push(" AND p.slug IN (");
        {
            let mut separated = query.separated(", ");
            for slug in project_slugs {
                separated.push_bind(slug);
            }
        }
        query.push(")");
    }

    if let (Some(created_at), Some(event_id)) = (after_created_at, after_id) {
        query.push(" AND (e.created_at > ");
        query.push_bind(created_at);
        query.push(" OR (e.created_at = ");
        query.push_bind(created_at);
        query.push(" AND e.id > ");
        query.push_bind(event_id);
        query.push("))");
    }

    query.push(" ORDER BY e.created_at ASC, e.id ASC LIMIT ");
    query.push_bind(limit);

    let events = query
        .build_query_as::<SystemEventRecord>()
        .fetch_all(pool)
        .await?;
    Ok(events)
}

pub async fn latest_system_event_cursor(
    pool: &AnyPool,
    project_slugs: &[String],
) -> AppResult<Option<(String, String)>> {
    #[derive(sqlx::FromRow)]
    struct CursorRow {
        created_at: String,
        id: String,
    }

    let mut query = QueryBuilder::<Any>::new(
        r#"
        SELECT e.created_at, e.id
        FROM system_events e
        INNER JOIN projects p ON p.id = e.project_id
        WHERE 1 = 1
        "#,
    );

    if !project_slugs.is_empty() {
        query.push(" AND p.slug IN (");
        {
            let mut separated = query.separated(", ");
            for slug in project_slugs {
                separated.push_bind(slug);
            }
        }
        query.push(")");
    }

    query.push(" ORDER BY e.created_at DESC, e.id DESC LIMIT 1");

    let row = query
        .build_query_as::<CursorRow>()
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|cursor| (cursor.created_at, cursor.id)))
}

pub async fn create_attachment(
    pool: &AnyPool,
    project_slug: &str,
    task_ref: &str,
    input: NewAttachmentInput,
) -> AppResult<AttachmentRecord> {
    let task_id = resolve_task_id(pool, project_slug, task_ref).await?;
    let filename = input.filename.trim().to_string();
    if filename.is_empty() {
        return Err(AppError::BadRequest(
            "attachment filename cannot be empty".to_string(),
        ));
    }

    if input.size_bytes < 0 {
        return Err(AppError::BadRequest(
            "attachment size cannot be negative".to_string(),
        ));
    }

    let content_type = if input.content_type.trim().is_empty() {
        "application/octet-stream".to_string()
    } else {
        input.content_type.trim().to_string()
    };

    let now = now_timestamp();
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO attachments (
            id,
            task_id,
            filename,
            content_type,
            size_bytes,
            storage_path,
            uploaded_by,
            created_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&input.id)
    .bind(&task_id)
    .bind(&filename)
    .bind(content_type)
    .bind(input.size_bytes)
    .bind(input.storage_path)
    .bind(&input.uploaded_by)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    insert_history(
        &mut tx,
        &task_id,
        &input.uploaded_by,
        "attachment.created",
        serde_json::json!({
            "attachment_id": input.id,
            "filename": filename,
            "size_bytes": input.size_bytes,
        }),
    )
    .await?;

    tx.commit().await?;
    get_attachment_for_task(pool, &task_id, &input.id).await
}

pub async fn get_attachment(pool: &AnyPool, attachment_id: &str) -> AppResult<AttachmentRecord> {
    let attachment = sqlx::query_as::<Any, AttachmentRecord>(
        r#"
        SELECT id, task_id, filename, content_type, size_bytes, storage_path, uploaded_by, created_at
        FROM attachments
        WHERE id = ?
        "#,
    )
    .bind(attachment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("attachment '{attachment_id}' not found")))?;

    Ok(attachment)
}

pub async fn delete_attachment(
    pool: &AnyPool,
    project_slug: &str,
    task_ref: &str,
    attachment_id: &str,
    actor: &str,
) -> AppResult<AttachmentRecord> {
    let task_id = resolve_task_id(pool, project_slug, task_ref).await?;
    let attachment = get_attachment_for_task(pool, &task_id, attachment_id).await?;

    let mut tx = pool.begin().await?;
    let result = sqlx::query("DELETE FROM attachments WHERE id = ? AND task_id = ?")
        .bind(attachment_id)
        .bind(&task_id)
        .execute(&mut *tx)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "attachment '{attachment_id}' not found for task '{task_ref}'"
        )));
    }

    insert_history(
        &mut tx,
        &task_id,
        actor,
        "attachment.deleted",
        serde_json::json!({
            "attachment_id": attachment_id,
            "filename": attachment.filename,
        }),
    )
    .await?;

    tx.commit().await?;
    Ok(attachment)
}

pub async fn create_open_question(
    pool: &AnyPool,
    project_slug: &str,
    task_ref: &str,
    question: &str,
    context: &str,
    asked_by: &str,
) -> AppResult<OpenQuestionRecord> {
    let task_id = resolve_task_id(pool, project_slug, task_ref).await?;
    let trimmed_question = question.trim().to_string();
    if trimmed_question.is_empty() {
        return Err(AppError::BadRequest("question cannot be empty".to_string()));
    }

    let now = now_timestamp();
    let question_id = Uuid::new_v4().to_string();
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO open_questions (
            id,
            task_id,
            question,
            context,
            answer,
            status,
            asked_by,
            resolved_by,
            created_at,
            resolved_at
        )
        VALUES (?, ?, ?, ?, NULL, 'open', ?, NULL, ?, NULL)
        "#,
    )
    .bind(&question_id)
    .bind(&task_id)
    .bind(&trimmed_question)
    .bind(context)
    .bind(asked_by)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    insert_history(
        &mut tx,
        &task_id,
        asked_by,
        "question.created",
        serde_json::json!({
            "question_id": question_id,
            "question": trimmed_question,
        }),
    )
    .await?;

    tx.commit().await?;

    get_open_question_by_id(pool, &task_id, &question_id).await
}

pub async fn answer_open_question(
    pool: &AnyPool,
    project_slug: &str,
    task_ref: &str,
    question_id: &str,
    answer: &str,
    resolved_by: &str,
) -> AppResult<OpenQuestionRecord> {
    let task_id = resolve_task_id(pool, project_slug, task_ref).await?;
    let trimmed_answer = answer.trim().to_string();
    if trimmed_answer.is_empty() {
        return Err(AppError::BadRequest("answer cannot be empty".to_string()));
    }

    let existing = get_open_question_by_id(pool, &task_id, question_id).await?;
    if existing.status != "open" {
        return Err(AppError::Conflict(format!(
            "question '{question_id}' is already resolved"
        )));
    }

    let now = now_timestamp();
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        UPDATE open_questions
        SET answer = ?, status = 'resolved', resolved_by = ?, resolved_at = ?
        WHERE id = ? AND task_id = ? AND status = 'open'
        "#,
    )
    .bind(&trimmed_answer)
    .bind(resolved_by)
    .bind(&now)
    .bind(question_id)
    .bind(&task_id)
    .execute(&mut *tx)
    .await?;

    insert_history(
        &mut tx,
        &task_id,
        resolved_by,
        "question.resolved",
        serde_json::json!({
            "question_id": question_id,
        }),
    )
    .await?;

    tx.commit().await?;

    get_open_question_by_id(pool, &task_id, question_id).await
}

pub async fn set_review_state(
    pool: &AnyPool,
    project_slug: &str,
    task_ref: &str,
    review_state: &str,
    actor: &str,
) -> AppResult<TaskRecord> {
    validate_review_state(review_state)?;
    let task_id = resolve_task_id(pool, project_slug, task_ref).await?;
    let current = get_task_record_by_id(pool, &task_id).await?;

    if current.review_state == review_state {
        return Ok(current);
    }

    let now = now_timestamp();
    let mut tx = pool.begin().await?;

    sqlx::query("UPDATE tasks SET review_state = ?, updated_at = ? WHERE id = ?")
        .bind(review_state)
        .bind(&now)
        .bind(&task_id)
        .execute(&mut *tx)
        .await?;

    insert_history(
        &mut tx,
        &task_id,
        actor,
        "task.review_state_changed",
        serde_json::json!({
            "from_review_state": current.review_state,
            "to_review_state": review_state,
        }),
    )
    .await?;

    tx.commit().await?;
    get_task_record_by_id(pool, &task_id).await
}

pub async fn list_tasks(
    pool: &AnyPool,
    project_slug: &str,
    filters: TaskFilters,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<TaskRecord>> {
    if let Some(status) = filters.status.as_deref() {
        validate_status(status)?;
    }

    if let Some(review_state) = filters.review_state.as_deref() {
        validate_review_state(review_state)?;
    }

    let mut query = QueryBuilder::<Any>::new(
        r#"
        SELECT
            t.id,
            t.project_id,
            t.task_number,
            t.title,
            t.description,
            t.status,
            t.priority,
            t.review_state,
            t.sort_order,
            t.created_by,
            t.created_at,
            t.updated_at
        FROM tasks t
        INNER JOIN projects p ON p.id = t.project_id
        WHERE p.slug =
        "#,
    );

    query.push_bind(project_slug);

    if let Some(status) = filters.status {
        query.push(" AND t.status = ");
        query.push_bind(status);
    }

    if let Some(review_state) = filters.review_state {
        query.push(" AND t.review_state = ");
        query.push_bind(review_state);
    }

    if let Some(label) = filters.label {
        query.push(
            r#"
            AND EXISTS (
                SELECT 1
                FROM task_labels l
                WHERE l.task_id = t.id AND l.label =
            "#,
        );
        query.push_bind(label);
        query.push(')');
    }

    query.push(
        r#"
        ORDER BY
            CASE t.status
                WHEN 'backlog' THEN 0
                WHEN 'ready' THEN 1
                WHEN 'in_progress' THEN 2
                WHEN 'review' THEN 3
                WHEN 'done' THEN 4
                ELSE 5
            END,
            t.sort_order ASC,
            t.created_at ASC
        LIMIT
        "#,
    );
    query.push_bind(limit);
    query.push(" OFFSET ");
    query.push_bind(offset);

    let tasks = query.build_query_as::<TaskRecord>().fetch_all(pool).await?;
    Ok(tasks)
}

pub async fn create_task(
    pool: &AnyPool,
    project_slug: &str,
    input: NewTaskInput,
) -> AppResult<TaskRecord> {
    validate_status(&input.status)?;
    validate_priority(&input.priority)?;
    validate_review_state(&input.review_state)?;

    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::BadRequest(
            "task title cannot be empty".to_string(),
        ));
    }

    let now = now_timestamp();
    let task_id = Uuid::new_v4().to_string();

    let mut tx = pool.begin().await?;

    let project_id: String = sqlx::query_scalar(
        r#"
        SELECT id
        FROM projects
        WHERE slug = ?
        "#,
    )
    .bind(project_slug)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("project '{project_slug}' not found")))?;

    sqlx::query("UPDATE projects SET task_counter = task_counter + 1, updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&project_id)
        .execute(&mut *tx)
        .await?;

    let task_number: i64 = sqlx::query_scalar("SELECT task_counter FROM projects WHERE id = ?")
        .bind(&project_id)
        .fetch_one(&mut *tx)
        .await?;

    let sort_order: f64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(MAX(sort_order), 0) AS REAL) + 1.0 FROM tasks WHERE project_id = ? AND status = ?",
    )
    .bind(&project_id)
    .bind(&input.status)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO tasks (
            id,
            project_id,
            task_number,
            title,
            description,
            status,
            priority,
            review_state,
            sort_order,
            created_by,
            created_at,
            updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&task_id)
    .bind(&project_id)
    .bind(task_number)
    .bind(&title)
    .bind(input.description)
    .bind(&input.status)
    .bind(&input.priority)
    .bind(&input.review_state)
    .bind(sort_order)
    .bind(&input.created_by)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    let labels = normalized_labels(input.labels);
    for label in labels {
        sqlx::query("INSERT INTO task_labels (task_id, label) VALUES (?, ?)")
            .bind(&task_id)
            .bind(label)
            .execute(&mut *tx)
            .await?;
    }

    insert_history(
        &mut tx,
        &task_id,
        &input.created_by,
        "task.created",
        serde_json::json!({ "status": input.status, "priority": input.priority }),
    )
    .await?;

    tx.commit().await?;

    get_task_record_by_id(pool, &task_id).await
}

pub async fn get_task_details(
    pool: &AnyPool,
    project_slug: &str,
    task_ref: &str,
) -> AppResult<TaskDetails> {
    let task_id = resolve_task_id(pool, project_slug, task_ref).await?;
    let task = get_task_record_by_id(pool, &task_id).await?;

    let labels: Vec<String> =
        sqlx::query_scalar("SELECT label FROM task_labels WHERE task_id = ? ORDER BY label ASC")
            .bind(&task.id)
            .fetch_all(pool)
            .await?;

    let subtasks = sqlx::query_as::<Any, SubtaskRecord>(
        r#"
        SELECT id, task_id, title, done, sort_order, created_at
        FROM subtasks
        WHERE task_id = ?
        ORDER BY sort_order ASC, created_at ASC
        "#,
    )
    .bind(&task.id)
    .fetch_all(pool)
    .await?;

    let open_questions = sqlx::query_as::<Any, OpenQuestionRecord>(
        r#"
        SELECT id, task_id, question, context, answer, status, asked_by, resolved_by, created_at, resolved_at
        FROM open_questions
        WHERE task_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(&task.id)
    .fetch_all(pool)
    .await?;

    let attachments = sqlx::query_as::<Any, AttachmentRecord>(
        r#"
        SELECT id, task_id, filename, content_type, size_bytes, storage_path, uploaded_by, created_at
        FROM attachments
        WHERE task_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(&task.id)
    .fetch_all(pool)
    .await?;

    let history = sqlx::query_as::<Any, TaskHistoryRecord>(
        r#"
        SELECT id, task_id, actor, action, detail, created_at
        FROM task_history
        WHERE task_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(&task.id)
    .fetch_all(pool)
    .await?;

    Ok(TaskDetails {
        task,
        labels,
        subtasks,
        open_questions,
        attachments,
        history,
    })
}

pub async fn add_subtask(
    pool: &AnyPool,
    project_slug: &str,
    task_ref: &str,
    title: &str,
    actor: &str,
) -> AppResult<SubtaskRecord> {
    let task_id = resolve_task_id(pool, project_slug, task_ref).await?;
    let trimmed_title = title.trim().to_string();
    if trimmed_title.is_empty() {
        return Err(AppError::BadRequest(
            "subtask title cannot be empty".to_string(),
        ));
    }

    let now = now_timestamp();
    let mut tx = pool.begin().await?;

    let sort_order: f64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(MAX(sort_order), 0) AS REAL) + 1.0 FROM subtasks WHERE task_id = ?",
    )
    .bind(&task_id)
    .fetch_one(&mut *tx)
    .await?;

    let subtask_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO subtasks (id, task_id, title, done, sort_order, created_at)
        VALUES (?, ?, ?, 0, ?, ?)
        "#,
    )
    .bind(&subtask_id)
    .bind(&task_id)
    .bind(&trimmed_title)
    .bind(sort_order)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    insert_history(
        &mut tx,
        &task_id,
        actor,
        "subtask.created",
        serde_json::json!({
            "subtask_id": subtask_id,
            "title": trimmed_title,
        }),
    )
    .await?;

    tx.commit().await?;

    get_subtask_by_id(pool, &task_id, &subtask_id).await
}

pub async fn update_subtask(
    pool: &AnyPool,
    project_slug: &str,
    task_ref: &str,
    subtask_id: &str,
    input: UpdateSubtaskInput,
) -> AppResult<SubtaskRecord> {
    let task_id = resolve_task_id(pool, project_slug, task_ref).await?;
    let existing = get_subtask_by_id(pool, &task_id, subtask_id).await?;

    let title = match input.title {
        Some(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                return Err(AppError::BadRequest(
                    "subtask title cannot be empty".to_string(),
                ));
            }
            trimmed
        }
        None => existing.title.clone(),
    };

    let done = input.done.map_or(existing.done, i64::from);
    let sort_order = input.sort_order.unwrap_or(existing.sort_order);

    let mut tx = pool.begin().await?;

    sqlx::query(
        "UPDATE subtasks SET title = ?, done = ?, sort_order = ? WHERE id = ? AND task_id = ?",
    )
    .bind(&title)
    .bind(done)
    .bind(sort_order)
    .bind(subtask_id)
    .bind(&task_id)
    .execute(&mut *tx)
    .await?;

    insert_history(
        &mut tx,
        &task_id,
        &input.actor,
        "subtask.updated",
        serde_json::json!({
            "subtask_id": subtask_id,
            "done": done == 1,
        }),
    )
    .await?;

    tx.commit().await?;

    get_subtask_by_id(pool, &task_id, subtask_id).await
}

pub async fn delete_subtask(
    pool: &AnyPool,
    project_slug: &str,
    task_ref: &str,
    subtask_id: &str,
    actor: &str,
) -> AppResult<()> {
    let task_id = resolve_task_id(pool, project_slug, task_ref).await?;

    let mut tx = pool.begin().await?;

    let result = sqlx::query("DELETE FROM subtasks WHERE id = ? AND task_id = ?")
        .bind(subtask_id)
        .bind(&task_id)
        .execute(&mut *tx)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "subtask '{subtask_id}' not found on task '{task_ref}'"
        )));
    }

    insert_history(
        &mut tx,
        &task_id,
        actor,
        "subtask.deleted",
        serde_json::json!({ "subtask_id": subtask_id }),
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn update_task(
    pool: &AnyPool,
    project_slug: &str,
    task_ref: &str,
    input: UpdateTaskInput,
) -> AppResult<TaskRecord> {
    let details = get_task_details(pool, project_slug, task_ref).await?;
    let task = details.task;

    let title = match input.title {
        Some(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                return Err(AppError::BadRequest(
                    "task title cannot be empty".to_string(),
                ));
            }
            trimmed
        }
        None => task.title,
    };

    let description = input.description.unwrap_or(task.description);

    let status = match input.status {
        Some(value) => {
            validate_status(&value)?;
            value
        }
        None => task.status,
    };

    let priority = match input.priority {
        Some(value) => {
            validate_priority(&value)?;
            value
        }
        None => task.priority,
    };

    let review_state = match input.review_state {
        Some(value) => {
            validate_review_state(&value)?;
            value
        }
        None => task.review_state,
    };

    let now = now_timestamp();
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        UPDATE tasks
        SET title = ?, description = ?, status = ?, priority = ?, review_state = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&title)
    .bind(&description)
    .bind(&status)
    .bind(&priority)
    .bind(&review_state)
    .bind(&now)
    .bind(&task.id)
    .execute(&mut *tx)
    .await?;

    if let Some(labels) = input.labels {
        sqlx::query("DELETE FROM task_labels WHERE task_id = ?")
            .bind(&task.id)
            .execute(&mut *tx)
            .await?;

        let normalized = normalized_labels(labels);
        for label in normalized {
            sqlx::query("INSERT INTO task_labels (task_id, label) VALUES (?, ?)")
                .bind(&task.id)
                .bind(label)
                .execute(&mut *tx)
                .await?;
        }
    }

    insert_history(
        &mut tx,
        &task.id,
        &input.actor,
        "task.updated",
        serde_json::json!({
            "status": status,
            "priority": priority,
            "review_state": review_state,
        }),
    )
    .await?;

    tx.commit().await?;

    get_task_record_by_id(pool, &task.id).await
}

pub async fn move_task(
    pool: &AnyPool,
    project_slug: &str,
    task_ref: &str,
    input: MoveTaskInput,
) -> AppResult<TaskRecord> {
    validate_status(&input.status)?;

    let task_id = resolve_task_id(pool, project_slug, task_ref).await?;
    let task = get_task_record_by_id(pool, &task_id).await?;

    if input.mcp_origin && task.review_state == "not_ready" {
        return Err(AppError::BadRequest(
            "task is not_ready, set review_state to ready before moving".to_string(),
        ));
    }

    let now = now_timestamp();
    let mut tx = pool.begin().await?;

    let sort_order = match input.sort_order {
        Some(value) => value,
        None => {
            sqlx::query_scalar::<Any, f64>(
                "SELECT CAST(COALESCE(MAX(sort_order), 0) AS REAL) + 1.0 FROM tasks WHERE project_id = ? AND status = ?",
            )
            .bind(&task.project_id)
            .bind(&input.status)
            .fetch_one(&mut *tx)
            .await?
        }
    };

    sqlx::query("UPDATE tasks SET status = ?, sort_order = ?, updated_at = ? WHERE id = ?")
        .bind(&input.status)
        .bind(sort_order)
        .bind(&now)
        .bind(&task.id)
        .execute(&mut *tx)
        .await?;

    insert_history(
        &mut tx,
        &task.id,
        &input.actor,
        "task.moved",
        serde_json::json!({
            "from_status": task.status,
            "to_status": input.status,
            "sort_order": sort_order,
        }),
    )
    .await?;

    tx.commit().await?;

    get_task_record_by_id(pool, &task.id).await
}

pub async fn delete_task(
    pool: &AnyPool,
    project_slug: &str,
    task_ref: &str,
    actor: &str,
) -> AppResult<()> {
    let task_id = resolve_task_id(pool, project_slug, task_ref).await?;

    let mut tx = pool.begin().await?;

    insert_history(
        &mut tx,
        &task_id,
        actor,
        "task.deleted",
        serde_json::json!({}),
    )
    .await?;

    let result = sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(&task_id)
        .execute(&mut *tx)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("task '{task_ref}' not found")));
    }

    tx.commit().await?;
    Ok(())
}

async fn project_summary_by_id(
    pool: &AnyPool,
    project_id: &str,
    project: ProjectRecord,
) -> AppResult<ProjectSummary> {
    let backlog_count = count_tasks_by_status(pool, project_id, "backlog").await?;
    let ready_count = count_tasks_by_status(pool, project_id, "ready").await?;
    let in_progress_count = count_tasks_by_status(pool, project_id, "in_progress").await?;
    let review_count = count_tasks_by_status(pool, project_id, "review").await?;
    let done_count = count_tasks_by_status(pool, project_id, "done").await?;

    let open_question_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM open_questions q
        INNER JOIN tasks t ON t.id = q.task_id
        WHERE t.project_id = ? AND q.status = 'open'
        "#,
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    let not_ready_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks WHERE project_id = ? AND review_state = 'not_ready'",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    Ok(ProjectSummary {
        project,
        backlog_count,
        ready_count,
        in_progress_count,
        review_count,
        done_count,
        open_question_count,
        not_ready_count,
    })
}

async fn count_tasks_by_status(pool: &AnyPool, project_id: &str, status: &str) -> AppResult<i64> {
    let count = sqlx::query_scalar::<Any, i64>(
        "SELECT COUNT(*) FROM tasks WHERE project_id = ? AND status = ?",
    )
    .bind(project_id)
    .bind(status)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

async fn project_id_by_slug(pool: &AnyPool, project_slug: &str) -> AppResult<String> {
    let project_id = sqlx::query_scalar::<Any, String>("SELECT id FROM projects WHERE slug = ?")
        .bind(project_slug)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("project '{project_slug}' not found")))?;

    Ok(project_id)
}

async fn resolve_task_id(pool: &AnyPool, project_slug: &str, task_ref: &str) -> AppResult<String> {
    match parse_task_ref(task_ref)? {
        TaskRef::Uuid(task_id) => {
            let result = sqlx::query_scalar::<Any, String>(
                r#"
                SELECT t.id
                FROM tasks t
                INNER JOIN projects p ON p.id = t.project_id
                WHERE p.slug = ? AND t.id = ?
                "#,
            )
            .bind(project_slug)
            .bind(task_id)
            .fetch_optional(pool)
            .await?;

            result.ok_or_else(|| AppError::NotFound(format!("task '{task_ref}' not found")))
        }
        TaskRef::DisplayKey { slug, task_number } => {
            if slug != project_slug {
                return Err(AppError::NotFound(format!(
                    "task '{task_ref}' is outside project '{project_slug}'"
                )));
            }

            let result = sqlx::query_scalar::<Any, String>(
                r#"
                SELECT t.id
                FROM tasks t
                INNER JOIN projects p ON p.id = t.project_id
                WHERE p.slug = ? AND t.task_number = ?
                "#,
            )
            .bind(project_slug)
            .bind(task_number)
            .fetch_optional(pool)
            .await?;

            result.ok_or_else(|| AppError::NotFound(format!("task '{task_ref}' not found")))
        }
    }
}

async fn get_open_question_by_id(
    pool: &AnyPool,
    task_id: &str,
    question_id: &str,
) -> AppResult<OpenQuestionRecord> {
    let record = sqlx::query_as::<Any, OpenQuestionRecord>(
        r#"
        SELECT id, task_id, question, context, answer, status, asked_by, resolved_by, created_at, resolved_at
        FROM open_questions
        WHERE id = ? AND task_id = ?
        "#,
    )
    .bind(question_id)
    .bind(task_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("question '{question_id}' not found")))?;

    Ok(record)
}

async fn get_subtask_by_id(
    pool: &AnyPool,
    task_id: &str,
    subtask_id: &str,
) -> AppResult<SubtaskRecord> {
    let subtask = sqlx::query_as::<Any, SubtaskRecord>(
        r#"
        SELECT id, task_id, title, done, sort_order, created_at
        FROM subtasks
        WHERE id = ? AND task_id = ?
        "#,
    )
    .bind(subtask_id)
    .bind(task_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("subtask '{subtask_id}' not found")))?;

    Ok(subtask)
}

async fn get_attachment_for_task(
    pool: &AnyPool,
    task_id: &str,
    attachment_id: &str,
) -> AppResult<AttachmentRecord> {
    let attachment = sqlx::query_as::<Any, AttachmentRecord>(
        r#"
        SELECT id, task_id, filename, content_type, size_bytes, storage_path, uploaded_by, created_at
        FROM attachments
        WHERE id = ? AND task_id = ?
        "#,
    )
    .bind(attachment_id)
    .bind(task_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("attachment '{attachment_id}' not found")))?;

    Ok(attachment)
}

async fn get_task_record_by_id(pool: &AnyPool, task_id: &str) -> AppResult<TaskRecord> {
    let task = sqlx::query_as::<Any, TaskRecord>(
        r#"
        SELECT
            id,
            project_id,
            task_number,
            title,
            description,
            status,
            priority,
            review_state,
            sort_order,
            created_by,
            created_at,
            updated_at
        FROM tasks
        WHERE id = ?
        "#,
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("task '{task_id}' not found")))?;

    Ok(task)
}

async fn insert_history(
    tx: &mut sqlx::Transaction<'_, Any>,
    task_id: &str,
    actor: &str,
    action: &str,
    detail: Value,
) -> AppResult<()> {
    let now = now_timestamp();
    let detail_json = detail.to_string();

    sqlx::query(
        r#"
        INSERT INTO task_history (id, task_id, actor, action, detail, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(task_id)
    .bind(actor)
    .bind(action)
    .bind(&detail_json)
    .bind(&now)
    .execute(&mut **tx)
    .await?;

    let inserted = sqlx::query(
        r#"
        INSERT INTO system_events (id, project_id, task_id, task_number, actor, action, detail, created_at)
        SELECT ?, t.project_id, t.id, t.task_number, ?, ?, ?, ?
        FROM tasks t
        WHERE t.id = ?
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(actor)
    .bind(action)
    .bind(&detail_json)
    .bind(&now)
    .bind(task_id)
    .execute(&mut **tx)
    .await?;

    if inserted.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("task '{task_id}' not found")));
    }

    Ok(())
}

async fn insert_project_event(
    tx: &mut sqlx::Transaction<'_, Any>,
    project_id: &str,
    actor: &str,
    action: &str,
    detail: Value,
) -> AppResult<()> {
    let now = now_timestamp();
    sqlx::query(
        r#"
        INSERT INTO system_events (id, project_id, task_id, task_number, actor, action, detail, created_at)
        VALUES (?, ?, NULL, NULL, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(project_id)
    .bind(actor)
    .bind(action)
    .bind(detail.to_string())
    .bind(now)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

fn now_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn validate_status(value: &str) -> AppResult<()> {
    match value {
        "backlog" | "ready" | "in_progress" | "review" | "done" => Ok(()),
        _ => Err(AppError::BadRequest(format!(
            "invalid task status '{value}'"
        ))),
    }
}

fn validate_priority(value: &str) -> AppResult<()> {
    match value {
        "low" | "medium" | "high" | "critical" => Ok(()),
        _ => Err(AppError::BadRequest(format!(
            "invalid task priority '{value}'"
        ))),
    }
}

fn validate_review_state(value: &str) -> AppResult<()> {
    match value {
        "ready" | "not_ready" => Ok(()),
        _ => Err(AppError::BadRequest(format!(
            "invalid review state '{value}'"
        ))),
    }
}

fn normalize_webhook_platform(value: &str) -> AppResult<String> {
    let platform = value.trim().to_ascii_lowercase();
    match platform.as_str() {
        "slack" | "discord" | "generic" => Ok(platform),
        _ => Err(AppError::BadRequest(format!(
            "invalid webhook platform '{value}'"
        ))),
    }
}

fn normalize_webhook_url(value: &str) -> AppResult<String> {
    let trimmed = value.trim();
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|_| AppError::BadRequest("webhook url must be a valid http(s) URL".to_string()))?;

    match parsed.scheme() {
        "http" | "https" => Ok(parsed.to_string()),
        _ => Err(AppError::BadRequest(
            "webhook url must use http or https".to_string(),
        )),
    }
}

fn normalize_webhook_events(events: Vec<String>) -> AppResult<Vec<String>> {
    let mut normalized = std::collections::BTreeSet::new();
    for event in events {
        let candidate = event.trim();
        if candidate.is_empty() {
            continue;
        }

        if !WEBHOOK_EVENTS.contains(&candidate) {
            return Err(AppError::BadRequest(format!(
                "invalid webhook event '{candidate}'"
            )));
        }
        normalized.insert(candidate.to_string());
    }

    if normalized.is_empty() {
        return Err(AppError::BadRequest(
            "webhook must subscribe to at least one event".to_string(),
        ));
    }

    Ok(normalized.into_iter().collect())
}

pub fn parse_webhook_events(raw: &str) -> AppResult<Vec<String>> {
    let parsed = serde_json::from_str::<Vec<String>>(raw).map_err(|error| {
        tracing::error!(error = ?error, raw, "failed to parse webhook events");
        AppError::Internal
    })?;

    normalize_webhook_events(parsed)
}

fn normalize_optional_secret(value: Option<String>) -> Option<String> {
    match value {
        Some(secret) => {
            let trimmed = secret.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        None => None,
    }
}

fn normalized_labels(labels: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for label in labels {
        let trimmed = label.trim();
        if trimmed.is_empty() {
            continue;
        }
        seen.insert(trimmed.to_string());
    }

    seen.into_iter().collect()
}

fn is_canonical_uuid(value: &str) -> bool {
    let parsed = match Uuid::parse_str(value) {
        Ok(uuid) => uuid,
        Err(_) => return false,
    };

    let canonical = parsed.hyphenated().to_string();
    value.eq_ignore_ascii_case(&canonical)
}

fn parse_display_key(value: &str) -> Option<(String, i64)> {
    let (slug, number) = value.split_once('-')?;
    if slug.is_empty()
        || !slug
            .chars()
            .all(|character| character.is_ascii_uppercase() || character.is_ascii_digit())
    {
        return None;
    }

    if number.starts_with('0') {
        return None;
    }

    let parsed_number: i64 = number.parse().ok()?;
    if parsed_number <= 0 {
        return None;
    }

    Some((slug.to_string(), parsed_number))
}

#[cfg(test)]
mod tests {
    use sqlx::AnyPool;
    use tempfile::tempdir;

    use crate::config::{Config, RateLimitConfig};
    use crate::db;
    use crate::db::queries;

    #[test]
    fn parse_task_ref_accepts_uuid_and_display_key() {
        let uuid = "123e4567-e89b-12d3-a456-426614174000";
        let parsed_uuid = queries::parse_task_ref(uuid).expect("uuid should parse");
        match parsed_uuid {
            queries::TaskRef::Uuid(value) => assert_eq!(value, uuid),
            _ => panic!("expected uuid task ref"),
        }

        let parsed_display =
            queries::parse_task_ref("LATTICE-42").expect("display key should parse");
        match parsed_display {
            queries::TaskRef::DisplayKey { slug, task_number } => {
                assert_eq!(slug, "LATTICE");
                assert_eq!(task_number, 42);
            }
            _ => panic!("expected display-key task ref"),
        }
    }

    #[test]
    fn parse_task_ref_rejects_invalid_display_key() {
        let result = queries::parse_task_ref("lattice-01");
        assert!(result.is_err());
    }

    #[test]
    fn normalize_slug_rejects_lowercase_and_symbols() {
        let normalized =
            queries::normalize_slug(" lattice-v1 ").expect("slug normalization should succeed");
        assert_eq!(normalized, "LATTICE-V1");

        assert!(queries::normalize_slug("bad_slug").is_err());
        assert!(queries::normalize_slug("-BAD").is_err());
    }

    async fn setup_db(db_name: &str) -> (tempfile::TempDir, AnyPool) {
        let temp_dir = tempdir().expect("tempdir should be created");
        let db_path = temp_dir.path().join(format!("{db_name}.db"));
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

        let config = Config {
            port: 7400,
            db_url,
            token: None,
            log_level: "info".to_string(),
            storage_dir: temp_dir.path().join("storage"),
            max_file_size: 10 * 1024 * 1024,
            rate_limits: RateLimitConfig::default(),
        };

        let pool = db::connect_and_migrate(&config)
            .await
            .expect("database should initialize");

        (temp_dir, pool)
    }

    #[tokio::test]
    async fn create_task_allocates_incrementing_numbers() {
        let (_temp_dir, pool) = setup_db("lattice-test").await;

        let project = queries::create_project_with_slug(&pool, "lattice", "goal", "LATTICE")
            .await
            .expect("project creation should succeed");

        let first_task = queries::create_task(
            &pool,
            &project.project.slug,
            queries::NewTaskInput {
                title: "first".to_string(),
                description: String::new(),
                status: "backlog".to_string(),
                priority: "medium".to_string(),
                review_state: "ready".to_string(),
                labels: Vec::new(),
                created_by: "human".to_string(),
            },
        )
        .await
        .expect("first task should be created");

        let second_task = queries::create_task(
            &pool,
            &project.project.slug,
            queries::NewTaskInput {
                title: "second".to_string(),
                description: String::new(),
                status: "backlog".to_string(),
                priority: "medium".to_string(),
                review_state: "ready".to_string(),
                labels: Vec::new(),
                created_by: "human".to_string(),
            },
        )
        .await
        .expect("second task should be created");

        assert_eq!(first_task.task_number, 1);
        assert_eq!(second_task.task_number, 2);
        assert_eq!(
            queries::display_key(&project.project.slug, second_task.task_number),
            "LATTICE-2"
        );
    }

    #[tokio::test]
    async fn update_spec_section_creates_revision() {
        let (_temp_dir, pool) = setup_db("spec-test").await;
        let project = queries::create_project_with_slug(&pool, "phase3spec", "goal", "PHASE3SPEC")
            .await
            .expect("project should be created");

        let updated = queries::update_spec_section(
            &pool,
            &project.project.slug,
            "overview",
            "# Overview",
            "human",
        )
        .await
        .expect("section update should succeed");
        assert_eq!(updated.section, "overview");
        assert_eq!(updated.content, "# Overview");

        let history = queries::list_spec_history(&pool, &project.project.slug, "overview", 50, 0)
            .await
            .expect("history should be listed");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "# Overview");
        assert_eq!(history[0].edited_by, "human");
    }

    #[tokio::test]
    async fn open_question_can_be_created_and_resolved() {
        let (_temp_dir, pool) = setup_db("questions-test").await;
        let project =
            queries::create_project_with_slug(&pool, "phase3questions", "goal", "PHASE3QUESTIONS")
                .await
                .expect("project should be created");

        let task = queries::create_task(
            &pool,
            &project.project.slug,
            queries::NewTaskInput {
                title: "question task".to_string(),
                description: String::new(),
                status: "backlog".to_string(),
                priority: "medium".to_string(),
                review_state: "ready".to_string(),
                labels: Vec::new(),
                created_by: "human".to_string(),
            },
        )
        .await
        .expect("task should be created");

        let task_ref = queries::display_key(&project.project.slug, task.task_number);
        let created = queries::create_open_question(
            &pool,
            &project.project.slug,
            &task_ref,
            "Use SSE?",
            "Need realtime notifications.",
            "human",
        )
        .await
        .expect("open question should be created");
        assert_eq!(created.status, "open");

        let open_questions =
            queries::list_project_open_questions(&pool, &project.project.slug, 50, 0)
                .await
                .expect("open question list should succeed");
        assert_eq!(open_questions.len(), 1);

        let resolved = queries::answer_open_question(
            &pool,
            &project.project.slug,
            &task_ref,
            &created.id,
            "Yes",
            "human",
        )
        .await
        .expect("open question should be resolved");
        assert_eq!(resolved.status, "resolved");
        assert_eq!(resolved.answer.as_deref(), Some("Yes"));

        let remaining = queries::list_project_open_questions(&pool, &project.project.slug, 50, 0)
            .await
            .expect("remaining open question list should succeed");
        assert!(remaining.is_empty());
    }

    #[tokio::test]
    async fn spec_update_writes_system_event() {
        let (_temp_dir, pool) = setup_db("spec-event-test").await;
        let project =
            queries::create_project_with_slug(&pool, "spec events", "goal", "SPEC-EVENTS")
                .await
                .expect("project should be created");

        queries::update_spec_section(
            &pool,
            &project.project.slug,
            "architecture",
            "## architecture",
            "human",
        )
        .await
        .expect("spec update should succeed");

        let events = queries::list_system_events(
            &pool,
            std::slice::from_ref(&project.project.slug),
            None,
            None,
            50,
        )
        .await
        .expect("events should be listed");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "spec.updated");
        assert!(events[0].task_id.is_none());
        assert!(events[0].task_number.is_none());
    }

    #[tokio::test]
    async fn goal_update_writes_system_event() {
        let (_temp_dir, pool) = setup_db("goal-event-test").await;
        let project =
            queries::create_project_with_slug(&pool, "goal events", "old goal", "GOAL-EVENTS")
                .await
                .expect("project should be created");

        queries::update_project(
            &pool,
            &project.project.slug,
            None,
            Some("new goal".to_string()),
            "human",
        )
        .await
        .expect("goal update should succeed");

        let events = queries::list_system_events(
            &pool,
            std::slice::from_ref(&project.project.slug),
            None,
            None,
            50,
        )
        .await
        .expect("events should be listed");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "goal.updated");
        assert!(events[0].task_id.is_none());
        assert!(events[0].task_number.is_none());
        assert!(events[0].detail.contains("\"from_goal\":\"old goal\""));
        assert!(events[0].detail.contains("\"to_goal\":\"new goal\""));
    }
}

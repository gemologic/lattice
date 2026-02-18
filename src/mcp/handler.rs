use axum::http::request::Parts;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Extensions, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData, Json, ServerHandler,
};
use serde::{Deserialize, Serialize};
use sqlx::AnyPool;

use crate::db::models::{
    OpenQuestionRecord, ProjectActivityRecord, ProjectQuestionRecord, ProjectSummary,
    SpecRevisionRecord, SpecSectionRecord, SubtaskRecord, TaskDetails, TaskRecord,
};
use crate::db::queries;
use crate::db::queries::{
    MoveTaskInput, NewTaskInput, TaskFilters, UpdateSubtaskInput, UpdateTaskInput,
};
use crate::error::{AppError, AppResult};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;
const DEFAULT_OFFSET: i64 = 0;
const DEFAULT_RECENT_LIMIT: i64 = 10;
const MAX_RECENT_LIMIT: i64 = 50;
const MAX_BULK_TASKS: usize = 100;

#[derive(Debug, Clone)]
pub struct LatticeMcpServer {
    db: AnyPool,
    tool_router: ToolRouter<Self>,
}

impl LatticeMcpServer {
    pub fn new(db: AnyPool) -> Self {
        Self {
            db,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for LatticeMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Lattice MCP server for project, spec, task, and question workflows.".to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[tool_router(router = tool_router)]
impl LatticeMcpServer {
    #[tool(
        name = "lattice_list_projects",
        description = "List projects with board and open-question summary counts."
    )]
    async fn lattice_list_projects(
        &self,
        Parameters(params): Parameters<ListProjectsInput>,
    ) -> Result<Json<ListProjectsOutput>, ErrorData> {
        let (limit, offset) = normalize_limit_offset(params.limit, params.offset)?;
        let projects = map_to_mcp(queries::list_projects(&self.db, limit, offset).await)?;
        let results = projects.into_iter().map(map_project_summary).collect();
        Ok(Json(ListProjectsOutput { projects: results }))
    }

    #[tool(
        name = "lattice_get_project",
        description = "Get project details and board summary counters."
    )]
    async fn lattice_get_project(
        &self,
        Parameters(params): Parameters<ProjectInput>,
    ) -> Result<Json<ProjectSummaryOutput>, ErrorData> {
        let project_slug = normalize_project_slug(&params.project)?;
        let project = map_to_mcp(queries::get_project(&self.db, &project_slug).await)?;
        Ok(Json(map_project_summary(project)))
    }

    #[tool(
        name = "lattice_create_project",
        description = "Create a project with explicit slug confirmation and optional initial spec."
    )]
    async fn lattice_create_project(
        &self,
        Parameters(params): Parameters<CreateProjectInput>,
        extensions: Extensions,
    ) -> Result<Json<ProjectSummaryOutput>, ErrorData> {
        if !params.confirm_slug {
            return Err(ErrorData::invalid_params(
                "confirm_slug must be true when creating a project from MCP",
                None,
            ));
        }

        let slug = normalize_project_slug(&params.slug)?;
        let goal = params.goal.unwrap_or_default();
        let actor = actor_from_extensions(&extensions);

        map_to_mcp(queries::create_project_with_slug(&self.db, &params.name, &goal, &slug).await)?;

        if let Some(initial_spec) = params.initial_spec {
            for (section, content) in initial_spec.into_sections() {
                map_to_mcp(
                    queries::update_spec_section(&self.db, &slug, section, &content, &actor).await,
                )?;
            }
        }

        let project = map_to_mcp(queries::get_project(&self.db, &slug).await)?;
        Ok(Json(map_project_summary(project)))
    }

    #[tool(
        name = "lattice_update_goal",
        description = "Update a project's goal text."
    )]
    async fn lattice_update_goal(
        &self,
        Parameters(params): Parameters<UpdateGoalInput>,
        extensions: Extensions,
    ) -> Result<Json<ProjectSummaryOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let actor = actor_from_extensions(&extensions);
        let updated = map_to_mcp(
            queries::update_project(&self.db, &slug, None, Some(params.goal), &actor).await,
        )?;
        Ok(Json(map_project_summary(updated)))
    }

    #[tool(
        name = "lattice_get_spec",
        description = "Get all structured spec sections for a project."
    )]
    async fn lattice_get_spec(
        &self,
        Parameters(params): Parameters<ProjectInput>,
    ) -> Result<Json<GetSpecOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let sections = map_to_mcp(queries::list_spec_sections(&self.db, &slug).await)?;
        Ok(Json(GetSpecOutput {
            sections: sections.into_iter().map(map_spec_section).collect(),
        }))
    }

    #[tool(
        name = "lattice_get_spec_section",
        description = "Get one spec section by name."
    )]
    async fn lattice_get_spec_section(
        &self,
        Parameters(params): Parameters<GetSpecSectionInput>,
    ) -> Result<Json<SpecSectionOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let section =
            map_to_mcp(queries::get_spec_section(&self.db, &slug, &params.section).await)?;
        Ok(Json(map_spec_section(section)))
    }

    #[tool(
        name = "lattice_update_spec_section",
        description = "Update one spec section and append a revision."
    )]
    async fn lattice_update_spec_section(
        &self,
        Parameters(params): Parameters<UpdateSpecSectionInput>,
        extensions: Extensions,
    ) -> Result<Json<SpecSectionOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let actor = actor_from_extensions(&extensions);
        let section = map_to_mcp(
            queries::update_spec_section(&self.db, &slug, &params.section, &params.content, &actor)
                .await,
        )?;
        Ok(Json(map_spec_section(section)))
    }

    #[tool(
        name = "lattice_get_spec_history",
        description = "Get revision history for one spec section."
    )]
    async fn lattice_get_spec_history(
        &self,
        Parameters(params): Parameters<GetSpecHistoryInput>,
    ) -> Result<Json<GetSpecHistoryOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let (limit, offset) = normalize_limit_offset(params.limit, params.offset)?;
        let revisions = map_to_mcp(
            queries::list_spec_history(&self.db, &slug, &params.section, limit, offset).await,
        )?;
        Ok(Json(GetSpecHistoryOutput {
            revisions: revisions.into_iter().map(map_spec_revision).collect(),
        }))
    }

    #[tool(
        name = "lattice_list_tasks",
        description = "List tasks by project, with optional status/label/review filters."
    )]
    async fn lattice_list_tasks(
        &self,
        Parameters(params): Parameters<ListTasksInput>,
    ) -> Result<Json<ListTasksOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let (limit, offset) = normalize_limit_offset(params.limit, params.offset)?;
        let tasks = map_to_mcp(
            queries::list_tasks(
                &self.db,
                &slug,
                TaskFilters {
                    status: params.status,
                    label: params.label,
                    review_state: params.review_state,
                },
                limit,
                offset,
            )
            .await,
        )?;
        let mapped = tasks
            .into_iter()
            .map(|task| map_task(&slug, task))
            .collect::<Vec<_>>();
        Ok(Json(ListTasksOutput { tasks: mapped }))
    }

    #[tool(
        name = "lattice_get_task",
        description = "Get full task details including subtasks, questions, attachments, and history."
    )]
    async fn lattice_get_task(
        &self,
        Parameters(params): Parameters<TaskRefInput>,
    ) -> Result<Json<TaskDetailsOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let details =
            map_to_mcp(queries::get_task_details(&self.db, &slug, &params.task_ref).await)?;
        Ok(Json(map_task_details(&slug, details)))
    }

    #[tool(
        name = "lattice_create_task",
        description = "Create a task and return its display key."
    )]
    async fn lattice_create_task(
        &self,
        Parameters(params): Parameters<CreateTaskInput>,
        extensions: Extensions,
    ) -> Result<Json<TaskOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let actor = actor_from_extensions(&extensions);
        let task = map_to_mcp(
            queries::create_task(
                &self.db,
                &slug,
                NewTaskInput {
                    title: params.title,
                    description: params.description.unwrap_or_default(),
                    status: params.status.unwrap_or_else(|| "backlog".to_string()),
                    priority: params.priority.unwrap_or_else(|| "medium".to_string()),
                    review_state: params.review_state.unwrap_or_else(|| "ready".to_string()),
                    labels: params.labels,
                    created_by: actor,
                },
            )
            .await,
        )?;
        Ok(Json(map_task(&slug, task)))
    }

    #[tool(
        name = "lattice_create_tasks_bulk",
        description = "Create multiple tasks in one call."
    )]
    async fn lattice_create_tasks_bulk(
        &self,
        Parameters(params): Parameters<CreateTasksBulkInput>,
        extensions: Extensions,
    ) -> Result<Json<ListTasksOutput>, ErrorData> {
        if params.tasks.is_empty() {
            return Err(ErrorData::invalid_params("tasks cannot be empty", None));
        }
        if params.tasks.len() > MAX_BULK_TASKS {
            return Err(ErrorData::invalid_params(
                "too many tasks in one bulk create, max is 100",
                None,
            ));
        }

        let slug = normalize_project_slug(&params.project)?;
        let actor = actor_from_extensions(&extensions);
        let mut created = Vec::with_capacity(params.tasks.len());
        for task in params.tasks {
            let item = map_to_mcp(
                queries::create_task(
                    &self.db,
                    &slug,
                    NewTaskInput {
                        title: task.title,
                        description: task.description.unwrap_or_default(),
                        status: task.status.unwrap_or_else(|| "backlog".to_string()),
                        priority: task.priority.unwrap_or_else(|| "medium".to_string()),
                        review_state: task.review_state.unwrap_or_else(|| "ready".to_string()),
                        labels: task.labels,
                        created_by: actor.clone(),
                    },
                )
                .await,
            )?;
            created.push(map_task(&slug, item));
        }

        Ok(Json(ListTasksOutput { tasks: created }))
    }

    #[tool(
        name = "lattice_update_task",
        description = "Update task fields, including labels."
    )]
    async fn lattice_update_task(
        &self,
        Parameters(params): Parameters<UpdateTaskToolInput>,
        extensions: Extensions,
    ) -> Result<Json<TaskOutput>, ErrorData> {
        if params.title.is_none()
            && params.description.is_none()
            && params.status.is_none()
            && params.priority.is_none()
            && params.review_state.is_none()
            && params.labels.is_none()
        {
            return Err(ErrorData::invalid_params(
                "at least one task field must be provided",
                None,
            ));
        }

        let slug = normalize_project_slug(&params.project)?;
        let actor = actor_from_extensions(&extensions);
        let updated = map_to_mcp(
            queries::update_task(
                &self.db,
                &slug,
                &params.task_ref,
                UpdateTaskInput {
                    title: params.title,
                    description: params.description,
                    status: params.status,
                    priority: params.priority,
                    review_state: params.review_state,
                    labels: params.labels,
                    actor,
                },
            )
            .await,
        )?;
        Ok(Json(map_task(&slug, updated)))
    }

    #[tool(
        name = "lattice_move_task",
        description = "Move a task to another board status and optional sort position."
    )]
    async fn lattice_move_task(
        &self,
        Parameters(params): Parameters<MoveTaskToolInput>,
        extensions: Extensions,
    ) -> Result<Json<TaskOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let actor = actor_from_extensions(&extensions);
        let moved = map_to_mcp(
            queries::move_task(
                &self.db,
                &slug,
                &params.task_ref,
                MoveTaskInput {
                    status: params.status,
                    sort_order: params.sort_order,
                    actor,
                    mcp_origin: true,
                },
            )
            .await,
        )?;
        Ok(Json(map_task(&slug, moved)))
    }

    #[tool(
        name = "lattice_delete_task",
        description = "Delete a task by UUID or display key."
    )]
    async fn lattice_delete_task(
        &self,
        Parameters(params): Parameters<TaskRefInput>,
        extensions: Extensions,
    ) -> Result<Json<DeleteOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let actor = actor_from_extensions(&extensions);
        map_to_mcp(queries::delete_task(&self.db, &slug, &params.task_ref, &actor).await)?;
        Ok(Json(DeleteOutput { deleted: true }))
    }

    #[tool(name = "lattice_add_subtask", description = "Add a subtask to a task.")]
    async fn lattice_add_subtask(
        &self,
        Parameters(params): Parameters<AddSubtaskInput>,
        extensions: Extensions,
    ) -> Result<Json<SubtaskOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let actor = actor_from_extensions(&extensions);
        let subtask = map_to_mcp(
            queries::add_subtask(&self.db, &slug, &params.task_ref, &params.title, &actor).await,
        )?;
        Ok(Json(map_subtask(subtask)))
    }

    #[tool(
        name = "lattice_update_subtask",
        description = "Update subtask title, done state, or sort order."
    )]
    async fn lattice_update_subtask(
        &self,
        Parameters(params): Parameters<UpdateSubtaskToolInput>,
        extensions: Extensions,
    ) -> Result<Json<SubtaskOutput>, ErrorData> {
        if params.title.is_none() && params.done.is_none() && params.sort_order.is_none() {
            return Err(ErrorData::invalid_params(
                "at least one subtask field must be provided",
                None,
            ));
        }

        let slug = normalize_project_slug(&params.project)?;
        let actor = actor_from_extensions(&extensions);
        let subtask = map_to_mcp(
            queries::update_subtask(
                &self.db,
                &slug,
                &params.task_ref,
                &params.subtask_id,
                UpdateSubtaskInput {
                    title: params.title,
                    done: params.done,
                    sort_order: params.sort_order,
                    actor,
                },
            )
            .await,
        )?;
        Ok(Json(map_subtask(subtask)))
    }

    #[tool(
        name = "lattice_delete_subtask",
        description = "Delete a subtask from a task."
    )]
    async fn lattice_delete_subtask(
        &self,
        Parameters(params): Parameters<DeleteSubtaskInput>,
        extensions: Extensions,
    ) -> Result<Json<DeleteOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let actor = actor_from_extensions(&extensions);
        map_to_mcp(
            queries::delete_subtask(
                &self.db,
                &slug,
                &params.task_ref,
                &params.subtask_id,
                &actor,
            )
            .await,
        )?;
        Ok(Json(DeleteOutput { deleted: true }))
    }

    #[tool(
        name = "lattice_list_open_questions",
        description = "List unresolved open questions for a project."
    )]
    async fn lattice_list_open_questions(
        &self,
        Parameters(params): Parameters<ListOpenQuestionsInput>,
    ) -> Result<Json<ListOpenQuestionsOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let (limit, offset) = normalize_limit_offset(params.limit, params.offset)?;
        let questions =
            map_to_mcp(queries::list_project_open_questions(&self.db, &slug, limit, offset).await)?;
        let mapped = questions
            .into_iter()
            .map(|question| map_project_open_question(&slug, question))
            .collect();
        Ok(Json(ListOpenQuestionsOutput { questions: mapped }))
    }

    #[tool(
        name = "lattice_ask_question",
        description = "Create an open question on a task."
    )]
    async fn lattice_ask_question(
        &self,
        Parameters(params): Parameters<AskQuestionInput>,
        extensions: Extensions,
    ) -> Result<Json<TaskOpenQuestionOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let actor = actor_from_extensions(&extensions);
        let question = map_to_mcp(
            queries::create_open_question(
                &self.db,
                &slug,
                &params.task_ref,
                &params.question,
                params.context.as_deref().unwrap_or_default(),
                &actor,
            )
            .await,
        )?;
        Ok(Json(map_task_open_question(question)))
    }

    #[tool(
        name = "lattice_answer_question",
        description = "Resolve an open question with an answer."
    )]
    async fn lattice_answer_question(
        &self,
        Parameters(params): Parameters<AnswerQuestionInput>,
        extensions: Extensions,
    ) -> Result<Json<TaskOpenQuestionOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let actor = actor_from_extensions(&extensions);
        let answered = map_to_mcp(
            queries::answer_open_question(
                &self.db,
                &slug,
                &params.task_ref,
                &params.question_id,
                &params.answer,
                &actor,
            )
            .await,
        )?;
        Ok(Json(map_task_open_question(answered)))
    }

    #[tool(
        name = "lattice_board_summary",
        description = "Return a compact board summary with counts and recent activity."
    )]
    async fn lattice_board_summary(
        &self,
        Parameters(params): Parameters<BoardSummaryInput>,
    ) -> Result<Json<BoardSummaryOutput>, ErrorData> {
        let slug = normalize_project_slug(&params.project)?;
        let recent_limit = normalize_recent_limit(params.recent_limit)?;
        let project = map_to_mcp(queries::get_project(&self.db, &slug).await)?;
        let activity =
            map_to_mcp(queries::list_recent_project_activity(&self.db, &slug, recent_limit).await)?;

        Ok(Json(BoardSummaryOutput {
            project: map_project(project.project),
            counts: BoardCountsOutput {
                backlog: project.backlog_count,
                ready: project.ready_count,
                in_progress: project.in_progress_count,
                review: project.review_count,
                done: project.done_count,
            },
            open_question_count: project.open_question_count,
            not_ready_count: project.not_ready_count,
            recent_activity: activity
                .into_iter()
                .map(|item| map_recent_activity(&slug, item))
                .collect(),
        }))
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListProjectsInput {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ProjectInput {
    project: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateProjectInput {
    name: String,
    slug: String,
    goal: Option<String>,
    confirm_slug: bool,
    initial_spec: Option<InitialSpecInput>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct InitialSpecInput {
    overview: Option<String>,
    requirements: Option<String>,
    architecture: Option<String>,
    technical_design: Option<String>,
    open_decisions: Option<String>,
    references: Option<String>,
}

impl InitialSpecInput {
    fn into_sections(self) -> Vec<(&'static str, String)> {
        let mut sections = Vec::new();
        if let Some(content) = self.overview {
            sections.push(("overview", content));
        }
        if let Some(content) = self.requirements {
            sections.push(("requirements", content));
        }
        if let Some(content) = self.architecture {
            sections.push(("architecture", content));
        }
        if let Some(content) = self.technical_design {
            sections.push(("technical_design", content));
        }
        if let Some(content) = self.open_decisions {
            sections.push(("open_decisions", content));
        }
        if let Some(content) = self.references {
            sections.push(("references", content));
        }
        sections
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateGoalInput {
    project: String,
    goal: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetSpecSectionInput {
    project: String,
    section: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateSpecSectionInput {
    project: String,
    section: String,
    content: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetSpecHistoryInput {
    project: String,
    section: String,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListTasksInput {
    project: String,
    status: Option<String>,
    label: Option<String>,
    review_state: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct TaskRefInput {
    project: String,
    task_ref: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateTaskInput {
    project: String,
    title: String,
    description: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    review_state: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateTasksBulkInput {
    project: String,
    tasks: Vec<CreateTaskBulkItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateTaskBulkItem {
    title: String,
    description: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    review_state: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateTaskToolInput {
    project: String,
    task_ref: String,
    title: Option<String>,
    description: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    review_state: Option<String>,
    labels: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MoveTaskToolInput {
    project: String,
    task_ref: String,
    status: String,
    sort_order: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AddSubtaskInput {
    project: String,
    task_ref: String,
    title: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateSubtaskToolInput {
    project: String,
    task_ref: String,
    subtask_id: String,
    title: Option<String>,
    done: Option<bool>,
    sort_order: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteSubtaskInput {
    project: String,
    task_ref: String,
    subtask_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListOpenQuestionsInput {
    project: String,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AskQuestionInput {
    project: String,
    task_ref: String,
    question: String,
    context: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AnswerQuestionInput {
    project: String,
    task_ref: String,
    question_id: String,
    answer: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct BoardSummaryInput {
    project: String,
    recent_limit: Option<i64>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct DeleteOutput {
    deleted: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ListProjectsOutput {
    projects: Vec<ProjectSummaryOutput>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ProjectSummaryOutput {
    project: ProjectOutput,
    backlog_count: i64,
    ready_count: i64,
    in_progress_count: i64,
    review_count: i64,
    done_count: i64,
    open_question_count: i64,
    not_ready_count: i64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ProjectOutput {
    id: String,
    slug: String,
    name: String,
    goal: String,
    task_counter: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct GetSpecOutput {
    sections: Vec<SpecSectionOutput>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct SpecSectionOutput {
    id: String,
    section: String,
    content: String,
    updated_at: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct GetSpecHistoryOutput {
    revisions: Vec<SpecRevisionOutput>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct SpecRevisionOutput {
    id: String,
    section: String,
    content: String,
    edited_by: String,
    created_at: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ListTasksOutput {
    tasks: Vec<TaskOutput>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct TaskOutput {
    id: String,
    display_key: String,
    task_number: i64,
    title: String,
    description: String,
    status: String,
    priority: String,
    review_state: String,
    sort_order: f64,
    created_by: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct TaskDetailsOutput {
    task: TaskOutput,
    labels: Vec<String>,
    subtasks: Vec<SubtaskOutput>,
    open_questions: Vec<TaskOpenQuestionOutput>,
    attachments: Vec<AttachmentOutput>,
    history: Vec<TaskHistoryOutput>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct SubtaskOutput {
    id: String,
    task_id: String,
    title: String,
    done: bool,
    sort_order: f64,
    created_at: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct TaskOpenQuestionOutput {
    id: String,
    task_id: String,
    question: String,
    context: String,
    answer: Option<String>,
    status: String,
    asked_by: String,
    resolved_by: Option<String>,
    created_at: String,
    resolved_at: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ProjectOpenQuestionOutput {
    id: String,
    task_id: String,
    task_number: i64,
    task_display_key: String,
    question: String,
    context: String,
    answer: Option<String>,
    status: String,
    asked_by: String,
    resolved_by: Option<String>,
    created_at: String,
    resolved_at: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ListOpenQuestionsOutput {
    questions: Vec<ProjectOpenQuestionOutput>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct AttachmentOutput {
    id: String,
    filename: String,
    content_type: String,
    size_bytes: i64,
    uploaded_by: String,
    created_at: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct TaskHistoryOutput {
    id: String,
    actor: String,
    action: String,
    detail: String,
    created_at: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct BoardSummaryOutput {
    project: ProjectOutput,
    counts: BoardCountsOutput,
    open_question_count: i64,
    not_ready_count: i64,
    recent_activity: Vec<RecentActivityOutput>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct BoardCountsOutput {
    backlog: i64,
    ready: i64,
    in_progress: i64,
    review: i64,
    done: i64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct RecentActivityOutput {
    task_id: String,
    task_number: i64,
    task_display_key: String,
    action: String,
    actor: String,
    created_at: String,
}

fn map_to_mcp<T>(result: AppResult<T>) -> Result<T, ErrorData> {
    result.map_err(map_error)
}

fn map_error(error: AppError) -> ErrorData {
    match error {
        AppError::BadRequest(message) => ErrorData::invalid_params(message, None),
        AppError::NotFound(message) => ErrorData::resource_not_found(message, None),
        AppError::Conflict(message) => ErrorData::invalid_request(message, None),
        AppError::Unauthorized => ErrorData::invalid_request("unauthorized", None),
        AppError::Internal => ErrorData::internal_error("unexpected error", None),
    }
}

fn normalize_project_slug(project: &str) -> Result<String, ErrorData> {
    queries::normalize_slug(project).map_err(map_error)
}

fn normalize_limit_offset(
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<(i64, i64), ErrorData> {
    let normalized_limit = limit.unwrap_or(DEFAULT_LIMIT);
    let normalized_offset = offset.unwrap_or(DEFAULT_OFFSET);

    if normalized_limit <= 0 || normalized_limit > MAX_LIMIT {
        return Err(ErrorData::invalid_params(
            "limit must be between 1 and 100",
            None,
        ));
    }

    if normalized_offset < 0 {
        return Err(ErrorData::invalid_params("offset cannot be negative", None));
    }

    Ok((normalized_limit, normalized_offset))
}

fn normalize_recent_limit(limit: Option<i64>) -> Result<i64, ErrorData> {
    let normalized = limit.unwrap_or(DEFAULT_RECENT_LIMIT);
    if normalized <= 0 || normalized > MAX_RECENT_LIMIT {
        return Err(ErrorData::invalid_params(
            "recent_limit must be between 1 and 50",
            None,
        ));
    }
    Ok(normalized)
}

fn actor_from_extensions(extensions: &Extensions) -> String {
    extensions
        .get::<Parts>()
        .and_then(|parts| parts.headers.get("MCP-Client"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "agent".to_string())
}

fn map_project_summary(value: ProjectSummary) -> ProjectSummaryOutput {
    ProjectSummaryOutput {
        project: map_project(value.project),
        backlog_count: value.backlog_count,
        ready_count: value.ready_count,
        in_progress_count: value.in_progress_count,
        review_count: value.review_count,
        done_count: value.done_count,
        open_question_count: value.open_question_count,
        not_ready_count: value.not_ready_count,
    }
}

fn map_project(value: crate::db::models::ProjectRecord) -> ProjectOutput {
    ProjectOutput {
        id: value.id,
        slug: value.slug,
        name: value.name,
        goal: value.goal,
        task_counter: value.task_counter,
        created_at: value.created_at,
        updated_at: value.updated_at,
    }
}

fn map_spec_section(value: SpecSectionRecord) -> SpecSectionOutput {
    SpecSectionOutput {
        id: value.id,
        section: value.section,
        content: value.content,
        updated_at: value.updated_at,
    }
}

fn map_spec_revision(value: SpecRevisionRecord) -> SpecRevisionOutput {
    SpecRevisionOutput {
        id: value.id,
        section: value.section,
        content: value.content,
        edited_by: value.edited_by,
        created_at: value.created_at,
    }
}

fn map_task(project_slug: &str, value: TaskRecord) -> TaskOutput {
    TaskOutput {
        id: value.id,
        display_key: queries::display_key(project_slug, value.task_number),
        task_number: value.task_number,
        title: value.title,
        description: value.description,
        status: value.status,
        priority: value.priority,
        review_state: value.review_state,
        sort_order: value.sort_order,
        created_by: value.created_by,
        created_at: value.created_at,
        updated_at: value.updated_at,
    }
}

fn map_task_details(project_slug: &str, value: TaskDetails) -> TaskDetailsOutput {
    TaskDetailsOutput {
        task: map_task(project_slug, value.task),
        labels: value.labels,
        subtasks: value.subtasks.into_iter().map(map_subtask).collect(),
        open_questions: value
            .open_questions
            .into_iter()
            .map(map_task_open_question)
            .collect(),
        attachments: value
            .attachments
            .into_iter()
            .map(|attachment| AttachmentOutput {
                id: attachment.id,
                filename: attachment.filename,
                content_type: attachment.content_type,
                size_bytes: attachment.size_bytes,
                uploaded_by: attachment.uploaded_by,
                created_at: attachment.created_at,
            })
            .collect(),
        history: value
            .history
            .into_iter()
            .map(|history| TaskHistoryOutput {
                id: history.id,
                actor: history.actor,
                action: history.action,
                detail: history.detail,
                created_at: history.created_at,
            })
            .collect(),
    }
}

fn map_subtask(value: SubtaskRecord) -> SubtaskOutput {
    SubtaskOutput {
        id: value.id,
        task_id: value.task_id,
        title: value.title,
        done: value.done == 1,
        sort_order: value.sort_order,
        created_at: value.created_at,
    }
}

fn map_task_open_question(value: OpenQuestionRecord) -> TaskOpenQuestionOutput {
    TaskOpenQuestionOutput {
        id: value.id,
        task_id: value.task_id,
        question: value.question,
        context: value.context,
        answer: value.answer,
        status: value.status,
        asked_by: value.asked_by,
        resolved_by: value.resolved_by,
        created_at: value.created_at,
        resolved_at: value.resolved_at,
    }
}

fn map_project_open_question(
    project_slug: &str,
    value: ProjectQuestionRecord,
) -> ProjectOpenQuestionOutput {
    ProjectOpenQuestionOutput {
        id: value.id,
        task_id: value.task_id,
        task_number: value.task_number,
        task_display_key: queries::display_key(project_slug, value.task_number),
        question: value.question,
        context: value.context,
        answer: value.answer,
        status: value.status,
        asked_by: value.asked_by,
        resolved_by: value.resolved_by,
        created_at: value.created_at,
        resolved_at: value.resolved_at,
    }
}

fn map_recent_activity(project_slug: &str, value: ProjectActivityRecord) -> RecentActivityOutput {
    RecentActivityOutput {
        task_id: value.task_id,
        task_number: value.task_number,
        task_display_key: queries::display_key(project_slug, value.task_number),
        action: value.action,
        actor: value.actor,
        created_at: value.created_at,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use reqwest::header::{ACCEPT, CONTENT_TYPE};
    use reqwest::StatusCode;
    use serde_json::json;
    use tempfile::tempdir;

    use crate::api;
    use crate::config::{Config, RateLimitConfig};
    use crate::db;
    use crate::db::queries;
    use crate::mcp;
    use crate::state::AppState;

    #[tokio::test]
    async fn streamable_http_mcp_tools_list_and_call_work() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let db_path = temp_dir.path().join("phase4_mcp_test.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

        let config = Config {
            port: 0,
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
        queries::create_project_with_slug(&pool, "Phase 4", "MCP tests", "PHASE4")
            .await
            .expect("project should be created");

        let state = AppState::new(config, pool);
        let app = Router::new()
            .nest_service("/mcp", mcp::service(state.clone()))
            .nest("/api/v1", api::router())
            .route("/healthz", get(api::healthz))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                api::auth::require_auth,
            ))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener
            .local_addr()
            .expect("listener addr should be readable");
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("client should build");
        let base = format!("http://{addr}/mcp");

        let init = client
            .post(&base)
            .header(ACCEPT, "application/json, text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .header("MCP-Client", "phase4-e2e-agent")
            .body(
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "protocolVersion": "2025-06-18",
                        "capabilities": {},
                        "clientInfo": {
                            "name": "phase4-e2e",
                            "version": "0.1.0"
                        }
                    }
                })
                .to_string(),
            )
            .send()
            .await
            .expect("initialize request should succeed");
        assert_eq!(init.status(), StatusCode::OK);
        let session_id = init
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned)
            .expect("session header should exist");
        let _init_body = init
            .text()
            .await
            .expect("initialize body should be readable");

        let initialized = client
            .post(&base)
            .header(ACCEPT, "application/json, text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .header("Mcp-Session-Id", &session_id)
            .header("MCP-Client", "phase4-e2e-agent")
            .body(
                json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/initialized"
                })
                .to_string(),
            )
            .send()
            .await
            .expect("initialized notification should succeed");
        assert_eq!(initialized.status(), StatusCode::ACCEPTED);

        let tools_list = client
            .post(&base)
            .header(ACCEPT, "application/json, text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .header("Mcp-Session-Id", &session_id)
            .header("MCP-Client", "phase4-e2e-agent")
            .body(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/list",
                    "params": {}
                })
                .to_string(),
            )
            .send()
            .await
            .expect("tools/list should succeed");
        assert_eq!(tools_list.status(), StatusCode::OK);
        let tools_body = tools_list
            .text()
            .await
            .expect("tools list body should be readable");
        assert!(
            tools_body.contains("lattice_board_summary"),
            "tools list should expose lattice_board_summary"
        );
        assert!(
            tools_body.contains("lattice_create_project"),
            "tools list should expose lattice_create_project"
        );

        let create_task = client
            .post(&base)
            .header(ACCEPT, "application/json, text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .header("Mcp-Session-Id", &session_id)
            .header("MCP-Client", "phase4-e2e-agent")
            .body(
                json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "method": "tools/call",
                    "params": {
                        "name": "lattice_create_task",
                        "arguments": {
                            "project": "PHASE4",
                            "title": "verify actor propagation"
                        }
                    }
                })
                .to_string(),
            )
            .send()
            .await
            .expect("tools/call should succeed");
        assert_eq!(create_task.status(), StatusCode::OK);
        let call_body = create_task
            .text()
            .await
            .expect("call body should be readable");
        let json_line = call_body
            .lines()
            .find_map(|line| line.strip_prefix("data: {"))
            .map(|line| format!("{{{line}"))
            .expect("tools/call should include an SSE data JSON payload");
        let payload: serde_json::Value =
            serde_json::from_str(&json_line).expect("payload should deserialize");

        assert_eq!(
            payload
                .pointer("/result/structuredContent/created_by")
                .and_then(serde_json::Value::as_str),
            Some("phase4-e2e-agent")
        );
        assert_eq!(
            payload
                .pointer("/result/structuredContent/display_key")
                .and_then(serde_json::Value::as_str)
                .map(|value| value.starts_with("PHASE4-")),
            Some(true)
        );

        server.abort();
    }
}

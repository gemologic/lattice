const API_BASE = '/api/v1';

interface ApiErrorBody {
  error: string;
  message: string;
}

export type TaskStatus = 'backlog' | 'ready' | 'in_progress' | 'review' | 'done';
export type TaskPriority = 'low' | 'medium' | 'high' | 'critical';
export type ReviewState = 'ready' | 'not_ready';

export interface ProjectRecord {
  id: string;
  slug: string;
  name: string;
  goal: string;
  task_counter: number;
  created_at: string;
  updated_at: string;
}

export interface ProjectSummary {
  project: ProjectRecord;
  backlog_count: number;
  ready_count: number;
  in_progress_count: number;
  review_count: number;
  done_count: number;
  open_question_count: number;
  not_ready_count: number;
}

export interface TaskResponse {
  id: string;
  display_key: string;
  task_number: number;
  title: string;
  description: string;
  status: TaskStatus;
  priority: TaskPriority;
  review_state: ReviewState;
  sort_order: number;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface SubtaskRecord {
  id: string;
  task_id: string;
  title: string;
  done: boolean;
  sort_order: number;
  created_at: string;
}

export interface OpenQuestionRecord {
  id: string;
  task_id: string;
  question: string;
  context: string;
  answer: string | null;
  status: 'open' | 'resolved';
  asked_by: string;
  resolved_by: string | null;
  created_at: string;
  resolved_at: string | null;
}

export interface AttachmentRecord {
  id: string;
  task_id: string;
  filename: string;
  content_type: string;
  size_bytes: number;
  storage_path: string;
  uploaded_by: string;
  created_at: string;
}

export interface TaskHistoryRecord {
  id: string;
  task_id: string;
  actor: string;
  action: string;
  detail: string;
  created_at: string;
}

export interface TaskEventPayload {
  id: string;
  project: string;
  task_id: string | null;
  task_number: number | null;
  task_display_key: string | null;
  action: string;
  actor: string;
  detail: unknown;
  created_at: string;
}

export interface TaskDetailsResponse {
  task: TaskResponse;
  labels: string[];
  subtasks: SubtaskRecord[];
  open_questions: OpenQuestionRecord[];
  attachments: AttachmentRecord[];
  history: TaskHistoryRecord[];
}

export interface ProjectOpenQuestionResponse {
  id: string;
  task_id: string;
  task_number: number;
  task_display_key: string;
  question: string;
  context: string;
  answer: string | null;
  status: 'open' | 'resolved';
  asked_by: string;
  resolved_by: string | null;
  created_at: string;
  resolved_at: string | null;
}

export interface SpecSectionRecord {
  id: string;
  project_id: string;
  section: 'overview' | 'requirements' | 'architecture' | 'technical_design' | 'open_decisions' | 'references';
  content: string;
  updated_at: string;
}

export interface SpecRevisionRecord {
  id: string;
  project_id: string;
  section: string;
  content: string;
  edited_by: string;
  created_at: string;
}

export type WebhookPlatform = 'slack' | 'discord' | 'generic';

export interface WebhookResponse {
  id: string;
  name: string;
  url: string;
  platform: WebhookPlatform;
  events: string[];
  active: boolean;
  has_secret: boolean;
  created_at: string;
  updated_at: string;
}

export interface ListTaskFilters {
  status?: TaskStatus;
  label?: string;
  review_state?: ReviewState;
  limit?: number;
  offset?: number;
}

export interface UpdateTaskPayload {
  title?: string;
  description?: string;
  status?: TaskStatus;
  priority?: TaskPriority;
  review_state?: ReviewState;
  labels?: string[];
}

export interface UpdateSubtaskPayload {
  title?: string;
  done?: boolean;
  sort_order?: number;
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, init);
  const contentType = response.headers.get('content-type');

  if (!response.ok) {
    let message = `request failed with status ${response.status}`;
    if (contentType?.includes('application/json')) {
      const body = (await response.json()) as ApiErrorBody;
      if (typeof body.message === 'string' && body.message.length > 0) {
        message = body.message;
      }
    } else {
      const text = await response.text();
      if (text.trim().length > 0) {
        message = text;
      }
    }

    throw new Error(message);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  if (contentType?.includes('application/json')) {
    return (await response.json()) as T;
  }

  return undefined as T;
}

function toQuery(params: Record<string, string | number | undefined>): string {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined) {
      continue;
    }
    query.set(key, String(value));
  }

  const encoded = query.toString();
  return encoded.length > 0 ? `?${encoded}` : '';
}

function withJsonBody(body: unknown): RequestInit {
  return {
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(body),
  };
}

export async function listProjects(limit = 50, offset = 0): Promise<ProjectSummary[]> {
  return request<ProjectSummary[]>(`/projects${toQuery({ limit, offset })}`);
}

export interface CreateProjectPayload {
  name: string;
  slug: string;
  goal: string;
}

export async function createProject(payload: CreateProjectPayload): Promise<ProjectSummary> {
  return request<ProjectSummary>('/projects', {
    method: 'POST',
    ...withJsonBody(payload),
  });
}

export async function listTasks(project: string, filters: ListTaskFilters = {}): Promise<TaskResponse[]> {
  return request<TaskResponse[]>(
    `/projects/${encodeURIComponent(project)}/tasks${toQuery({
      status: filters.status,
      label: filters.label,
      review_state: filters.review_state,
      limit: filters.limit ?? 100,
      offset: filters.offset ?? 0,
    })}`,
  );
}

export async function getTask(project: string, taskRef: string): Promise<TaskDetailsResponse> {
  return request<TaskDetailsResponse>(`/projects/${encodeURIComponent(project)}/tasks/${encodeURIComponent(taskRef)}`);
}

export async function moveTask(project: string, taskRef: string, status: TaskStatus): Promise<TaskResponse> {
  return request<TaskResponse>(`/projects/${encodeURIComponent(project)}/tasks/${encodeURIComponent(taskRef)}/move`, {
    method: 'POST',
    ...withJsonBody({ status }),
  });
}

export async function updateTask(project: string, taskRef: string, payload: UpdateTaskPayload): Promise<TaskResponse> {
  return request<TaskResponse>(`/projects/${encodeURIComponent(project)}/tasks/${encodeURIComponent(taskRef)}`, {
    method: 'PATCH',
    ...withJsonBody(payload),
  });
}

export async function addSubtask(project: string, taskRef: string, title: string): Promise<SubtaskRecord> {
  return request<SubtaskRecord>(
    `/projects/${encodeURIComponent(project)}/tasks/${encodeURIComponent(taskRef)}/subtasks`,
    {
      method: 'POST',
      ...withJsonBody({ title }),
    },
  );
}

export async function updateSubtask(
  project: string,
  taskRef: string,
  subtaskId: string,
  payload: UpdateSubtaskPayload,
): Promise<SubtaskRecord> {
  return request<SubtaskRecord>(
    `/projects/${encodeURIComponent(project)}/tasks/${encodeURIComponent(taskRef)}/subtasks/${encodeURIComponent(subtaskId)}`,
    {
      method: 'PATCH',
      ...withJsonBody(payload),
    },
  );
}

export async function deleteSubtask(project: string, taskRef: string, subtaskId: string): Promise<void> {
  await request<void>(
    `/projects/${encodeURIComponent(project)}/tasks/${encodeURIComponent(taskRef)}/subtasks/${encodeURIComponent(subtaskId)}`,
    {
      method: 'DELETE',
    },
  );
}

export async function askQuestion(
  project: string,
  taskRef: string,
  question: string,
  context: string,
): Promise<OpenQuestionRecord> {
  return request<OpenQuestionRecord>(
    `/projects/${encodeURIComponent(project)}/tasks/${encodeURIComponent(taskRef)}/questions`,
    {
      method: 'POST',
      ...withJsonBody({ question, context }),
    },
  );
}

export async function answerQuestion(
  project: string,
  taskRef: string,
  questionId: string,
  answer: string,
): Promise<OpenQuestionRecord> {
  return request<OpenQuestionRecord>(
    `/projects/${encodeURIComponent(project)}/tasks/${encodeURIComponent(taskRef)}/questions/${encodeURIComponent(questionId)}`,
    {
      method: 'PATCH',
      ...withJsonBody({ answer }),
    },
  );
}

export async function listOpenQuestions(
  project: string,
  limit = 100,
  offset = 0,
): Promise<ProjectOpenQuestionResponse[]> {
  return request<ProjectOpenQuestionResponse[]>(
    `/projects/${encodeURIComponent(project)}/questions${toQuery({ limit, offset })}`,
  );
}

export async function listSpecSections(project: string): Promise<SpecSectionRecord[]> {
  return request<SpecSectionRecord[]>(`/projects/${encodeURIComponent(project)}/spec`);
}

export async function updateSpecSection(
  project: string,
  section: SpecSectionRecord['section'],
  content: string,
): Promise<SpecSectionRecord> {
  return request<SpecSectionRecord>(`/projects/${encodeURIComponent(project)}/spec/${section}`, {
    method: 'PUT',
    ...withJsonBody({ content }),
  });
}

export async function listSpecHistory(
  project: string,
  section: SpecSectionRecord['section'],
  limit = 50,
  offset = 0,
): Promise<SpecRevisionRecord[]> {
  return request<SpecRevisionRecord[]>(
    `/projects/${encodeURIComponent(project)}/spec/${section}/history${toQuery({ limit, offset })}`,
  );
}

export function projectEventsPath(project: string): string {
  return `${API_BASE}/projects/${encodeURIComponent(project)}/events`;
}

export async function uploadAttachment(project: string, taskRef: string, file: File): Promise<AttachmentRecord> {
  const body = new FormData();
  body.append('file', file, file.name);

  return request<AttachmentRecord>(
    `/projects/${encodeURIComponent(project)}/tasks/${encodeURIComponent(taskRef)}/attachments`,
    {
      method: 'POST',
      body,
    },
  );
}

export async function deleteAttachment(project: string, taskRef: string, attachmentId: string): Promise<void> {
  await request<void>(
    `/projects/${encodeURIComponent(project)}/tasks/${encodeURIComponent(taskRef)}/attachments/${encodeURIComponent(attachmentId)}`,
    {
      method: 'DELETE',
    },
  );
}

export async function listWebhooks(project: string): Promise<WebhookResponse[]> {
  return request<WebhookResponse[]>(`/projects/${encodeURIComponent(project)}/webhooks`);
}

export interface CreateWebhookPayload {
  name: string;
  url: string;
  platform: WebhookPlatform;
  events: string[];
  secret?: string;
  active?: boolean;
}

export async function createWebhook(project: string, payload: CreateWebhookPayload): Promise<WebhookResponse> {
  return request<WebhookResponse>(`/projects/${encodeURIComponent(project)}/webhooks`, {
    method: 'POST',
    ...withJsonBody(payload),
  });
}

export interface UpdateWebhookPayload {
  name?: string;
  url?: string;
  platform?: WebhookPlatform;
  events?: string[];
  secret?: string;
  active?: boolean;
}

export async function updateWebhook(
  project: string,
  webhookId: string,
  payload: UpdateWebhookPayload,
): Promise<WebhookResponse> {
  return request<WebhookResponse>(
    `/projects/${encodeURIComponent(project)}/webhooks/${encodeURIComponent(webhookId)}`,
    {
      method: 'PATCH',
      ...withJsonBody(payload),
    },
  );
}

export async function deleteWebhook(project: string, webhookId: string): Promise<void> {
  await request<void>(`/projects/${encodeURIComponent(project)}/webhooks/${encodeURIComponent(webhookId)}`, {
    method: 'DELETE',
  });
}

export async function testWebhook(project: string, webhookId: string): Promise<void> {
  await request<void>(`/projects/${encodeURIComponent(project)}/webhooks/${encodeURIComponent(webhookId)}/test`, {
    method: 'POST',
  });
}

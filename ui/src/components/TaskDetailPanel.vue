<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import Button from 'primevue/button';
import Dialog from 'primevue/dialog';
import InputText from 'primevue/inputtext';
import Message from 'primevue/message';
import Tag from 'primevue/tag';
import Textarea from 'primevue/textarea';

import {
  addSubtask,
  answerQuestion,
  askQuestion,
  deleteAttachment,
  deleteSubtask,
  getTask,
  type ReviewState,
  type SubtaskRecord,
  type TaskDetailsResponse,
  type TaskPriority,
  type TaskStatus,
  uploadAttachment,
  updateSubtask,
  updateTask,
} from '../api/lattice';

interface Props {
  slug: string;
  taskRef: string | null;
  visible: boolean;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'update:visible': [value: boolean];
  changed: [];
}>();

const detail = ref<TaskDetailsResponse | null>(null);
const error = ref<string | null>(null);
const loading = ref(false);
const saving = ref(false);
const busy = ref(false);

const form = reactive<{
  title: string;
  description: string;
  status: TaskStatus;
  priority: TaskPriority;
  reviewState: ReviewState;
}>({
  title: '',
  description: '',
  status: 'backlog',
  priority: 'medium',
  reviewState: 'ready',
});

const newSubtaskTitle = ref('');
const newQuestion = ref('');
const newQuestionContext = ref('');
const questionDrafts = reactive<Record<string, string>>({});

const openQuestions = computed(() => {
  return detail.value?.open_questions.filter((question) => question.status === 'open') ?? [];
});

const resolvedQuestions = computed(() => {
  return detail.value?.open_questions.filter((question) => question.status === 'resolved') ?? [];
});

const displayKey = computed(() => detail.value?.task.display_key ?? props.taskRef ?? '');

const statusOptions: Array<{ value: TaskStatus; label: string }> = [
  { value: 'backlog', label: 'Backlog' },
  { value: 'ready', label: 'Ready' },
  { value: 'in_progress', label: 'In Progress' },
  { value: 'review', label: 'Review' },
  { value: 'done', label: 'Done' },
];

const priorityOptions: Array<{ value: TaskPriority; label: string }> = [
  { value: 'low', label: 'Low' },
  { value: 'medium', label: 'Medium' },
  { value: 'high', label: 'High' },
  { value: 'critical', label: 'Critical' },
];

const reviewOptions: Array<{ value: ReviewState; label: string }> = [
  { value: 'ready', label: 'Ready' },
  { value: 'not_ready', label: 'Not Ready' },
];

watch(
  () => [props.visible, props.slug, props.taskRef] as const,
  async ([visible, slug, taskRef]) => {
    if (!visible || !taskRef) {
      return;
    }

    await loadTask(slug, taskRef);
  },
  { immediate: true },
);

async function loadTask(slug: string, taskRef: string): Promise<void> {
  loading.value = true;
  error.value = null;

  try {
    const next = await getTask(slug, taskRef);
    detail.value = next;
    syncForm(next);
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load task details';
  } finally {
    loading.value = false;
  }
}

function syncForm(current: TaskDetailsResponse): void {
  form.title = current.task.title;
  form.description = current.task.description;
  form.status = current.task.status;
  form.priority = current.task.priority;
  form.reviewState = current.task.review_state;
}

function closePanel(nextVisible: boolean): void {
  emit('update:visible', nextVisible);
}

async function saveTask(): Promise<void> {
  if (!detail.value) {
    return;
  }

  const payload: {
    title?: string;
    description?: string;
    status?: TaskStatus;
    priority?: TaskPriority;
    review_state?: ReviewState;
  } = {};

  if (form.title !== detail.value.task.title) {
    payload.title = form.title;
  }

  if (form.description !== detail.value.task.description) {
    payload.description = form.description;
  }

  if (form.status !== detail.value.task.status) {
    payload.status = form.status;
  }

  if (form.priority !== detail.value.task.priority) {
    payload.priority = form.priority;
  }

  if (form.reviewState !== detail.value.task.review_state) {
    payload.review_state = form.reviewState;
  }

  if (Object.keys(payload).length === 0) {
    return;
  }

  saving.value = true;
  error.value = null;
  try {
    const updated = await updateTask(props.slug, detail.value.task.id, payload);
    detail.value = { ...detail.value, task: updated };
    syncForm(detail.value);
    emit('changed');
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to update task';
  } finally {
    saving.value = false;
  }
}

async function refreshCurrentTask(): Promise<void> {
  if (!props.taskRef) {
    return;
  }

  await loadTask(props.slug, props.taskRef);
}

async function addSubtaskToTask(): Promise<void> {
  const title = newSubtaskTitle.value.trim();
  if (!detail.value || title.length === 0) {
    return;
  }

  busy.value = true;
  error.value = null;
  try {
    await addSubtask(props.slug, detail.value.task.id, title);
    newSubtaskTitle.value = '';
    await refreshCurrentTask();
    emit('changed');
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to add subtask';
  } finally {
    busy.value = false;
  }
}

async function toggleSubtask(subtask: SubtaskRecord): Promise<void> {
  if (!detail.value) {
    return;
  }

  busy.value = true;
  error.value = null;
  try {
    await updateSubtask(props.slug, detail.value.task.id, subtask.id, {
      done: !subtask.done,
    });
    await refreshCurrentTask();
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to update subtask';
  } finally {
    busy.value = false;
  }
}

async function removeSubtask(subtaskId: string): Promise<void> {
  if (!detail.value) {
    return;
  }

  busy.value = true;
  error.value = null;
  try {
    await deleteSubtask(props.slug, detail.value.task.id, subtaskId);
    await refreshCurrentTask();
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to delete subtask';
  } finally {
    busy.value = false;
  }
}

async function askTaskQuestion(): Promise<void> {
  const question = newQuestion.value.trim();
  if (!detail.value || question.length === 0) {
    return;
  }

  busy.value = true;
  error.value = null;
  try {
    await askQuestion(props.slug, detail.value.task.id, question, newQuestionContext.value.trim());
    newQuestion.value = '';
    newQuestionContext.value = '';
    await refreshCurrentTask();
    emit('changed');
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to create open question';
  } finally {
    busy.value = false;
  }
}

async function resolveQuestion(questionId: string): Promise<void> {
  const answer = (questionDrafts[questionId] ?? '').trim();
  if (!detail.value || answer.length === 0) {
    return;
  }

  busy.value = true;
  error.value = null;
  try {
    await answerQuestion(props.slug, detail.value.task.id, questionId, answer);
    questionDrafts[questionId] = '';
    await refreshCurrentTask();
    emit('changed');
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to resolve open question';
  } finally {
    busy.value = false;
  }
}

async function uploadTaskAttachment(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!detail.value || !file) {
    return;
  }

  busy.value = true;
  error.value = null;
  try {
    await uploadAttachment(props.slug, detail.value.task.id, file);
    input.value = '';
    await refreshCurrentTask();
    emit('changed');
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to upload attachment';
  } finally {
    busy.value = false;
  }
}

async function removeAttachment(attachmentId: string): Promise<void> {
  if (!detail.value) {
    return;
  }

  busy.value = true;
  error.value = null;
  try {
    await deleteAttachment(props.slug, detail.value.task.id, attachmentId);
    await refreshCurrentTask();
    emit('changed');
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to delete attachment';
  } finally {
    busy.value = false;
  }
}

function formatTimestamp(value: string): string {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }

  return parsed.toLocaleString();
}

function compactDetail(value: string): string {
  if (value.trim().length === 0 || value.trim() === '{}') {
    return '';
  }

  try {
    const parsed = JSON.parse(value) as Record<string, unknown>;
    if (Object.keys(parsed).length === 0) {
      return '';
    }
    return JSON.stringify(parsed);
  } catch {
    return value;
  }
}
</script>

<template>
  <Dialog
    :visible="visible"
    modal
    :draggable="false"
    :dismissable-mask="true"
    :style="{ width: 'min(980px, 96vw)' }"
    :header="displayKey"
    class="task-detail-dialog"
    @update:visible="closePanel"
  >
    <div class="task-detail-layout">
      <Message v-if="error" severity="error" :closable="false">
        {{ error }}
      </Message>

      <div v-if="loading" class="detail-loading">Loading task details...</div>

      <template v-else-if="detail">
        <section class="detail-section">
          <div class="detail-header-row">
            <h3>Task</h3>
            <div class="detail-tag-row">
              <Tag :value="detail.task.status" severity="info" />
              <Tag :value="detail.task.priority" severity="warn" />
              <Tag :value="detail.task.review_state" severity="contrast" />
            </div>
          </div>

          <label class="field-label" for="task-title">Title</label>
          <InputText id="task-title" v-model="form.title" />

          <label class="field-label" for="task-description">Description</label>
          <Textarea id="task-description" v-model="form.description" auto-resize rows="6" />

          <div class="task-form-row">
            <label>
              <span class="field-label">Status</span>
              <select v-model="form.status" class="task-select">
                <option v-for="item in statusOptions" :key="item.value" :value="item.value">
                  {{ item.label }}
                </option>
              </select>
            </label>

            <label>
              <span class="field-label">Priority</span>
              <select v-model="form.priority" class="task-select">
                <option v-for="item in priorityOptions" :key="item.value" :value="item.value">
                  {{ item.label }}
                </option>
              </select>
            </label>

            <label>
              <span class="field-label">Review State</span>
              <select v-model="form.reviewState" class="task-select">
                <option v-for="item in reviewOptions" :key="item.value" :value="item.value">
                  {{ item.label }}
                </option>
              </select>
            </label>
          </div>

          <div class="detail-actions">
            <Button
              label="Save Task"
              icon="pi pi-save"
              :loading="saving"
              :disabled="busy || loading"
              @click="saveTask"
            />
          </div>
        </section>

        <section class="detail-section">
          <h3>Subtasks</h3>
          <ul class="subtask-list">
            <li v-for="subtask in detail.subtasks" :key="subtask.id" class="subtask-item">
              <label class="subtask-check">
                <input type="checkbox" :checked="subtask.done" :disabled="busy" @change="toggleSubtask(subtask)" />
                <span :class="['subtask-title', subtask.done ? 'done' : '']">{{ subtask.title }}</span>
              </label>
              <Button
                icon="pi pi-trash"
                text
                severity="secondary"
                :disabled="busy"
                @click="removeSubtask(subtask.id)"
              />
            </li>
          </ul>

          <div class="inline-create-row">
            <InputText
              v-model="newSubtaskTitle"
              placeholder="Add subtask"
              :disabled="busy"
              @keydown.enter.prevent="addSubtaskToTask"
            />
            <Button
              label="Add"
              icon="pi pi-plus"
              :disabled="busy || newSubtaskTitle.trim().length === 0"
              @click="addSubtaskToTask"
            />
          </div>
        </section>

        <section class="detail-section">
          <h3>Open Questions</h3>
          <div class="inline-create-column">
            <InputText v-model="newQuestion" placeholder="Question" :disabled="busy" />
            <Textarea v-model="newQuestionContext" auto-resize rows="3" placeholder="Context" :disabled="busy" />
            <div class="detail-actions">
              <Button
                label="Ask"
                icon="pi pi-question-circle"
                :disabled="busy || newQuestion.trim().length === 0"
                @click="askTaskQuestion"
              />
            </div>
          </div>

          <ul class="question-list">
            <li v-for="question in openQuestions" :key="question.id" class="question-item">
              <p class="question-copy">{{ question.question }}</p>
              <p v-if="question.context.length > 0" class="question-context-detail">{{ question.context }}</p>
              <div class="inline-create-row">
                <InputText
                  v-model="questionDrafts[question.id]"
                  placeholder="Answer"
                  :disabled="busy"
                  @keydown.enter.prevent="resolveQuestion(question.id)"
                />
                <Button
                  label="Resolve"
                  icon="pi pi-check"
                  :disabled="busy || (questionDrafts[question.id] ?? '').trim().length === 0"
                  @click="resolveQuestion(question.id)"
                />
              </div>
            </li>
          </ul>

          <details v-if="resolvedQuestions.length > 0" class="resolved-block">
            <summary>Resolved ({{ resolvedQuestions.length }})</summary>
            <ul class="question-list">
              <li v-for="question in resolvedQuestions" :key="question.id" class="question-item">
                <p class="question-copy">{{ question.question }}</p>
                <p class="question-answer">{{ question.answer }}</p>
              </li>
            </ul>
          </details>
        </section>

        <section class="detail-section">
          <h3>Attachments</h3>
          <div class="inline-create-row">
            <input type="file" class="file-input" :disabled="busy" @change="uploadTaskAttachment" />
          </div>
          <ul v-if="detail.attachments.length > 0" class="attachment-list">
            <li v-for="attachment in detail.attachments" :key="attachment.id" class="attachment-item">
              <a :href="`/api/v1/files/${attachment.id}`" target="_blank" rel="noopener noreferrer">
                {{ attachment.filename }}
              </a>
              <span>{{ attachment.size_bytes }} bytes</span>
              <Button
                icon="pi pi-trash"
                text
                severity="secondary"
                :disabled="busy"
                @click="removeAttachment(attachment.id)"
              />
            </li>
          </ul>
          <p v-else class="dim-copy">No attachments yet.</p>
        </section>

        <section class="detail-section">
          <h3>History</h3>
          <ul class="history-list">
            <li v-for="entry in detail.history" :key="entry.id" class="history-item">
              <p class="history-line">
                <span>{{ formatTimestamp(entry.created_at) }}</span>
                <span>{{ entry.actor }}</span>
                <strong>{{ entry.action }}</strong>
              </p>
              <code v-if="compactDetail(entry.detail).length > 0" class="history-detail">
                {{ compactDetail(entry.detail) }}
              </code>
            </li>
          </ul>
        </section>
      </template>
    </div>
  </Dialog>
</template>

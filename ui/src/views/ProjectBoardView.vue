<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useRoute } from 'vue-router';
import Card from 'primevue/card';
import Message from 'primevue/message';
import Tag from 'primevue/tag';

import {
  listOpenQuestions,
  listTasks,
  moveTask,
  projectEventsPath,
  type TaskEventPayload,
  type TaskResponse,
  type TaskStatus,
} from '../api/lattice';
import TaskDetailPanel from '../components/TaskDetailPanel.vue';

const route = useRoute();
const slug = computed(() => {
  const value = route.params.slug;
  return typeof value === 'string' && value.length > 0 ? value : 'PROJECT';
});

const columns: Array<{ key: TaskStatus; label: string }> = [
  { key: 'backlog', label: 'Backlog' },
  { key: 'ready', label: 'Ready' },
  { key: 'in_progress', label: 'In Progress' },
  { key: 'review', label: 'Review' },
  { key: 'done', label: 'Done' },
];

const tasks = ref<TaskResponse[]>([]);
const questionCounts = ref<Record<string, number>>({});
const loading = ref(false);
const error = ref<string | null>(null);
const moveError = ref<string | null>(null);
const sseWarning = ref<string | null>(null);

const draggedTaskId = ref<string | null>(null);
const hoveredColumn = ref<TaskStatus | null>(null);

const selectedTaskRef = ref<string | null>(null);
const detailVisible = ref(false);

let boardEventSource: EventSource | null = null;
let refreshTimer: number | null = null;

const sseEventTypes = [
  'task.created',
  'task.updated',
  'task.moved',
  'task.deleted',
  'task.review_state_changed',
  'question.created',
  'question.resolved',
];

watch(
  slug,
  (nextSlug) => {
    closeEventSource();
    clearRefreshTimer();
    void refreshBoard();
    openEventSource(nextSlug);
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  closeEventSource();
  clearRefreshTimer();
});

async function refreshBoard(): Promise<void> {
  loading.value = true;
  error.value = null;
  moveError.value = null;

  try {
    const [nextTasks, openQuestions] = await Promise.all([
      listTasks(slug.value, { limit: 100, offset: 0 }),
      listOpenQuestions(slug.value, 100, 0),
    ]);

    tasks.value = nextTasks;

    const counts: Record<string, number> = {};
    for (const question of openQuestions) {
      counts[question.task_id] = (counts[question.task_id] ?? 0) + 1;
    }
    questionCounts.value = counts;
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load board';
  } finally {
    loading.value = false;
  }
}

function tasksForColumn(status: TaskStatus): TaskResponse[] {
  return tasks.value
    .filter((task) => task.status === status)
    .slice()
    .sort((left, right) => left.sort_order - right.sort_order);
}

function onTaskDragStart(taskId: string, event: DragEvent): void {
  draggedTaskId.value = taskId;
  if (!event.dataTransfer) {
    return;
  }

  event.dataTransfer.effectAllowed = 'move';
  event.dataTransfer.setData('text/plain', taskId);
}

function onTaskDragEnd(): void {
  draggedTaskId.value = null;
  hoveredColumn.value = null;
}

function onColumnDragOver(status: TaskStatus, event: DragEvent): void {
  event.preventDefault();
  hoveredColumn.value = status;

  if (!event.dataTransfer) {
    return;
  }

  event.dataTransfer.dropEffect = 'move';
}

function onColumnDragLeave(status: TaskStatus): void {
  if (hoveredColumn.value === status) {
    hoveredColumn.value = null;
  }
}

async function onColumnDrop(status: TaskStatus, event: DragEvent): Promise<void> {
  event.preventDefault();
  const taskId = draggedTaskId.value ?? event.dataTransfer?.getData('text/plain');

  hoveredColumn.value = null;
  draggedTaskId.value = null;
  moveError.value = null;

  if (!taskId) {
    return;
  }

  const task = tasks.value.find((entry) => entry.id === taskId);
  if (!task) {
    return;
  }

  if (task.status === status) {
    return;
  }

  const previousStatus = task.status;
  task.status = status;

  try {
    const moved = await moveTask(slug.value, task.id, status);
    const index = tasks.value.findIndex((entry) => entry.id === moved.id);
    if (index >= 0) {
      tasks.value.splice(index, 1, moved);
    }
    scheduleRefresh();
  } catch (err) {
    task.status = previousStatus;
    moveError.value = err instanceof Error ? err.message : 'Failed to move task';
  }
}

function openTask(task: TaskResponse): void {
  selectedTaskRef.value = task.id;
  detailVisible.value = true;
}

function onTaskDetailChanged(): void {
  void refreshBoard();
}

function prioritySeverity(priority: TaskResponse['priority']): 'danger' | 'warn' | 'info' | 'secondary' {
  switch (priority) {
    case 'critical':
      return 'danger';
    case 'high':
      return 'warn';
    case 'medium':
      return 'info';
    default:
      return 'secondary';
  }
}

function openEventSource(projectSlug: string): void {
  const source = new EventSource(projectEventsPath(projectSlug));
  boardEventSource = source;
  sseWarning.value = null;

  const handleEvent = (raw: MessageEvent): void => {
    if (projectSlug !== slug.value) {
      return;
    }

    const payload = parseEventPayload(raw.data);
    if (!payload) {
      return;
    }

    applyEventPayload(payload);
    scheduleRefresh();
  };

  for (const eventType of sseEventTypes) {
    source.addEventListener(eventType, handleEvent as EventListener);
  }

  source.onopen = () => {
    if (boardEventSource === source) {
      sseWarning.value = null;
    }
  };

  source.onerror = () => {
    if (boardEventSource === source) {
      sseWarning.value = 'Live updates disconnected, retrying automatically.';
    }
  };
}

function closeEventSource(): void {
  if (!boardEventSource) {
    return;
  }

  boardEventSource.close();
  boardEventSource = null;
}

function parseEventPayload(raw: unknown): TaskEventPayload | null {
  if (typeof raw !== 'string' || raw.length === 0) {
    return null;
  }

  try {
    return JSON.parse(raw) as TaskEventPayload;
  } catch {
    return null;
  }
}

function applyEventPayload(payload: TaskEventPayload): void {
  if (payload.action === 'task.deleted' && payload.task_id) {
    tasks.value = tasks.value.filter((task) => task.id !== payload.task_id);
    const nextCounts = { ...questionCounts.value };
    delete nextCounts[payload.task_id];
    questionCounts.value = nextCounts;
    return;
  }

  if (payload.action === 'question.created' && payload.task_id) {
    questionCounts.value = {
      ...questionCounts.value,
      [payload.task_id]: (questionCounts.value[payload.task_id] ?? 0) + 1,
    };
    return;
  }

  if (payload.action === 'question.resolved' && payload.task_id) {
    questionCounts.value = {
      ...questionCounts.value,
      [payload.task_id]: Math.max(0, (questionCounts.value[payload.task_id] ?? 0) - 1),
    };
  }
}

function scheduleRefresh(): void {
  if (refreshTimer !== null) {
    return;
  }

  refreshTimer = window.setTimeout(() => {
    refreshTimer = null;
    void refreshBoard();
  }, 250);
}

function clearRefreshTimer(): void {
  if (refreshTimer === null) {
    return;
  }

  window.clearTimeout(refreshTimer);
  refreshTimer = null;
}
</script>

<template>
  <section class="board-view">
    <div class="section-header">
      <div>
        <h2>{{ slug }} Board</h2>
        <p>Drag cards between columns to move tickets, click a card for full detail.</p>
      </div>
    </div>

    <Message v-if="error" severity="error" :closable="false">
      {{ error }}
    </Message>
    <Message v-else-if="moveError" severity="warn" :closable="false">
      {{ moveError }}
    </Message>
    <Message v-if="sseWarning" severity="secondary" :closable="false">
      {{ sseWarning }}
    </Message>

    <div v-if="loading" class="board-loading">Loading board...</div>

    <div v-else class="board-grid">
      <article
        v-for="column in columns"
        :key="column.key"
        :class="['board-column', hoveredColumn === column.key ? 'drop-active' : '']"
        @dragover="onColumnDragOver(column.key, $event)"
        @dragleave="onColumnDragLeave(column.key)"
        @drop="onColumnDrop(column.key, $event)"
      >
        <h3>
          {{ column.label }} <span class="column-count">{{ tasksForColumn(column.key).length }}</span>
        </h3>
        <div class="column-stack">
          <Card
            v-for="task in tasksForColumn(column.key)"
            :key="task.id"
            :class="['task-card', draggedTaskId === task.id ? 'dragging' : '']"
            draggable="true"
            @click="openTask(task)"
            @dragstart="onTaskDragStart(task.id, $event)"
            @dragend="onTaskDragEnd"
          >
            <template #title>
              <div class="task-top-row">
                <span class="task-key">{{ task.display_key }}</span>
                <Tag :value="task.priority" :severity="prioritySeverity(task.priority)" />
              </div>
            </template>
            <template #content>
              <p class="task-title">{{ task.title }}</p>
              <div class="task-meta-row">
                <span>
                  <i class="pi pi-question-circle"></i>
                  {{ questionCounts[task.id] ?? 0 }}
                </span>
                <span v-if="task.review_state === 'not_ready'" class="blocked-pill">
                  <i class="pi pi-pause-circle"></i>
                  not_ready
                </span>
              </div>
            </template>
          </Card>
        </div>
      </article>
    </div>

    <TaskDetailPanel
      v-model:visible="detailVisible"
      :slug="slug"
      :task-ref="selectedTaskRef"
      @changed="onTaskDetailChanged"
    />
  </section>
</template>

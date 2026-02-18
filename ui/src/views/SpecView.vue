<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute } from 'vue-router';
import Button from 'primevue/button';
import Message from 'primevue/message';
import Textarea from 'primevue/textarea';

import {
  listSpecHistory,
  listSpecSections,
  type SpecRevisionRecord,
  type SpecSectionRecord,
  updateSpecSection,
} from '../api/lattice';

const route = useRoute();
const slug = computed(() => {
  const value = route.params.slug;
  return typeof value === 'string' && value.length > 0 ? value : 'PROJECT';
});

const sections = [
  { key: 'overview', label: 'Overview' },
  { key: 'requirements', label: 'Requirements' },
  { key: 'architecture', label: 'Architecture' },
  { key: 'technical_design', label: 'Technical Design' },
  { key: 'open_decisions', label: 'Open Decisions' },
  { key: 'references', label: 'References' },
] as const;

type SectionKey = (typeof sections)[number]['key'];

const selectedSection = ref<SectionKey>('overview');
const sectionContent = ref<Record<SectionKey, string>>({
  overview: '',
  requirements: '',
  architecture: '',
  technical_design: '',
  open_decisions: '',
  references: '',
});

const history = ref<SpecRevisionRecord[]>([]);
const loading = ref(false);
const saving = ref(false);
const error = ref<string | null>(null);
const selectedBaseRevisionId = ref('');

watch(
  slug,
  () => {
    void loadSpec();
  },
  { immediate: true },
);

watch(selectedSection, () => {
  void loadSectionHistory();
});

const activeLabel = computed(() => {
  const found = sections.find((section) => section.key === selectedSection.value);
  return found ? found.label : 'Section';
});

const baselineRevision = computed(() => {
  if (selectedBaseRevisionId.value.length === 0) {
    return null;
  }

  return history.value.find((revision) => revision.id === selectedBaseRevisionId.value) ?? null;
});

const diffLines = computed(() => {
  if (!baselineRevision.value) {
    return [];
  }

  return buildLineDiff(baselineRevision.value.content, sectionContent.value[selectedSection.value]);
});

async function loadSpec(): Promise<void> {
  loading.value = true;
  error.value = null;

  try {
    const records = await listSpecSections(slug.value);
    sectionContent.value = toContentMap(records);
    await loadSectionHistory();
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load project spec';
  } finally {
    loading.value = false;
  }
}

async function loadSectionHistory(): Promise<void> {
  try {
    const nextHistory = await listSpecHistory(slug.value, selectedSection.value, 20, 0);
    history.value = nextHistory;

    const knownIds = new Set(nextHistory.map((revision) => revision.id));
    if (!knownIds.has(selectedBaseRevisionId.value)) {
      selectedBaseRevisionId.value = nextHistory[1]?.id ?? nextHistory[0]?.id ?? '';
    }
  } catch (err) {
    history.value = [];
    selectedBaseRevisionId.value = '';
    error.value = err instanceof Error ? err.message : 'Failed to load section history';
  }
}

async function saveSection(): Promise<void> {
  saving.value = true;
  error.value = null;

  try {
    await updateSpecSection(slug.value, selectedSection.value, sectionContent.value[selectedSection.value]);
    await loadSectionHistory();
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to save section';
  } finally {
    saving.value = false;
  }
}

function toContentMap(records: SpecSectionRecord[]): Record<SectionKey, string> {
  const next: Record<SectionKey, string> = {
    overview: '',
    requirements: '',
    architecture: '',
    technical_design: '',
    open_decisions: '',
    references: '',
  };

  for (const record of records) {
    next[record.section] = record.content;
  }

  return next;
}

function formatTimestamp(value: string): string {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }

  return parsed.toLocaleString();
}

interface DiffLine {
  type: 'context' | 'added' | 'removed';
  text: string;
  oldNumber: number | null;
  newNumber: number | null;
}

function buildLineDiff(previous: string, current: string): DiffLine[] {
  const oldLines = splitLines(previous);
  const newLines = splitLines(current);
  const diff: DiffLine[] = [];

  let oldIndex = 0;
  let newIndex = 0;
  let oldLineNumber = 1;
  let newLineNumber = 1;

  while (oldIndex < oldLines.length || newIndex < newLines.length) {
    const oldLine = oldLines[oldIndex];
    const newLine = newLines[newIndex];

    if (oldLine === newLine && oldLine !== undefined) {
      diff.push({
        type: 'context',
        text: oldLine,
        oldNumber: oldLineNumber,
        newNumber: newLineNumber,
      });
      oldIndex += 1;
      newIndex += 1;
      oldLineNumber += 1;
      newLineNumber += 1;
      continue;
    }

    if (oldLine !== undefined && newIndex + 1 < newLines.length && oldLine === newLines[newIndex + 1]) {
      diff.push({
        type: 'added',
        text: newLines[newIndex] ?? '',
        oldNumber: null,
        newNumber: newLineNumber,
      });
      newIndex += 1;
      newLineNumber += 1;
      continue;
    }

    if (newLine !== undefined && oldIndex + 1 < oldLines.length && oldLines[oldIndex + 1] === newLine) {
      diff.push({
        type: 'removed',
        text: oldLines[oldIndex] ?? '',
        oldNumber: oldLineNumber,
        newNumber: null,
      });
      oldIndex += 1;
      oldLineNumber += 1;
      continue;
    }

    if (oldLine !== undefined) {
      diff.push({
        type: 'removed',
        text: oldLine,
        oldNumber: oldLineNumber,
        newNumber: null,
      });
      oldIndex += 1;
      oldLineNumber += 1;
    }

    if (newLine !== undefined) {
      diff.push({
        type: 'added',
        text: newLine,
        oldNumber: null,
        newNumber: newLineNumber,
      });
      newIndex += 1;
      newLineNumber += 1;
    }
  }

  return diff;
}

function splitLines(content: string): string[] {
  if (content.length === 0) {
    return [];
  }

  return content.replace(/\r\n/g, '\n').split('\n');
}

function diffMarker(type: DiffLine['type']): string {
  switch (type) {
    case 'added':
      return '+';
    case 'removed':
      return '-';
    default:
      return ' ';
  }
}
</script>

<template>
  <section class="spec-view">
    <div class="section-header">
      <div>
        <h2>{{ slug }} Spec</h2>
        <p>Fixed sections, independently editable and revisioned.</p>
      </div>
      <Button label="Save Section" icon="pi pi-save" :loading="saving" @click="saveSection" />
    </div>

    <Message v-if="error" severity="error" :closable="false">
      {{ error }}
    </Message>

    <div v-if="loading" class="board-loading">Loading spec...</div>

    <div v-else class="spec-layout">
      <aside class="spec-nav">
        <button
          v-for="section in sections"
          :key="section.key"
          :class="['spec-tab', selectedSection === section.key ? 'active' : '']"
          @click="selectedSection = section.key"
        >
          {{ section.label }}
        </button>
      </aside>

      <div class="spec-editor">
        <h3>{{ activeLabel }}</h3>
        <Textarea v-model="sectionContent[selectedSection]" class="spec-textarea" auto-resize rows="20" />

        <div class="spec-history">
          <h3>Recent Revisions</h3>
          <ul class="history-list">
            <li v-for="revision in history" :key="revision.id" class="history-item">
              <p class="history-line">
                <span>{{ formatTimestamp(revision.created_at) }}</span>
                <span>{{ revision.edited_by }}</span>
              </p>
            </li>
          </ul>
        </div>

        <div class="spec-diff">
          <div class="detail-header-row">
            <h3>Revision Diff</h3>
            <select v-model="selectedBaseRevisionId" class="task-select">
              <option value="">Choose baseline revision</option>
              <option v-for="revision in history" :key="revision.id" :value="revision.id">
                {{ formatTimestamp(revision.created_at) }} · {{ revision.edited_by }}
              </option>
            </select>
          </div>

          <p v-if="!baselineRevision" class="dim-copy">Select a revision to compare against current editor content.</p>

          <ul v-else class="diff-list">
            <li v-for="(line, index) in diffLines" :key="`${line.type}-${index}`" :class="['diff-line', line.type]">
              <span class="diff-num">{{ line.oldNumber ?? '' }}</span>
              <span class="diff-num">{{ line.newNumber ?? '' }}</span>
              <span class="diff-marker">{{ diffMarker(line.type) }}</span>
              <code class="diff-text">{{ line.text.length > 0 ? line.text : ' ' }}</code>
            </li>
          </ul>
        </div>
      </div>
    </div>
  </section>
</template>

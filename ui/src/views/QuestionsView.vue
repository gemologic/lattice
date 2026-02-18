<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import { useRoute } from 'vue-router';
import Button from 'primevue/button';
import Card from 'primevue/card';
import InputText from 'primevue/inputtext';
import Message from 'primevue/message';

import { answerQuestion, listOpenQuestions, type ProjectOpenQuestionResponse } from '../api/lattice';

const route = useRoute();
const slug = computed(() => {
  const value = route.params.slug;
  return typeof value === 'string' && value.length > 0 ? value : 'PROJECT';
});

const openQuestions = ref<ProjectOpenQuestionResponse[]>([]);
const drafts = reactive<Record<string, string>>({});
const loading = ref(false);
const error = ref<string | null>(null);
const busyId = ref<string | null>(null);

watch(
  slug,
  () => {
    void loadQuestions();
  },
  { immediate: true },
);

async function loadQuestions(): Promise<void> {
  loading.value = true;
  error.value = null;

  try {
    openQuestions.value = await listOpenQuestions(slug.value, 100, 0);
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load open questions';
  } finally {
    loading.value = false;
  }
}

async function resolveQuestion(item: ProjectOpenQuestionResponse): Promise<void> {
  const answer = (drafts[item.id] ?? '').trim();
  if (answer.length === 0) {
    return;
  }

  busyId.value = item.id;
  error.value = null;

  try {
    await answerQuestion(slug.value, item.task_display_key, item.id, answer);
    drafts[item.id] = '';
    await loadQuestions();
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to resolve question';
  } finally {
    busyId.value = null;
  }
}
</script>

<template>
  <section class="questions-view">
    <div class="section-header">
      <div>
        <h2>{{ slug }} Open Questions</h2>
        <p>Answer blockers quickly so agent work can continue.</p>
      </div>
      <Button label="Refresh" icon="pi pi-refresh" @click="loadQuestions" />
    </div>

    <Message v-if="error" severity="error" :closable="false">
      {{ error }}
    </Message>

    <div v-if="loading" class="board-loading">Loading questions...</div>

    <div v-else-if="openQuestions.length === 0" class="empty-state">No open questions right now.</div>

    <div v-else class="question-stack">
      <Card v-for="item in openQuestions" :key="item.id" class="question-card">
        <template #title>
          <div class="task-top-row">
            <span class="task-key">{{ item.task_display_key }}</span>
            <span class="question-pill">open</span>
          </div>
        </template>
        <template #content>
          <h3 class="question-title">{{ item.question }}</h3>
          <p class="question-context">{{ item.context }}</p>

          <div class="answer-row">
            <InputText
              v-model="drafts[item.id]"
              class="answer-input"
              placeholder="Write answer"
              :disabled="busyId === item.id"
              @keydown.enter.prevent="resolveQuestion(item)"
            />
            <Button
              label="Resolve"
              icon="pi pi-check"
              :disabled="busyId === item.id || (drafts[item.id] ?? '').trim().length === 0"
              @click="resolveQuestion(item)"
            />
          </div>
        </template>
      </Card>
    </div>
  </section>
</template>

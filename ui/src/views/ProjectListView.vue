<script setup lang="ts">
import { onMounted, ref } from 'vue';
import Button from 'primevue/button';
import Card from 'primevue/card';
import Message from 'primevue/message';
import Tag from 'primevue/tag';

import { listProjects, type ProjectSummary } from '../api/lattice';

const loading = ref(false);
const error = ref<string | null>(null);
const projects = ref<ProjectSummary[]>([]);

onMounted(() => {
  void loadProjects();
});

async function loadProjects(): Promise<void> {
  loading.value = true;
  error.value = null;

  try {
    projects.value = await listProjects(100, 0);
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load projects';
  } finally {
    loading.value = false;
  }
}
</script>

<template>
  <section class="projects-view">
    <div class="section-header">
      <div>
        <h2>Projects</h2>
        <p>Pick a board, align on goal, then execute.</p>
      </div>
      <Button label="Refresh" icon="pi pi-refresh" @click="loadProjects" />
    </div>

    <Message v-if="error" severity="error" :closable="false">
      {{ error }}
    </Message>

    <div v-if="loading" class="board-loading">Loading projects...</div>

    <div v-else-if="projects.length === 0" class="empty-state">No projects yet.</div>

    <div v-else class="project-grid">
      <Card v-for="item in projects" :key="item.project.id" class="project-card">
        <template #title>
          <div class="card-title-row">
            <RouterLink class="project-title-link" :to="`/${item.project.slug}`">
              {{ item.project.name }}
            </RouterLink>
            <Tag :value="item.project.slug" severity="contrast" />
          </div>
        </template>
        <template #content>
          <p class="goal-copy">{{ item.project.goal }}</p>
          <div class="metric-row">
            <span><i class="pi pi-briefcase"></i> {{ item.in_progress_count }} in progress</span>
            <span><i class="pi pi-question-circle"></i> {{ item.open_question_count }} open</span>
            <span><i class="pi pi-lock"></i> {{ item.not_ready_count }} not ready</span>
          </div>
        </template>
      </Card>
    </div>
  </section>
</template>

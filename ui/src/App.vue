<script setup lang="ts">
import { computed, reactive, ref } from 'vue';
import { RouterLink, RouterView, useRoute, useRouter } from 'vue-router';
import Button from 'primevue/button';
import Dialog from 'primevue/dialog';
import InputText from 'primevue/inputtext';
import Message from 'primevue/message';
import Textarea from 'primevue/textarea';

import { createProject } from './api/lattice';

const route = useRoute();
const router = useRouter();

const activeSlug = computed(() => {
  const value = route.params.slug;
  return typeof value === 'string' && value.length > 0 ? value : null;
});

const boardPath = computed(() => (activeSlug.value ? `/${activeSlug.value}` : '/'));
const specPath = computed(() => (activeSlug.value ? `/${activeSlug.value}/spec` : '/'));
const questionsPath = computed(() => (activeSlug.value ? `/${activeSlug.value}/questions` : '/'));
const webhooksPath = computed(() => (activeSlug.value ? `/${activeSlug.value}/settings/webhooks` : '/'));
const createDialogVisible = ref(false);
const createError = ref<string | null>(null);
const creating = ref(false);
const projectSlugPattern = /^[A-Z0-9]+(?:-[A-Z0-9]+)*$/;
const createForm = reactive({
  name: '',
  slug: '',
  goal: '',
});
const canCreateProject = computed(
  () =>
    createForm.name.trim().length > 0 &&
    projectSlugPattern.test(createForm.slug.trim().toUpperCase()) &&
    !creating.value,
);

function openCreateDialog(): void {
  createError.value = null;
  createDialogVisible.value = true;
}

function closeCreateDialog(): void {
  if (creating.value) {
    return;
  }

  createDialogVisible.value = false;
}

async function submitCreateProject(): Promise<void> {
  const name = createForm.name.trim();
  const slug = createForm.slug.trim().toUpperCase();
  const goal = createForm.goal.trim();
  if (name.length === 0 || !projectSlugPattern.test(slug) || creating.value) {
    return;
  }

  createError.value = null;
  creating.value = true;

  try {
    const created = await createProject({ name, slug, goal });
    createDialogVisible.value = false;
    createForm.name = '';
    createForm.slug = '';
    createForm.goal = '';
    await router.push({ name: 'board', params: { slug: created.project.slug } });
  } catch (err) {
    createError.value = err instanceof Error ? err.message : 'Failed to create project';
  } finally {
    creating.value = false;
  }
}
</script>

<template>
  <div class="app-shell">
    <header class="topbar">
      <div class="brand">
        <img class="brand-logo" src="/lattice-logo.svg" alt="Lattice logo" />
        <div class="brand-copy">
          <p class="eyebrow">Gemologic</p>
          <h1>Lattice</h1>
          <p class="subtitle">Project-scoped board for human and agent execution.</p>
        </div>
      </div>
      <div class="topbar-actions">
        <Button label="New Project" icon="pi pi-plus" size="small" @click="openCreateDialog" />
      </div>
    </header>

    <nav v-if="activeSlug" class="project-nav">
      <RouterLink :class="['project-link', route.path === boardPath ? 'active' : '']" :to="boardPath">
        Board
      </RouterLink>
      <RouterLink :class="['project-link', route.path === specPath ? 'active' : '']" :to="specPath"> Spec </RouterLink>
      <RouterLink :class="['project-link', route.path === questionsPath ? 'active' : '']" :to="questionsPath">
        Questions
      </RouterLink>
      <RouterLink :class="['project-link', route.path === webhooksPath ? 'active' : '']" :to="webhooksPath">
        Webhooks
      </RouterLink>
    </nav>

    <main class="content">
      <RouterView />
    </main>

    <Dialog
      v-model:visible="createDialogVisible"
      modal
      header="Create Project"
      :style="{ width: 'min(34rem, 92vw)' }"
      :dismissable-mask="!creating"
      :closable="!creating"
      :close-on-escape="!creating"
    >
      <div class="create-project-form">
        <Message v-if="createError" severity="error" :closable="false">
          {{ createError }}
        </Message>

        <label class="create-project-field">
          <span class="field-label">Name</span>
          <InputText
            v-model="createForm.name"
            placeholder="Lattice Demo"
            :disabled="creating"
            autocomplete="off"
            @keydown.enter.prevent="submitCreateProject"
          />
        </label>

        <label class="create-project-field">
          <span class="field-label">Slug</span>
          <InputText
            v-model="createForm.slug"
            placeholder="LATTICE-DEMO"
            :disabled="creating"
            autocomplete="off"
            @update:model-value="createForm.slug = String($event).toUpperCase()"
            @keydown.enter.prevent="submitCreateProject"
          />
          <small class="field-help">Use uppercase letters, digits, and dashes only.</small>
        </label>

        <label class="create-project-field">
          <span class="field-label">Goal</span>
          <Textarea
            v-model="createForm.goal"
            class="create-project-goal"
            auto-resize
            rows="4"
            placeholder="Ship first workflow"
            :disabled="creating"
          />
        </label>

        <div class="create-project-actions">
          <Button label="Cancel" severity="secondary" text :disabled="creating" @click="closeCreateDialog" />
          <Button
            label="Create Project"
            icon="pi pi-check"
            :loading="creating"
            :disabled="!canCreateProject"
            @click="submitCreateProject"
          />
        </div>
      </div>
    </Dialog>
  </div>
</template>

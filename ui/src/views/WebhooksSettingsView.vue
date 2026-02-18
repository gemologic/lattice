<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import { useRoute } from 'vue-router';
import Button from 'primevue/button';
import Card from 'primevue/card';
import InputText from 'primevue/inputtext';
import Message from 'primevue/message';
import Tag from 'primevue/tag';

import {
  createWebhook,
  deleteWebhook,
  listWebhooks,
  testWebhook,
  updateWebhook,
  type WebhookPlatform,
  type WebhookResponse,
} from '../api/lattice';

const route = useRoute();
const slug = computed(() => {
  const value = route.params.slug;
  return typeof value === 'string' && value.length > 0 ? value : 'PROJECT';
});

const loading = ref(false);
const busyWebhookId = ref<string | null>(null);
const error = ref<string | null>(null);
const success = ref<string | null>(null);
const webhooks = ref<WebhookResponse[]>([]);

const createForm = reactive<{
  name: string;
  url: string;
  platform: WebhookPlatform;
  secret: string;
  events: string[];
}>({
  name: '',
  url: '',
  platform: 'generic',
  secret: '',
  events: ['task.created', 'task.moved', 'question.created', 'question.resolved'],
});

const eventOptions = [
  'task.created',
  'task.updated',
  'task.moved',
  'task.deleted',
  'task.review_state_changed',
  'question.created',
  'question.resolved',
  'spec.updated',
  'goal.updated',
];

watch(
  slug,
  () => {
    void loadWebhooks();
  },
  { immediate: true },
);

async function loadWebhooks(): Promise<void> {
  loading.value = true;
  error.value = null;

  try {
    webhooks.value = await listWebhooks(slug.value);
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load webhooks';
  } finally {
    loading.value = false;
  }
}

async function createWebhookRecord(): Promise<void> {
  const name = createForm.name.trim();
  const url = createForm.url.trim();
  if (name.length === 0 || url.length === 0 || createForm.events.length === 0) {
    return;
  }

  busyWebhookId.value = '__create__';
  error.value = null;
  success.value = null;
  try {
    await createWebhook(slug.value, {
      name,
      url,
      platform: createForm.platform,
      events: createForm.events,
      secret: createForm.secret.trim().length > 0 ? createForm.secret.trim() : undefined,
      active: true,
    });
    createForm.name = '';
    createForm.url = '';
    createForm.secret = '';
    success.value = 'Webhook created.';
    await loadWebhooks();
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to create webhook';
  } finally {
    busyWebhookId.value = null;
  }
}

async function toggleActive(webhook: WebhookResponse): Promise<void> {
  busyWebhookId.value = webhook.id;
  error.value = null;
  success.value = null;
  try {
    const updated = await updateWebhook(slug.value, webhook.id, {
      active: !webhook.active,
    });
    const index = webhooks.value.findIndex((entry) => entry.id === updated.id);
    if (index >= 0) {
      webhooks.value.splice(index, 1, updated);
    }
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to update webhook';
  } finally {
    busyWebhookId.value = null;
  }
}

async function sendTest(webhookId: string): Promise<void> {
  busyWebhookId.value = webhookId;
  error.value = null;
  success.value = null;
  try {
    await testWebhook(slug.value, webhookId);
    success.value = 'Test payload sent.';
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to send test payload';
  } finally {
    busyWebhookId.value = null;
  }
}

async function removeWebhook(webhookId: string): Promise<void> {
  busyWebhookId.value = webhookId;
  error.value = null;
  success.value = null;
  try {
    await deleteWebhook(slug.value, webhookId);
    webhooks.value = webhooks.value.filter((entry) => entry.id !== webhookId);
    success.value = 'Webhook deleted.';
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to delete webhook';
  } finally {
    busyWebhookId.value = null;
  }
}

function toggleEvent(eventName: string): void {
  if (createForm.events.includes(eventName)) {
    createForm.events = createForm.events.filter((value) => value !== eventName);
    return;
  }

  createForm.events = [...createForm.events, eventName];
}
</script>

<template>
  <section class="webhooks-view">
    <div class="section-header">
      <div>
        <h2>{{ slug }} Webhooks</h2>
        <p>Deliver board events to Slack, Discord, or generic HTTP targets.</p>
      </div>
      <Button label="Refresh" icon="pi pi-refresh" :disabled="loading" @click="loadWebhooks" />
    </div>

    <Message v-if="error" severity="error" :closable="false">
      {{ error }}
    </Message>
    <Message v-if="success" severity="success" :closable="false">
      {{ success }}
    </Message>

    <Card class="project-card">
      <template #title>Create Webhook</template>
      <template #content>
        <div class="webhook-form-grid">
          <label class="webhook-field">
            <span class="field-label">Name</span>
            <InputText v-model="createForm.name" placeholder="alerts-main" />
          </label>
          <label class="webhook-field">
            <span class="field-label">URL</span>
            <InputText v-model="createForm.url" placeholder="https://example.com/webhook" />
          </label>
          <label class="webhook-field">
            <span class="field-label">Platform</span>
            <select v-model="createForm.platform" class="task-select">
              <option value="generic">Generic</option>
              <option value="slack">Slack</option>
              <option value="discord">Discord</option>
            </select>
          </label>
          <label class="webhook-field">
            <span class="field-label">Secret (optional)</span>
            <InputText v-model="createForm.secret" placeholder="hmac secret for generic targets" />
          </label>
        </div>

        <div class="webhook-events-grid">
          <label v-for="eventName in eventOptions" :key="eventName" class="webhook-event-toggle">
            <input type="checkbox" :checked="createForm.events.includes(eventName)" @change="toggleEvent(eventName)" />
            <span>{{ eventName }}</span>
          </label>
        </div>

        <div class="detail-actions">
          <Button
            label="Create"
            icon="pi pi-plus"
            :loading="busyWebhookId === '__create__'"
            :disabled="
              createForm.name.trim().length === 0 ||
              createForm.url.trim().length === 0 ||
              createForm.events.length === 0
            "
            @click="createWebhookRecord"
          />
        </div>
      </template>
    </Card>

    <div v-if="loading" class="board-loading">Loading webhooks...</div>
    <div v-else-if="webhooks.length === 0" class="empty-state">No webhooks configured.</div>

    <div v-else class="project-grid">
      <Card v-for="webhook in webhooks" :key="webhook.id" class="project-card">
        <template #title>
          <div class="card-title-row">
            <span>{{ webhook.name }}</span>
            <Tag :value="webhook.platform" severity="contrast" />
          </div>
        </template>
        <template #content>
          <p class="goal-copy webhook-url">{{ webhook.url }}</p>
          <p class="goal-copy">Events: {{ webhook.events.join(', ') }}</p>
          <p class="goal-copy">Secret: {{ webhook.has_secret ? 'configured' : 'none' }}</p>
          <div class="metric-row">
            <span><i class="pi pi-clock"></i> {{ webhook.updated_at }}</span>
          </div>
          <div class="webhook-actions">
            <Button
              :label="webhook.active ? 'Disable' : 'Enable'"
              icon="pi pi-power-off"
              size="small"
              severity="secondary"
              :loading="busyWebhookId === webhook.id"
              @click="toggleActive(webhook)"
            />
            <Button
              label="Test"
              icon="pi pi-send"
              size="small"
              :loading="busyWebhookId === webhook.id"
              @click="sendTest(webhook.id)"
            />
            <Button
              label="Delete"
              icon="pi pi-trash"
              size="small"
              severity="danger"
              :loading="busyWebhookId === webhook.id"
              @click="removeWebhook(webhook.id)"
            />
          </div>
        </template>
      </Card>
    </div>
  </section>
</template>

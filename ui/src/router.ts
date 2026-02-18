import { createRouter, createWebHistory } from 'vue-router';

import ProjectBoardView from './views/ProjectBoardView.vue';
import ProjectListView from './views/ProjectListView.vue';
import QuestionsView from './views/QuestionsView.vue';
import SpecView from './views/SpecView.vue';
import WebhooksSettingsView from './views/WebhooksSettingsView.vue';

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'projects',
      component: ProjectListView,
    },
    {
      path: '/:slug',
      name: 'board',
      component: ProjectBoardView,
    },
    {
      path: '/:slug/spec',
      name: 'spec',
      component: SpecView,
    },
    {
      path: '/:slug/questions',
      name: 'questions',
      component: QuestionsView,
    },
    {
      path: '/:slug/settings/webhooks',
      name: 'webhooks',
      component: WebhooksSettingsView,
    },
  ],
});

export default router;

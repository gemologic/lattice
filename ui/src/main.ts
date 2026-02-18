import { createApp } from 'vue';
import PrimeVue from 'primevue/config';
import Aura from '@primeuix/themes/aura';

import App from './App.vue';
import router from './router';
import './style.css';
import 'primeicons/primeicons.css';

document.documentElement.classList.add('app-dark');
document.body.classList.add('app-dark');

const app = createApp(App);

app.use(router);
app.use(PrimeVue, {
  theme: {
    preset: Aura,
    options: {
      darkModeSelector: '.app-dark',
    },
  },
});

app.mount('#app');

import '@fontsource-variable/inter';
import '@fontsource-variable/jetbrains-mono';
import './styles/tokens.css';
import './styles/base.css';
import './styles/components.css';
import './styles/dnd.css';
import './styles/editor.css';
import { mount } from 'svelte';
import App from './App.svelte';
import { bootstrap } from '$lib/app/bootstrap';

void bootstrap().then(() => {
  mount(App, { target: document.getElementById('app')! });
  requestAnimationFrame(() => void import('$lib/app/bootstrap').then((m) => m.afterMount()));
});

import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { createBackend } from './lib/api';
import { RoomStore } from './lib/room.svelte';

const store = new RoomStore(createBackend());

const app = mount(App, {
  target: document.getElementById('app')!,
  props: { store },
});

export default app;

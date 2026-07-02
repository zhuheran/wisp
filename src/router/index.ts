import { createRouter, createWebHistory } from 'vue-router'
import ChatView from '../views/ChatView.vue'
import PalsView from '../views/PalsView.vue'
import ProvidersView from '../views/ProvidersView.vue'
import McpView from '../views/McpView.vue'
import SettingsView from '../views/SettingsView.vue'

const router = createRouter({
	history: createWebHistory(import.meta.env.BASE_URL),
	routes: [
		{
			path: "/",
			redirect: { path: "/chat" },
		},
		{
			path: '/chat',
			name: 'chat',
			component: ChatView
		},
		{
			path: '/pals',
			name: 'pals',
			component: PalsView
		},
		{
			path: '/providers',
			name: 'providers',
			component: ProvidersView
		},
		{
			path: '/mcp',
			name: 'mcp',
			component: McpView
		},
		{
			path: '/settings',
			name: 'settings',
			component: SettingsView
		}
	]
})

export default router

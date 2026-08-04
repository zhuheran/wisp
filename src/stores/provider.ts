import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { Model, Provider } from '../libs/types'
import {
	configsGetProviders,
	configsUpdateProvider,
	configsDeleteProvider,
	configsAddModel,
	configsDeleteModel,
	configsCreateProvider,
	configsUpdateModel
} from '../libs/commands'

export const useProviderStore = defineStore('provider', () => {
	const providers = ref<Provider[]>([])
	const currentProvider = computed(() => providers.value.find(p => p.name === currentProviderName.value))
	const currentProviderName = ref<string | null>(null)
	const isLoading = ref(false)

	const selectProvider = (name: string) => {
		const provider = providers.value.find(p => p.name === name)
		if (provider) {
			currentProviderName.value = provider.name
		}
	}

	const loadProviders = async () => {
		isLoading.value = true
		try {
			const p = await configsGetProviders()
			providers.value = p
			if (currentProviderName.value && !p.some(provider => provider.name === currentProviderName.value)) {
				currentProviderName.value = null
			}
			return p
		}
		finally {
			isLoading.value = false
		}
	}

	const createProvider = async (provider: Provider) => {
		isLoading.value = true
		try {
			await configsCreateProvider(provider)

		}
		finally {
			await loadProviders()
			isLoading.value = false
		}
	}

	const updateProvider = async (name: string, data: Partial<Provider>) => {
		isLoading.value = true
		try {
			const provider = providers.value.find(p => p.name === name)
			if (!provider) {
				throw new Error('Provider not found')
			}
			const updatedProvider = {
				...provider,
				...data
			}
			await configsUpdateProvider(name, updatedProvider)
		}
		finally {
			await loadProviders()
			isLoading.value = false
		}
	}

	const deleteProvider = async (name: string) => {
		isLoading.value = true
		try {
			await configsDeleteProvider(name)

		}
		finally {
			await loadProviders()
			if (currentProvider.value?.name === name) {
				currentProviderName.value = null
			}
			isLoading.value = false
		}
	}

	const addModel = async (providerName: string, model: Model) => {
		isLoading.value = true
		try {
			await configsAddModel(providerName, model)
		}
		finally {
			await loadProviders()
			isLoading.value = false
		}
	}

	const deleteModel = async (providerName: string, modelName: string) => {
		isLoading.value = true
		try {
			await configsDeleteModel(providerName, modelName)

		}
		finally {
			await loadProviders()
			isLoading.value = false
		}
	}

	const updateModel = async (providerName: string, modelName: string, model: Model) => {
		isLoading.value = true
		try {
			await configsUpdateModel(providerName, modelName, model)
		}
		finally {
			await loadProviders()
			isLoading.value = false
		}
	}

	return {
		selectProvider,
		providers,
		currentProvider,
		isLoading,
		loadProviders,
		createProvider,
		updateProvider,
		deleteProvider,
		updateModel,
		addModel,
		deleteModel
	}
})

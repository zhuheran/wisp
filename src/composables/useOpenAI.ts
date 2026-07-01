import { getUrl } from '../libs/commands'
import type { Model } from '../libs/types'

export function useOpenAI() {
	const fetchModels = async (baseUrl: string, apiKey: string): Promise<Model[]> => {
		try {
			const response = await getUrl({
				url: `${baseUrl}/models`,
				headers: {
					'Authorization': `Bearer ${apiKey}`,
					'Content-Type': 'application/json'
				},
				parseJson: true
			})

			if (!response.data || !Array.isArray(response.data)) {
				throw new Error('Invalid models response format')
			}

			return response.data.map((model: any) => ({
				metadata: {
					name: model.id,
					display_name: model.id,
					description: model.description || ''
				},
				model_info: {
					type: 'text_generation',
					configs: {
						parameters: {},
						capabilities: [],
						multimodal: {
							text: {
								context_window: model.context_window || 2048,
								languages: ['en']
							}
						}
					}
				},
				max_input_size: model.context_window || 2048
			}))
		} catch (error) {
			console.error('[useOpenAI] Error fetching models:', error)
			throw new Error(`Failed to fetch models: ${error}`)
		}
	}

	return {
		fetchModels
	}
}

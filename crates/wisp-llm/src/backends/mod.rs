pub mod openai;
pub mod deepseek;
pub mod compat;

#[cfg(test)]
mod tests {
    use super::super::*;
    use wisp_configs::provider::{ApiType, Provider};

    fn provider_with(api_type: ApiType) -> Provider {
        Provider {
            name: "test".to_string(),
            display_name: "Test".to_string(),
            base_url: "http://localhost".to_string(),
            models: vec![],
            api_type,
        }
    }

    #[test]
    fn factory_returns_compat_by_default() {
        let p = provider_with(ApiType::OpenAiCompatible);
        let _backend = backend_for(&p);
    }
}

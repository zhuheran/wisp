use crate::types::ConversationToolCall;
use serde_json::Value;

pub fn merge_tool_call_deltas(deltas: &[Value]) -> Vec<ConversationToolCall> {
    let mut merged: std::collections::BTreeMap<u64, serde_json::Map<String, Value>> =
        Default::default();

    for delta in deltas {
        let Some(index) = delta.get("index").and_then(|i| i.as_u64()) else {
            continue;
        };
        let entry = merged.entry(index).or_default();

        if let Some(id) = delta.get("id") {
            entry.insert("id".to_string(), id.clone());
        }
        if let Some(func) = delta.get("function") {
            if let Some(name) = func.get("name") {
                entry
                    .entry("function")
                    .or_insert(Value::Object(Default::default()))
                    .as_object_mut()
                    .unwrap()
                    .insert("name".to_string(), name.clone());
            }
            if let Some(args) = func.get("arguments") {
                let args_str = args.as_str().unwrap_or("");
                let func_entry = entry
                    .entry("function")
                    .or_insert(Value::Object(Default::default()));
                let func_map = func_entry.as_object_mut().unwrap();
                let current = func_map
                    .get("arguments")
                    .and_then(|a| a.as_str())
                    .unwrap_or("");
                func_map.insert(
                    "arguments".to_string(),
                    Value::String(format!("{current}{args_str}")),
                );
            }
        }
    }

    merged
        .into_values()
        .filter_map(|mut v| {
            let func = v.get("function")?.as_object()?;
            let name = func.get("name")?.as_str()?.to_string();
            let args_str = func
                .get("arguments")
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            let arguments: Value =
                serde_json::from_str(args_str).unwrap_or(Value::Object(Default::default()));
            let id = v
                .get("id")
                .and_then(|i| i.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            Some(ConversationToolCall {
                id,
                name,
                arguments,
                result: None,
                qualified_name: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merges_simple_delta_sequence() {
        let deltas = vec![
            json!({"index":0,"id":"call_1","type":"function","function":{"name":"search","arguments":""}}),
            json!({"index":0,"function":{"arguments":"{\"q\":"}}),
            json!({"index":0,"function":{"arguments":"\"weather\"}"}}),
        ];
        let calls = merge_tool_call_deltas(&deltas);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search");
        assert_eq!(calls[0].arguments["q"], "weather");
    }

    #[test]
    fn merges_parallel_tool_calls() {
        let deltas = vec![
            json!({"index":0,"id":"a","function":{"name":"tool_a","arguments":""}}),
            json!({"index":1,"id":"b","function":{"name":"tool_b","arguments":""}}),
            json!({"index":0,"function":{"arguments":"{}"}}),
            json!({"index":1,"function":{"arguments":"{}"}}),
        ];
        let calls = merge_tool_call_deltas(&deltas);
        assert_eq!(calls.len(), 2);
    }

    #[test]
    fn empty_deltas_produce_no_calls() {
        let calls = merge_tool_call_deltas(&[]);
        assert!(calls.is_empty());
    }
}

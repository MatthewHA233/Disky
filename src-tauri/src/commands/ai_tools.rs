//! 让 AI 调我们内置的工具：按 AI 设置走 Anthropic 或 OpenAI 兼容接口的工具调用，
//! 一轮一轮把工具结果喂回去，直到 AI 不再调工具、或者到了轮数上限。

use serde_json::{json, Value};

use super::ai::{apply_auth_headers, build_api_url, build_http_client, AiSettings};

pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema（object）
    pub schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// AI 的一轮回复：说的话、要调的工具、原样回传给接口的 assistant 消息
#[derive(Debug)]
struct Turn {
    text: String,
    calls: Vec<ToolCall>,
    assistant: Value,
}

const MAX_TOKENS: u32 = 2048;
/// 单次请求最多等多久，免得接口挂住整个流程
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

fn is_openai(settings: &AiSettings) -> bool {
    settings.api_type == "openai"
}

fn request_body(settings: &AiSettings, system: &str, tools: &[ToolSpec], messages: &[Value]) -> Value {
    if is_openai(settings) {
        let tools: Vec<Value> = tools
            .iter()
            .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.schema } }))
            .collect();
        let mut all = vec![json!({ "role": "system", "content": system })];
        all.extend(messages.iter().cloned());
        json!({ "model": settings.model_id, "max_tokens": MAX_TOKENS, "messages": all, "tools": tools })
    } else {
        let tools: Vec<Value> = tools
            .iter()
            .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.schema }))
            .collect();
        json!({ "model": settings.model_id, "max_tokens": MAX_TOKENS, "system": system, "messages": messages, "tools": tools })
    }
}

fn parse_anthropic(v: &Value) -> Turn {
    let content = v["content"].as_array().cloned().unwrap_or_default();
    let text = content.iter().filter_map(|b| (b["type"] == "text").then(|| b["text"].as_str()).flatten()).collect::<Vec<_>>().join("\n");
    let calls = content
        .iter()
        .filter(|b| b["type"] == "tool_use")
        .map(|b| ToolCall {
            id: b["id"].as_str().unwrap_or_default().to_string(),
            name: b["name"].as_str().unwrap_or_default().to_string(),
            input: b["input"].clone(),
        })
        .collect();
    Turn { text, calls, assistant: json!({ "role": "assistant", "content": content }) }
}

fn parse_openai(v: &Value) -> Turn {
    let msg = &v["choices"][0]["message"];
    let calls = msg["tool_calls"]
        .as_array()
        .map(|calls| {
            calls
                .iter()
                .map(|c| ToolCall {
                    id: c["id"].as_str().unwrap_or_default().to_string(),
                    name: c["function"]["name"].as_str().unwrap_or_default().to_string(),
                    // 参数是 JSON 字符串；写坏了就当没给参数，工具会回报缺什么
                    input: c["function"]["arguments"].as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_else(|| json!({})),
                })
                .collect()
        })
        .unwrap_or_default();
    let mut assistant = json!({ "role": "assistant", "content": msg["content"].clone() });
    if !msg["tool_calls"].is_null() {
        assistant["tool_calls"] = msg["tool_calls"].clone();
    }
    Turn { text: msg["content"].as_str().unwrap_or_default().to_string(), calls, assistant }
}

/// 有的 OpenAI 兼容接口不给工具调用编号：补一个，回传的 assistant 消息里也一起改，结果才对得上
fn fill_missing_ids(turn: &mut Turn, round: usize, openai: bool) {
    for (i, call) in turn.calls.iter_mut().enumerate() {
        if !call.id.is_empty() {
            continue;
        }
        call.id = format!("call_{round}_{i}");
        let slot = if openai {
            turn.assistant["tool_calls"].get_mut(i)
        } else {
            turn.assistant["content"].as_array_mut().and_then(|blocks| blocks.iter_mut().filter(|b| b["type"] == "tool_use").nth(i))
        };
        if let Some(v) = slot {
            v["id"] = json!(call.id);
        }
    }
}

/// 工具结果按接口的格式包成要追加的消息
fn result_messages(openai: bool, results: &[(String, String)]) -> Vec<Value> {
    if openai {
        results.iter().map(|(id, out)| json!({ "role": "tool", "tool_call_id": id, "content": out })).collect()
    } else {
        let blocks: Vec<Value> = results.iter().map(|(id, out)| json!({ "type": "tool_result", "tool_use_id": id, "content": out })).collect();
        vec![json!({ "role": "user", "content": blocks })]
    }
}

async fn request(settings: &AiSettings, body: &Value) -> Result<Turn, String> {
    let client = build_http_client(settings)?;
    let resp = apply_auth_headers(client.post(build_api_url(settings)), settings)
        .header("content-type", "application/json")
        .timeout(REQUEST_TIMEOUT)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("请求 AI 失败: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("AI 返回错误（{status}）：{}", text.chars().take(300).collect::<String>()));
    }
    let v: Value = resp.json().await.map_err(|e| format!("AI 回复无法解析: {e}"))?;
    Ok(if is_openai(settings) { parse_openai(&v) } else { parse_anthropic(&v) })
}

/// 跑一次带工具的对话，返回 AI 最后说的话。`messages` 是之前的对话（user / assistant 交替，最后一条是用户的），
/// `call` 执行一次工具调用、返回给 AI 看的结果
pub async fn run_tool_loop(
    settings: &AiSettings,
    system: &str,
    mut messages: Vec<Value>,
    tools: &[ToolSpec],
    max_rounds: usize,
    call: &mut (dyn FnMut(&ToolCall) -> String + Send),
) -> Result<String, String> {
    if settings.api_key.is_empty() {
        return Err("还没有配置 AI：请在 AI 设置里填写 API Key".into());
    }
    let openai = is_openai(settings);
    for round in 0..max_rounds {
        let mut turn = request(settings, &request_body(settings, system, tools, &messages)).await?;
        if turn.calls.is_empty() {
            return Ok(turn.text);
        }
        fill_missing_ids(&mut turn, round, openai);
        let results: Vec<(String, String)> = turn.calls.iter().map(|c| (c.id.clone(), call(c))).collect();
        messages.push(turn.assistant);
        messages.extend(result_messages(openai, &results));
    }
    Ok(format!("查到第 {max_rounds} 轮还没查完，先停在这里；记下的结论都保留了。"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_anthropic_tool_use() {
        let v = json!({ "content": [
            { "type": "text", "text": "先看看" },
            { "type": "tool_use", "id": "t1", "name": "list_dir", "input": { "path": "C:\\x" } }
        ], "stop_reason": "tool_use" });
        let t = parse_anthropic(&v);
        assert_eq!(t.text, "先看看");
        assert_eq!(t.calls, vec![ToolCall { id: "t1".into(), name: "list_dir".into(), input: json!({ "path": "C:\\x" }) }]);
        assert_eq!(t.assistant["content"].as_array().unwrap().len(), 2, "原样回传");
        let msgs = result_messages(false, &[("t1".into(), "ok".into())]);
        assert_eq!(msgs[0]["content"][0]["tool_use_id"], "t1");
    }

    #[test]
    fn parses_openai_tool_calls_with_bad_arguments() {
        let v = json!({ "choices": [{ "message": { "role": "assistant", "content": null, "tool_calls": [
            { "id": "c1", "type": "function", "function": { "name": "find_apps", "arguments": "{\"query\":\"qq\"}" } },
            { "id": "c2", "type": "function", "function": { "name": "list_dir", "arguments": "{oops" } }
        ] } }] });
        let mut t = parse_openai(&v);
        assert_eq!(t.calls[0].input, json!({ "query": "qq" }));
        assert_eq!(t.calls[1].input, json!({}));
        assert!(t.assistant["tool_calls"].is_array());
        // 没给编号的补上，回传的消息里同步
        t.calls[1].id.clear();
        t.assistant["tool_calls"][1]["id"] = json!("");
        fill_missing_ids(&mut t, 3, true);
        assert_eq!((t.calls[1].id.as_str(), t.assistant["tool_calls"][1]["id"].as_str()), ("call_3_1", Some("call_3_1")));
        let msgs = result_messages(true, &[("c1".into(), "a".into()), ("c2".into(), "b".into())]);
        assert_eq!((msgs.len(), msgs[1]["tool_call_id"].as_str()), (2, Some("c2")));
    }

    #[test]
    fn final_answer_has_no_calls() {
        let t = parse_openai(&json!({ "choices": [{ "message": { "role": "assistant", "content": "查完了" } }] }));
        assert!(t.calls.is_empty());
        assert_eq!(t.text, "查完了");
    }
}

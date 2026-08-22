use super::util;
use crate::model::{Domain, Item};

/// Known AI/ML Python libraries we surface under the AI & Agents domain rather
/// than lumping them in with ordinary pip packages.
const AI_LIBS: &[&str] = &[
    "torch",
    "torchvision",
    "tensorflow",
    "keras",
    "jax",
    "transformers",
    "diffusers",
    "accelerate",
    "datasets",
    "sentence-transformers",
    "huggingface-hub",
    "langchain",
    "langgraph",
    "langchain-core",
    "llama-index",
    "openai",
    "anthropic",
    "vllm",
    "onnxruntime",
    "spacy",
    "scikit-learn",
    "opencv-python",
];

pub fn is_ai_lib(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    AI_LIBS.contains(&n.as_str())
}

/// AI Python libraries found in the active/global pip environment.
pub async fn collect() -> Vec<Item> {
    let pip = if util::is_available("pip3") {
        "pip3"
    } else if util::is_available("pip") {
        "pip"
    } else {
        return Vec::new();
    };
    let Some(out) = util::run(pip, &["list", "--format=json"]).await else {
        return Vec::new();
    };
    let mut items = Vec::new();
    if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str(&out) {
        for pkg in arr {
            let name = pkg.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if !is_ai_lib(name) {
                continue;
            }
            let mut item = Item::new(Domain::AiAgent, "python-ai-lib", name);
            if let Some(v) = pkg.get("version").and_then(|v| v.as_str()) {
                item = item.version(v);
            }
            items.push(item);
        }
    }
    items
}

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SynthesizeRequest {
    pub text: String,
    pub voice: Option<String>,
    #[serde(default = "default_model")]
    pub model: String,
    pub context: Option<String>,
    pub api_key: Option<String>,
    pub task_name: Option<String>,  // Optional custom task name
}

#[derive(Debug, Deserialize)]
pub struct VoiceCloneRequest {
    pub text: String,
    pub voice_file_base64: String,
    pub context: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VoiceDesignRequest {
    pub text: String,
    pub voice_description: String,
    pub context: Option<String>,
    pub api_key: Option<String>,
}

fn default_model() -> String {
    "mimo-v2.5-tts".to_string()
}

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapabilityInputContract {
    pub capability: String,
    pub input_schema: Value,
}

impl CapabilityInputContract {
    fn new(capability: &str, input_schema: Value) -> Self {
        Self {
            capability: capability.to_owned(),
            input_schema,
        }
    }
}

fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn optional_nas_selector_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "targetId": {
                "type": "string",
                "minLength": 1
            },
            "mountPoint": {
                "type": "string",
                "minLength": 1
            },
            "displayName": {
                "type": "string",
                "minLength": 1
            },
            "protocol": {
                "type": "string",
                "minLength": 1
            }
        },
        "additionalProperties": false,
        "maxProperties": 1
    })
}

fn speech_synthesize_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "text": {
                "type": "string",
                "minLength": 1,
                "maxLength": 2000
            },
            "backend": {
                "type": "string",
                "enum": ["auto", "local", "online"]
            },
            "voice": {
                "type": "string",
                "enum": [
                    "zf_xiaobei",
                    "zm_yunxi",
                    "af_heart",
                    "zh-CN-XiaoxiaoNeural",
                    "en-US-JennyNeural"
                ]
            }
        },
        "required": ["text"],
        "additionalProperties": false
    })
}

fn web_search_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "minLength": 1,
                "maxLength": 200
            },
            "maxResults": {
                "type": "integer",
                "minimum": 1,
                "maximum": 10
            }
        },
        "required": ["query"],
        "additionalProperties": false
    })
}

fn filesystem_scan_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "minLength": 1
            }
        },
        "required": ["path"],
        "additionalProperties": false
    })
}

fn filesystem_mkdir_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "parent": {
                "type": "string",
                "minLength": 1
            },
            "name": {
                "type": "string",
                "minLength": 1,
                "maxLength": 255
            }
        },
        "required": ["parent", "name"],
        "additionalProperties": false
    })
}

/// A well-known folder the goal names in English or Chinese.
fn named_location(goal: &str) -> Option<std::path::PathBuf> {
    let lower = goal.to_lowercase();
    if lower.contains("downloads") || goal.contains("下载") {
        dirs::download_dir()
    } else if lower.contains("desktop") || goal.contains("桌面") {
        dirs::desktop_dir()
    } else if lower.contains("documents") || goal.contains("文档") {
        dirs::document_dir()
    } else {
        None
    }
}

/// An absolute path the Agent proposed that also appears verbatim in the goal.
fn goal_absolute_path(goal: &str, proposed: Option<&Value>) -> Option<std::path::PathBuf> {
    proposed
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| {
            !path.is_empty() && goal.contains(path) && std::path::Path::new(path).is_absolute()
        })
        .map(Into::into)
}

fn computer_use_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task": {"type": "string"},
            "goal": {"type": "string"},
            "allowedApplications": {
                "type": "array",
                "items": {"type": "string", "minLength": 1},
                "minItems": 1,
                "maxItems": 16,
                "uniqueItems": true
            },
            "maxSteps": {"type": "integer", "minimum": 1, "maximum": 100},
            "maxDurationMs": {"type": "integer", "minimum": 1, "maximum": 900000},
            "displayScope": {"type": "string", "minLength": 1}
        },
        "required": ["task", "goal", "allowedApplications", "maxSteps", "maxDurationMs"],
        "additionalProperties": false
    })
}

pub(crate) fn input_contract_for_capability(capability: &str) -> Option<CapabilityInputContract> {
    let schema = match capability {
        "speech.synthesize" => speech_synthesize_schema(),
        "browser.search" => web_search_schema(),
        "filesystem.scan" => filesystem_scan_schema(),
        "filesystem.mkdir" => filesystem_mkdir_schema(),
        "computer.use.execute" => computer_use_schema(),

        "models.list" => empty_object_schema(),

        "nas.discover" | "nas.list" => empty_object_schema(),

        "nas.status" | "nas.resolve" | "nas.capacity" => optional_nas_selector_schema(),

        _ => return None,
    };

    Some(CapabilityInputContract::new(capability, schema))
}

pub(crate) fn validate_capability_input(capability: &str, input: &Value) -> Result<(), String> {
    let Some(contract) = input_contract_for_capability(capability) else {
        return Ok(());
    };
    let validator = jsonschema::draft7::new(&contract.input_schema)
        .map_err(|_| "Skill input contract is invalid.".to_owned())?;

    validator
        .validate(input)
        .map_err(|_| "Skill input does not match the capability contract.".to_owned())
}

pub(crate) fn ground_capability_input(capability: &str, goal: &str, mut input: Value) -> Value {
    if capability == "computer.use.execute" {
        let Some(object) = input.as_object_mut() else {
            return input;
        };
        let bounded_goal: String = goal.trim().chars().take(2_048).collect();
        let bounded_task: String = bounded_goal.chars().take(256).collect();
        object.insert("task".to_owned(), Value::String(bounded_task));
        object.insert("goal".to_owned(), Value::String(bounded_goal));
        let applications = object
            .get("allowedApplications")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|application| !application.is_empty() && goal.contains(application))
                    .map(|application| Value::String(application.to_owned()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        object.insert("allowedApplications".to_owned(), Value::Array(applications));
        let steps = object
            .get("maxSteps")
            .and_then(Value::as_u64)
            .filter(|steps| *steps > 0)
            .unwrap_or(3)
            .min(3);
        let duration = object
            .get("maxDurationMs")
            .and_then(Value::as_u64)
            .filter(|duration| *duration > 0)
            .unwrap_or(120_000)
            .min(120_000);
        object.insert("maxSteps".to_owned(), json!(steps));
        object.insert("maxDurationMs".to_owned(), json!(duration));
        if !object
            .get("displayScope")
            .and_then(Value::as_str)
            .is_some_and(|scope| goal.contains(scope))
        {
            object.remove("displayScope");
        }
        return input;
    }
    if capability == "filesystem.scan" {
        let Some(object) = input.as_object_mut() else {
            return input;
        };
        let grounded_path =
            named_location(goal).or_else(|| goal_absolute_path(goal, object.get("path")));
        match grounded_path {
            Some(path) => {
                object.insert(
                    "path".to_owned(),
                    Value::String(path.to_string_lossy().into_owned()),
                );
            }
            None => {
                object.remove("path");
            }
        }
        return input;
    }

    if capability == "filesystem.mkdir" {
        let Some(object) = input.as_object_mut() else {
            return input;
        };
        // An explicit absolute path in the goal wins over a named folder.
        let parent = goal_absolute_path(goal, object.get("parent")).or_else(|| named_location(goal));
        match parent {
            Some(path) => {
                object.insert(
                    "parent".to_owned(),
                    Value::String(path.to_string_lossy().into_owned()),
                );
            }
            None => {
                object.remove("parent");
            }
        }
        let name_in_goal = object
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty() && goal.contains(name))
            .map(str::to_owned);
        match name_in_goal {
            Some(name) => {
                object.insert("name".to_owned(), Value::String(name));
            }
            None => {
                object.remove("name");
            }
        }
        return input;
    }

    if !matches!(capability, "nas.status" | "nas.resolve" | "nas.capacity") {
        return input;
    }
    let Some(object) = input.as_object_mut() else {
        return input;
    };

    for key in ["targetId", "mountPoint", "displayName", "protocol"] {
        let grounded = object
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty() && goal.contains(value));
        if !grounded {
            object.remove(key);
        }
    }
    input
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mkdir_input_is_grounded_to_the_goal_in_chinese_or_english() {
        let desktop = dirs::desktop_dir().unwrap().to_string_lossy().into_owned();
        for goal in ["在桌面新建一个叫 test 的文件夹", "Create a folder named test on my Desktop"] {
            let input = ground_capability_input(
                "filesystem.mkdir",
                goal,
                json!({"parent": "~/Desktop", "name": "test"}),
            );
            assert_eq!(input, json!({"parent": desktop, "name": "test"}));
            assert!(validate_capability_input("filesystem.mkdir", &input).is_ok());
        }
        let invented = ground_capability_input(
            "filesystem.mkdir",
            "在桌面新建一个文件夹",
            json!({"parent": "/tmp", "name": "invented"}),
        );
        assert!(invented.get("name").is_none());
        assert!(validate_capability_input("filesystem.mkdir", &invented).is_err());
        let explicit = ground_capability_input(
            "filesystem.mkdir",
            "Create folder reports in /Users/someone/Work",
            json!({"parent": "/Users/someone/Work", "name": "reports"}),
        );
        assert_eq!(explicit["parent"], "/Users/someone/Work");
    }

    #[test]
    fn scan_understands_chinese_folder_names() {
        let input = ground_capability_input("filesystem.scan", "列出我桌面上的文件", json!({}));
        assert_eq!(
            input["path"],
            dirs::desktop_dir().unwrap().to_string_lossy().into_owned()
        );
    }

    #[test]
    fn computer_use_input_is_grounded_to_goal_and_bounded() {
        let goal = "Only com.apple.TextEdit; enter a marker in 3 steps and 120 seconds.";
        let input = ground_capability_input(
            "computer.use.execute",
            goal,
            json!({
                "task": "invented task",
                "goal": "invented goal",
                "allowedApplications": ["com.apple.TextEdit", "com.apple.Safari"],
                "maxSteps": 100,
                "maxDurationMs": 900000,
                "displayScope": "other display"
            }),
        );
        assert_eq!(input["task"], goal);
        assert_eq!(input["goal"], goal);
        assert_eq!(input["allowedApplications"], json!(["com.apple.TextEdit"]));
        assert_eq!(input["maxSteps"], 3);
        assert_eq!(input["maxDurationMs"], 120000);
        assert!(input.get("displayScope").is_none());
        assert!(validate_capability_input("computer.use.execute", &input).is_ok());
    }

    #[test]
    fn nas_discover_and_list_accept_no_agent_authored_fields() {
        for capability in ["nas.discover", "nas.list", "models.list"] {
            let contract = input_contract_for_capability(capability).expect("NAS contract");

            assert_eq!(
                contract.input_schema["additionalProperties"],
                Value::Bool(false)
            );

            assert_eq!(contract.input_schema["properties"], json!({}));
        }
    }

    #[test]
    fn nas_selector_contract_is_provider_and_hardware_neutral() {
        for capability in ["nas.status", "nas.resolve", "nas.capacity"] {
            let contract = input_contract_for_capability(capability).expect("NAS contract");

            let properties = contract.input_schema["properties"]
                .as_object()
                .expect("properties");

            assert!(properties.contains_key("targetId"));
            assert!(properties.contains_key("mountPoint"));
            assert!(properties.contains_key("displayName"));
            assert!(properties.contains_key("protocol"));

            assert!(!properties.contains_key("device"));
            assert!(!properties.contains_key("nas"));
            assert!(!properties.contains_key("hostname"));

            assert_eq!(
                contract.input_schema["additionalProperties"],
                Value::Bool(false)
            );

            assert_eq!(contract.input_schema["maxProperties"], Value::from(1));
        }
    }

    #[test]
    fn unknown_capability_has_no_invented_contract() {
        assert!(input_contract_for_capability("filesystem.read").is_none());
    }

    #[test]
    fn filesystem_scan_requires_one_grounded_path() {
        let contract = input_contract_for_capability("filesystem.scan").expect("scan contract");
        assert_eq!(contract.input_schema["required"], json!(["path"]));
        assert_eq!(
            contract.input_schema["additionalProperties"],
            Value::Bool(false)
        );

        let grounded = ground_capability_input(
            "filesystem.scan",
            "帮我查看 Downloads 文件夹里有哪些文件",
            json!({"path": "Downloads"}),
        );
        assert_eq!(
            grounded["path"],
            Value::String(
                dirs::download_dir()
                    .expect("Downloads directory")
                    .to_string_lossy()
                    .into_owned()
            )
        );
        validate_capability_input("filesystem.scan", &grounded).unwrap();

        let guessed_home = ground_capability_input(
            "filesystem.scan",
            "帮我查看 Downloads 文件夹里有哪些文件",
            json!({"path": "/Users/example/Downloads"}),
        );
        assert_eq!(guessed_home, grounded);

        let invented = ground_capability_input(
            "filesystem.scan",
            "帮我查看本机文件夹里有哪些文件",
            json!({"path": "/Users/example/Downloads"}),
        );
        assert!(validate_capability_input("filesystem.scan", &invented).is_err());
    }

    #[test]
    fn nas_capacity_accepts_empty_or_one_supported_selector() {
        for input in [
            json!({}),
            json!({"targetId": "target-1"}),
            json!({"mountPoint": "/Volumes/Shared"}),
            json!({"displayName": "Shared"}),
            json!({"protocol": "smb"}),
        ] {
            validate_capability_input("nas.capacity", &input).unwrap();
        }
    }

    #[test]
    fn nas_capacity_rejects_invented_multiple_or_unknown_selectors() {
        for input in [
            json!({"device": "my_nas"}),
            json!({"targetId": "target-1", "protocol": "smb"}),
            json!({"unknown": "value"}),
        ] {
            assert!(validate_capability_input("nas.capacity", &input).is_err());
        }
    }

    #[test]
    fn nas_selector_values_must_be_verbatim_grounded_in_the_goal() {
        let invented = json!({
            "displayName": "My NAS",
            "mountPoint": "/Volumes/MyNAS",
            "protocol": "SMB",
            "targetId": "nas-12345"
        });
        assert_eq!(
            ground_capability_input(
                "nas.capacity",
                "帮我看看我的 NAS 还有多少可用空间",
                invented
            ),
            json!({})
        );
        assert_eq!(
            ground_capability_input(
                "nas.capacity",
                "查看 /Volumes/Shared 的容量",
                json!({"mountPoint": "/Volumes/Shared", "displayName": "guessed"})
            ),
            json!({"mountPoint": "/Volumes/Shared"})
        );
    }
}

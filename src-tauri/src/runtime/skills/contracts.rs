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

pub(crate) fn input_contract_for_capability(capability: &str) -> Option<CapabilityInputContract> {
    let schema = match capability {
        "filesystem.scan" => filesystem_scan_schema(),

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
    if capability == "filesystem.scan" {
        let Some(object) = input.as_object_mut() else {
            return input;
        };
        let grounded_path = if goal.contains("Downloads") {
            dirs::download_dir()
        } else if goal.contains("Desktop") {
            dirs::desktop_dir()
        } else if goal.contains("Documents") {
            dirs::document_dir()
        } else {
            object
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|path| {
                    !path.is_empty()
                        && goal.contains(path)
                        && std::path::Path::new(path).is_absolute()
                })
                .map(Into::into)
        };
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

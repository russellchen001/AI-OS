use super::manifest::{
    SkillExecutor,
    SkillManifest,
};


pub fn built_in_skills() -> Vec<SkillManifest> {

    vec![

        SkillManifest {

            id: "filesystem".to_string(),

            name: "Filesystem".to_string(),

            category: "storage".to_string(),

            capabilities: vec![
                "filesystem.read".to_string(),
                "filesystem.write".to_string(),
            ],

            permissions: vec![
                "filesystem.read".to_string(),
                "filesystem.write".to_string(),
            ],

            executor: SkillExecutor {

                kind: "openclaw".to_string(),

                handler:
                    "filesystem".to_string(),
            },
        },


        SkillManifest {

            id: "browser".to_string(),

            name: "Browser".to_string(),

            category: "browser".to_string(),

            capabilities: vec![
                "browser.search".to_string(),
                "browser.control".to_string(),
            ],

            permissions: vec![
                "browser.control".to_string(),
            ],

            executor: SkillExecutor {

                kind: "mcp".to_string(),

                handler:
                    "browser".to_string(),
            },
        },

    ]
}


pub fn find_by_capability(
    capability: &str,
) -> Option<SkillManifest> {


    built_in_skills()
        .into_iter()
        .find(|skill|
            skill.capabilities.contains(
                &capability.to_string()
            )
        )
}

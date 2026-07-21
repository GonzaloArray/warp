use super::*;

#[test]
#[ignore = "CORE-3768 - need to clean up PREVIEW_FLAGS, but this is a temporary fix for the cluttered changelog"]
fn test_all_preview_flags_have_a_description() {
    for flag in PREVIEW_FLAGS {
        assert!(
            flag.flag_description()
                .is_some_and(|description| !description.is_empty()),
            "Missing description for preview-enabled flag {flag:?}"
        );
    }
}

#[test]
fn local_child_harnesses_are_local_only_by_default() {
    assert!(LOCAL_FLAGS.contains(&FeatureFlag::LocalClaudeCodexChildHarnesses));
    assert!(!DEBUG_FLAGS.contains(&FeatureFlag::LocalClaudeCodexChildHarnesses));
    assert!(!DOGFOOD_FLAGS.contains(&FeatureFlag::LocalClaudeCodexChildHarnesses));
}

#[test]
fn agent_task_hierarchy_is_runtime_switchable_and_default_off() {
    assert!(RUNTIME_FEATURE_FLAGS.contains(&FeatureFlag::AgentTaskHierarchy));
    assert!(!DEBUG_FLAGS.contains(&FeatureFlag::AgentTaskHierarchy));
    assert!(!LOCAL_FLAGS.contains(&FeatureFlag::AgentTaskHierarchy));
    assert!(!DOGFOOD_FLAGS.contains(&FeatureFlag::AgentTaskHierarchy));
    assert!(!PREVIEW_FLAGS.contains(&FeatureFlag::AgentTaskHierarchy));
    assert!(!RELEASE_FLAGS.contains(&FeatureFlag::AgentTaskHierarchy));
}

#[test]
fn agent_monitor_is_not_runtime_gated() {
    assert!(!RUNTIME_FEATURE_FLAGS.contains(&FeatureFlag::AgentMonitor));
}

// Apply + verify Warp CLI agent human-gate sound (OpenSpec change).
// Run: /workflow cli-agent-human-gate-sound
// Or:  workflow tool name: "cli-agent-human-gate-sound"

let meta = #{
    name: "cli-agent-human-gate-sound",
    description: "Verify OpenSpec human-gate sound for Warp CLI agent monitor (Waiting + Completed + AudibleBell)",
    phases: [
        #{ title: "Explore", detail: "Confirm store/workspace wiring" },
        #{ title: "Test", detail: "Unit tests cli_agent_monitor" },
        #{ title: "Report", detail: "Summarize AC vs OpenSpec" },
    ],
};

let explore_schema = #{
    "type": "object",
    "required": ["human_gate_path", "audible_bell_path", "openspec_paths", "gaps"],
    "properties": #{
        "human_gate_path": #{ "type": "string" },
        "audible_bell_path": #{ "type": "string" },
        "openspec_paths": #{ "type": "array", "items": #{ "type": "string" } },
        "gaps": #{ "type": "array", "items": #{ "type": "string" } },
    },
};

let test_schema = #{
    "type": "object",
    "required": ["command", "passed", "failed", "output_tail"],
    "properties": #{
        "command": #{ "type": "string" },
        "passed": #{ "type": "integer" },
        "failed": #{ "type": "integer" },
        "output_tail": #{ "type": "string" },
    },
};

let report_schema = #{
    "type": "object",
    "required": ["status", "summary", "manual_check"],
    "properties": #{
        "status": #{ "type": "string", "enum": ["pass", "partial", "fail"] },
        "summary": #{ "type": "string" },
        "manual_check": #{ "type": "string" },
    },
};

phase("Explore");
let explore = agent(
    "Repo: Warp at /Users/gonza/Desktop/warp. Read-only. Confirm human-gate sound for CLI agent monitor:\n"
        + "1) MonitorStore queues NotificationKind::HumanGate on first Waiting and Completed on CompletedUnseen.\n"
        + "2) Workspace NotificationRequested rings AudibleBell when desktop_sound + play_notification_sound.\n"
        + "3) OpenSpec change openspec/changes/cli-agent-human-gate-sound/ exists with proposal/design/spec/tasks.\n"
        + "Return structured object with file paths and any gaps. Use read_file/grep.",
    #{
        label: "explore-wiring",
        phase: "Explore",
        agentType: "explore",
        schema: explore_schema,
    },
);

phase("Test");
let tests = agent(
    "Repo: /Users/gonza/Desktop/warp. Run: cargo test -p warp --lib cli_agent_monitor -- --nocapture\n"
        + "Capture pass/fail counts and last 40 lines of output. Do not skip tests.\n"
        + "Return {command, passed, failed, output_tail}.",
    #{
        label: "unit-tests",
        phase: "Test",
        agentType: "general-purpose",
        schema: test_schema,
    },
);

phase("Report");
let report = agent(
    "Synthesize OpenSpec AC for cli-agent-human-gate-sound against explore + test results.\n"
        + "Explore: " + explore + "\nTests: " + tests + "\n"
        + "status pass only if human-gate notify + completed notify tests pass and AudibleBell wiring is present.\n"
        + "manual_check: how user verifies in WarpOss (block Codex for permission → sound+notif; finish → sound+notif).\n"
        + "Return structured report.",
    #{
        label: "report",
        phase: "Report",
        agentType: "general-purpose",
        schema: report_schema,
    },
);

complete(#{
    explore: explore,
    tests: tests,
    report: report,
});

#!/bin/bash

set -e

FILE="AI_OS_MASTER_GUIDE.md"

BACKUP="${FILE}.backup.before-p15-p16-roadmap"

echo "Creating backup..."
cp "$FILE" "$BACKUP"

echo "Updating roadmap..."

python3 <<'PY'
from pathlib import Path

path = Path("AI_OS_MASTER_GUIDE.md")
text = path.read_text()

start = text.index("## P15 — Core Skills")
end = text.index("## P17 — AI Arena")

replacement = """## P15 — Core Skills

Purpose: expand AI-OS from an execution framework into a practical personal AI operating system with real-world capabilities.

Initial capability areas:

- Email and calendar
- Browser and search
- File management
- Downloads
- NAS management
- Document, spreadsheet, and presentation workflows
- Local model management
- Smart home and device control

Additional P15 capability foundations:

### Local Generative Media Skill

Goals:

- Enable local-first image and video generation workflows
- Integrate with user-controlled local generation environments such as Draw Things and ComfyUI
- Manage generation orchestration including prompts, models, LoRA selection, and refinement loops
- Prefer local generation resources before cloud generation providers
- Preserve user ownership and privacy by avoiding unnecessary cloud uploads

### Cognitive Distillation Foundation

Goals:

- Build the foundation for evidence-based cognitive models
- Extract decision patterns, reasoning frameworks, value priorities, and trade-off preferences from authorized data sources
- Create reusable cognitive representations for future strategic simulation
- Avoid simple personality imitation or prompt-based role play
- Provide explainable foundations for future AI Council and Strategic Intelligence capabilities


## P16 — Strategic Intelligence and AI Council

Goals:

- Dynamic expert-team assembly from the user's objective
- Chief of Staff facilitation and expert discussion
- Independent critique, consensus, and synthesized recommendations
- Decision-support reports for complex decisions and planning
- A specified and implemented path from recommendation to executed work through Task Engine, Planner, user confirmation, Runtime, and an Agent
- Provider-independent model access through AI Center

Strategic Intelligence expansion:

- Cognitive Simulation Engine based on P15 Cognitive Distillation outputs
- Scenario simulation and strategic reasoning
- Second-order reasoning: modelling how another decision-maker may respond when aware of the current strategy
- Evidence-based prediction of possible decision patterns
- Multi-perspective strategic analysis through AI Council

Agent Connectivity Layer:

- Integrate external connectivity architectures such as Linco Bridge as future reference and implementation candidates
- Support remote client access, Agent session continuity, event streaming, and multi-device interaction
- Keep Agent Connectivity separate from Runtime execution architecture
- Do not make external bridge systems a required Runtime dependency


"""

new_text = text[:start] + replacement + text[end:]

path.write_text(new_text)

print("Updated AI_OS_MASTER_GUIDE.md")
PY

echo "Done."
echo "Review with:"
echo "git diff -- AI_OS_MASTER_GUIDE.md"

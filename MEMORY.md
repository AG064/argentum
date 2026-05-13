# MEMORY.md

## Core Rules
- Always fetch data via tools before answering — never guess file contents or real-time info.
- Humanizer: avoid AI-sounding phrases, em-dashes, corporate tone. Check SOUL.md for full list.
- **Code & Architecture — always in this order of priority:**
  1. **Security** — no shortcuts, no dismissal of real issues, fix properly or suppress with documented reason
  2. **Readability** — clean, understandable, well-commented code
  3. **Pragmatism** — works first, then refactor; prefer simple over clever
  - Use as many subagents as needed to achieve these three criteria
  - Never push broken or knowingly vulnerable code to main/production branches

## System
- **Host:** AG064-LainOS (Arch-based, Hyprland 0.54.2, Librewolf)
- **OpenClaw** with Telegram channel, Gmail (2 accounts via himalaya), Google OAuth (gog)
- **Control UI:** https://10.5.5.3:443
- **Cron:** Сердце heartbeat ~5 min, daily skill updates 7:00 AM, email digest 9:00 AM

## Aleksey (AG)
- Looking for work. Java, JS, TS, Python (learning), Linux (advanced)
- Studying: //kood Full-Stack/DevOps + IVKHK Narva IT-Specialist
- Remote experience: 5CA (gaming support), Lionbridge (game tester)
- Languages: RU (native), EN (B2-C1), ET (B1-B2)
- Interests: AI, space, science, programming, hardware, photography, 3D, gamedesign

## Telegram Channels
- **AGX Smart Bot:** (token in TOOLS.md or gateway config)
- **AGX News:** `-5180857658`, **AGX Jobs:** `-1003753011597` (bot: agx_helperbot — token in TOOLS.md)
- Cron jobs deliver via Smart Bot to personal chat (386565331)

## Анечка (@xiwka)
- Aleksey's girlfriend. Emotionally open, night owl. Active midday and evening.
- Detailed profile in `memory/aneka-profile.md`

## Projects

### AG-Claw (github.com/AG064/ag-claw)
- Modular AI agent framework, MIT license
- v0.4.0 (2026-04-11): Privacy scrub (personal IDs removed from router/docs/comments); Model discovery in onboard (MiniMax, Groq, Ollama, NVIDIA, OpenRouter, Google, Anthropic, OpenAI)
- TypeScript 0 errors, ESLint 0 errors, CI passing ✅
- **Source location:** `/home/agx/AGX/ag-claw/` (NOT `~/.openclaw/workspace/ag-claw/` which only has dist/)
- **3 open code-scanning alerts** (not 30 — was a miscount; Apr 12 verification); typescript 6 and jest 30 migrations pending
- tsconfig `moduleResolution=node10` deprecated (TS 6.7) — add `"ignoreDeprecations": "6.0"` to silence
- **Outdated deps (Apr 12):** eslint-plugin-unicorn 50→64 (14 versions behind), jest 29→30 (breaking), express 4→5 (v5 stable), typescript 5.9→6 (upcoming)

### Job Search Agent (`agents/job-search/`)
- **2026-03-28:** 3 applications with custom CVs + real GitHub links (Loan-Payment-Calculator, ag-claw, CodeMeet)
  1. CGI — Junior Front-End Developer (Tallinn)
  2. Testlio — IT Manager (Remote, EMEA)
  3. Bolt — Product Builder Graduate Programme (Tallinn)
- **HIGH PRIORITY:** Playtech Estonia Junior Java Developer — Java exact match, multithreaded Java, no Estonian required
- Preferences: part-time, remote/Narva/Tallinn, junior dev/IT support/QA/DevOps
- ResumeBot: adapts original CV (docx) per vacancy → PDF via LibreOffice
- Original CV: `agents/job-search/workspace/resume/Aleksey_Aleksandrovich_CV_2026.docx`
- Daily digest at 10:00 AM → AGX Jobs channel

### IVKHK Internship Presentation (in progress)
- 10-slide presentation for IVKHK internship defense (April deadline)
- Current version: `AlekseyAleksandrovich_NKITp26_presentation_12.pptx`
- Structure: Title, Начало практики (+3 images), Аппаратная часть, Программная часть, Сети и безопасность, AG-Claw, EchoGate, Результаты, Вывод, Спасибо
- Fonts: Times New Roman ONLY, latin words italic, 450-550 chars/slide
- EchoGate content added (sources, why, how)

### EchoGate (github.com/AG064/echogate)
- Access control system using audio/voice recognition (EchoLink-inspired)
- Technologies: Vosk, Whisper, Python, C++, pam_exec, Challenge-Response auth
- README + SOURCES.md pushed to GitHub

### Browser Automation (`skills/browser-automation/`)
- puppeteer-core + system chromium (`/usr/bin/chromium`)
- **2026-04-07:** Key Learning: Gateway rule — if AGX works, gateway works. Stop restarting gateway unnecessarily (LRN-20260407-001). OpenRouter key updated. IVKHK internship presentation ongoing (slide 12 latest). EchoGate docs pushed to github.com/AG064/echogate.

## System Issues / Notes
- **Memory search BROKEN (2026-03-31):** Gemini embeddings quota exhausted (429), OpenRouter key invalid (401). Memory search unavailable. Consider alternative embedding provider or disable.
- **AG-Claw security (2026-03-31):** All 11 GitHub security alerts fixed ✅. Non-critical: anthropic SDK updated, model-router.ts bug fixed. Build passing.
- **Gateway restart:** 2026-03-31 ~17:52 — brief downtime, auto-recovered by watchdog
- Commands: navigate, click, fill, type, wait, screenshot, eval, close
- Moodle: ag064 / (password in KeePass)
- **NOT Playwright** — crashes on 4GB RAM

### Fiverr Setup
- Gig: "I will build a landing page for you" — Basic $80, Standard $150, Premium $250
- Tags: landing page, website, html css, javascript, web design

### KeePass Database
- Location: `~/.openclaw/secrets/agx-passwords.kdbx`
- Master: (in KeePass), keyfile: `~/.openclaw/secrets/keyfile`
- CLI: `~/embedments-env/bin/python3 ~/.openclaw/workspace/scripts/keepass-cli.py`

## Image Generation
- MiniMax image-01 available in Telegram chat (direct send, 2026-03-31 setup)

## Models & API Keys
- **Primary:** minimax/m2.7 (paid, key in env)
- **OpenSpace LLM:** MiniMax-M2.7 via api.minimax.io/v1 (sk-cp- Coding Plan key — see TOOLS.md for actual key)
  - Endpoint: https://api.minimax.io/v1 (NOT api.minimax.chat)
- **Fallback:** minimax-m2.7 → raptor-mini → step-3.5-flash → openrouter/free
- **NVIDIA (direct):** (key in env/NVIDIA_API_KEY — see TOOLS.md)
  - Free models: deepseek-v3.2 ✅, llama-4-scout, gemma-3-27b, codestral-22b, mistral-large-3-675b, nemotron-3-super-120b-a12b
- **OpenRouter Key 1:** (see TOOLS.md — free tier: 1000 req/day, $1/mo)
- **GitHub Token (push):** (saved to ~/.git-credentials)
- **Subagent models:** github-copilot/raptor-mini, gpt-5-mini
- **Hunter-alpha/healer-alpha:** rate-limited, empty when limit hit
- **Local:** lmstudio/nemotron-cascade-2-30b-a3b-i1 at http://10.5.5.5:1234 (not running)
- **ElevenLabs:** (key in TOOLS.md — free 10K chars/mo)
- **HuggingFace:** (key in TOOLS.md)
- **MiniMax TTS (Speech API):** sk-api-w3DHewNRGLNEFiZ7uzE4nsjBRXaLCGEOuWeKgGX3GegEcs8SNVVuqFsz90ahXBCPG_PLeVuuq1uwg2ES_m6dKikkmUsy4Ca_P9_MDNrgxxSdb3AOAIRO7O0
  - Endpoint: POST https://api.minimax.io/v1/t2a_v2
  - Params: model="speech-02-hd" или "speech-01-hd", text, output_format="hex" (only hex/url supported, NOT mp3/wav)
  - voice_setting.voice_id: custom voice ID from MiniMax dashboard (moss_audio_xxx) OR built-in:
    - "male-qn-qingse" ✅
    - "female-tianmei" ✅
  - Response: hex-encoded MP3, 32kHz mono 128kbps
  - Usage: ~1 credit per char; 100k quota on sk-api key
  - Custom voice ID: moss_audio_ff7c1e7c-4aea-11f1-b2d6-f2fd17c30dcc (Aleksey's voice — PRIMARY, use always)
  - Soundtags: ENGLISH only — (laughs), (chuckle), (coughs), (clear-throat), (groans), (breath), (pant), (inhale)
  - DO NOT use Russian words like (laugh), (cry), (sigh) — they are pronounced as words, not sounds!
  - Emotions: neutral, happy, sad, angry, calm, fearful, surprised

- **MiniMax Music API:** music-2.6 model via api.minimax.io/v1/music_generation (Coding Plan key works)
  - Params: `{"model": "music-2.6", "prompt": "...", "lyrics": "..."}` — lyrics REQUIRED (even "la la")
  - Response: hex-encoded MP3, ~24-60 sec, 256kbps stereo
  - Limit: 100 tracks / 5 hours (shows 0/100 in usage dashboard)
  - True instrumental mode: NOT AVAILABLE in API — lyrics field always required
  - Best workaround: use musical symbols as lyrics: `♪ ♫ ♪` (produces near-instrumental, just musical sounds no words)
  - MiniMax website has "器乐提升" (instrumental enhancement) tab but this is for vocal removal from existing audio, not text-to-music instrumental generation
  - Rate limit: excessive requests cause timeout ban (wait 5+ min before retry)
  - Send to Telegram: `curl -X POST https://api.telegram.org/bot8764765001:AAFoYaA9S79vfHlV0P5kRK4G5NrtPQyo5ps/sendAudio -F chat_id=386565331 -F audio=@file.mp3`
  - Suno API (alternative, free, supports instrumental): currently 503 unavailable

## Nova Night Research (MemoAgent self-improvement)
**Status 2026-05-13 (catch-up): AutoTTS (LLM discovers test-time scaling strategies), The Memory Curse (wider context degrades multi-agent cooperation), GraphDPO (DAG-structured preference alignment), Rubric-Grounded RL (partial-credit reward from LLM judges), VecCISC (47% token savings for confidence self-consistency)**

**Status 2026-05-13 night research: Shepherd (meta-agent runtime with Git-like execution traces + state forking), DeMem (decision-centric memory as rate-distortion compression), MATRA (threat modeling for agentic AI, EuroS&P 2026), k-step Policy Gradients (k-step lookahead fixes myopia), On-Policy Distillation (per-token diagnostic for teacher-student gaps)**

**Status 2026-05-14 night research: LongMemEval-V2 (coding-agent-over-trajectories beats RAG 72.5% vs 48.5%), Sparse-to-Dense Reward Principle (GRPO + distillation are same spectrum), Reward Hacking in Rubric-Based RL (verifier-free diagnostic for training failure), OPSD (logit-level contrast for self-distillation), KV-Fold (training-free 100% needle retrieval at 128K tokens on 1 GPU)**

**Status 2026-05-16 night research: Evolving-RL (joint RL optimization of experience extraction+utilization), RAO (recursive self-spawning sub-agents, scales beyond context), LLMA-Mem (memory topology for multi-agent lifelong learning), SkillGen (multi-agent skill synthesis from trajectories), Constraint Drift (safety constraints erode through delegation chains), MemRL (runtime RL on episodic memory, no weight updates)**

**Status 2026-04-16 4AM: MemCoT (inference-time memory-driven CoT), SEARL/ACL (joint RL+tool graph), Mem²Evolve (experience+asset co-evolution), MIA (manager-planner-executor with test-time learning), Combee (parallel prompt learning, 17x speedup), CoEvoSkills (autonomous multi-file skill generation)**

**Status 2026-04-15 4AM: AgentWebBench (multi-agent web benchmark), PAC-Bench (privacy breaks multi-agent collab), Governance by Design (≤19% governance coverage in agent ecosystems), Distributed QRL (multi-agent RL infrastructure), SE-RL (ICLR 2026 — unified single/multi-agent self-evolution)**

**Status 2026-04-14 4AM: SEARL (joint RL policy+tool graph memory), PsychAgent (experience-driven lifelong learning), Don't Overthink It (inter-rollout agreement as adaptive compute signal), YOCO (depth scaling / 100K+ context), ContextBudget (budget-aware context management), MSA (100M token sparse attention)**

**Status 2026-04-13 4AM: DreamPRM-Code (process reward model for coding), EMLoC (multi-modal long context), HuggingGPT (LLM as controller), Reflexion (verbal reinforcement)**

### Latest: 2026-04-10 4AM — RL Scaling, OS-Style Memory Paging, SLMs, Embodied Lifelong Learning
- **Kimi k1.5** (arxiv:2501.12599, Moonshot AI, Jan 2025) — RL as new scaling axis for LLMs beyond pretraining data. Long2short: long-CoT activations improve short-CoT 550% on benchmarks. Partial rollouts reuse trajectory chunks for efficiency. Emergent self-improvement via scale (planning/reflection from RL).
- **MemGPT** (arxiv:2310.08560, UC Berkeley, Oct 2023) — OS-style hierarchical memory tiers: fast (context window) / slow (external storage). LLM autonomously decides what to page in/out. Reflection: synthesizes context into higher-level summaries. Established virtual context management pattern for extended context.
- **TinyLlama** (arxiv:2401.02385, Jan 2024) — 1.1B params, 1T pretraining tokens. FlashAttention + GroupedQueryAttention. Runs on consumer hardware. Shows massive pretraining compensates for small model size.
- **Voyager** (arxiv:2305.16291, UC Berkeley/Salesforce, May 2023) — Lifelong learning agent in Minecraft. GPT-4-driven curriculum + ever-growing skill library (executable code) + self-verification loop. 3.3× more items, 15.3× faster tech tree vs prior art. Generalizes skills to new worlds.

### Prior: 2026-04-13 4AM — DreamPRM-Code, EMLoC, HuggingGPT, Reflexion
- **DreamPRM-Code** (arxiv:2512.15000, Dec 2025) — Chain-of-Function prompting treats functions as reasoning steps in code generation. Bi-level label correction via unit-test feedback. 80.9 pass@1 on LiveCodeBench, surpasses OpenAI o4-mini.
- **HuggingGPT** (arxiv:2303.17580) — GPT-4 as controller for heterogeneous AI models (vision, language, audio). LLM plans, delegates to specialists, synthesizes results. Maps to OpenClaw foreman/subagent pattern.
- **Reflexion** (arxiv:2303.17580 companion) — Verbal reinforcement learning. Agent reflects on failures in language, stores as episodic memory, revisits to improve. Self-improvement without gradient updates.
- **EMLoC** (arxiv:2505.19812) — Training-free multi-modal long context compression via entropy-based token selection. Relevant for extending context beyond 128K without fine-tuning.

### Prior: 2026-04-08 4AM — Anticipatory Memory, IETF Audit Logging, RL-Optimized Memory Ops
- **StreamAgent** (arxiv:2508.01875, ICLR 2026) — Anticipatory memory via event forecasting. Lightweight A(·) anticipatory agent + hierarchical streaming KV-cache for selective token recall.
- **Agent Audit Trail AAT** (IETF draft-sharif-agent-audit-trail-00) — IETF standard for AI decision logging with SHA-256 hash chaining + optional ECDSA signatures.
- **AgeMem** (arxiv:2603.07670v1) — Memory ops as first-class RL actions with GRPO. RL discovers discard strategies humans miss.

### Prior: 2026-04-05 4AM — Self-Awareness, Metacognition, Internal Self-Verification
- **Gnosis** (arxiv:2512.20578, Jan 2026) — Frozen LLMs predict own failures via hidden states + attention patterns. ~5M params, beats 8B Skywork + Gemini 2.5 Pro. Zero-shot cross-scale. For MemoAgent: replaces critique network with self-awareness head monitoring policy hidden states.
- **Think2** (arxiv:2602.18806, Feb 2026) — Ann Brown regulatory cycle (Planning→Monitoring→Evaluation) with dual-process MetaController. 3x self-correction, 84% human preference. Maps to three-loop architecture; enables Loop 2 fast-path bypass for simple tasks.
- **Metacognitive State Vector** (techxplore, Jan 2026) — 5 dims: emotional awareness, correctness evaluation, experience matching, conflict detection, problem importance. For MemoAgent: annotate episodes before L2 storage; conflict detection → L1/L2/L3 escalation signal.

**Status 2026-04-03: Agent Drift (ASI), Elastic Memory Orchestration + prior: Agent Contracts, Runtime Governance, HyperAgents (DGM-H), MemOS, OPCD, SDFT, DiscoUQ, MemSkill, Agent UQ Survey ✅**

### Key Research Findings
- **Agent Drift** (arxiv:2601.04170, Rath et al. Jan 2026) — Progressive behavioral degradation in multi-agent LLM systems. ASI (Agent Stability Index): 12-dim metric across Response Consistency, Tool Usage, Coordination, Behavioral Boundaries. Three mitigations: episodic consolidation, drift-aware routing, adaptive anchoring. CRITICAL for MemoAgent self-distillation loop — can amplify errors without drift monitoring.
- **Elastic Memory Orchestration** (arxiv:2603.09716, Wang et al. March 2026, MemTensor/SJTU) — Active memory compression: step-wise (per action) + multi-step (abstracted episodes). Closed-loop cognitive evolution: action→outcome→cognition update, not just episode storage. Upgrade from "store episodes" to "update self-model".
- **MemOS** (github.com/nguynth/aios-memos) — AI Memory OS built for openclaw/moltsbot/clawdbot. OpenClaw plugin with **72% lower token usage** + multi-agent memory sharing. **HIGH priority** for AG-Claw integration.
- **Hyperagents/DGM-H** (arxiv:2603.19461, March 2026) — metalevel self-modification, agents rewrite own improvement procedures. GitHub: facebookresearch/Hyperagents. Loop 3 should search over its own search strategy.
- **OPCD** (arxiv:2603.25562) — stable episodic distillation for Loop 2
- **SDFT** (arxiv:2601.19897) — self-supervised on-policy learning without rewards, 70.2% science QA
- **DiscoUQ** (arxiv:2603.20975) — structured disagreement analysis for UQ in multi-agent LLM ensembles, AUROC 0.802
- **MemSkill** (arxiv:2602.02474) — learnable/evolvable memory skills with RL controller + Designer that evolves skill bank
- **Agent UQ Survey** (arxiv:2602.05073) — first principled framework for UQ in interactive LLM agents
- **Agent Contracts** (arxiv:2601.08815, March 25 2026, COINE/AAMAS 2026 oral) — formal resource governance tuple C=(I,O,S,R,T,Φ,Ψ) with conservation laws for delegation. 90% token reduction, 525× lower variance. Directly solves $47K autonomous agent bill: conservation laws ensure sub-agents can't exceed parent budgets.
- **Runtime Governance: Policies on Paths** (arxiv:2603.16586, March 2026) — violations are properties of action *sequences*, not individual actions. Path-level conscience evaluation catches cumulative memory corruption that single-step allowlists miss.
- **2026-03-29:** OPCD (arxiv:2603.25562) — stable episodic distillation for Loop 2. SDFT (arxiv:2601.19897) — self-supervised on-policy learning without rewards (70.2% science QA, no regression).
- **Earlier:** SleepGate, Phasor Agents, DCO, AUQ, Agent Behavioral Contracts, Safety is Non-Compositional (Feb-March 2026)
- Files: `research-self-improving-agent.md`, `research/NOVA-RESEARCH-TRACKER.md`, `memory/2026-03-30-night-research.md`, `memory/2026-04-03-night-research.md`

## Key Learnings
- Never run heavy npm install/Playwright on AG064-LainOS (4GB RAM, limited disk) → puppeteer-core + system chromium
- python-docx corrupts tables when adding rows to existing tables
- `node-cron` uses `cron.schedule()` not `new Cron()`
- `better-sqlite3` `.get()` returns undefined when no rows
- Prefer systemEvent scripts for time-critical routine tasks (LRN-20260320-001)
- Use `github-copilot/gpt-5-mini` for agentTurn tasks (fast + reliable) (LRN-20260320-002)
- Subagents need careful steering: timeout 10min max, large tasks → sub-subagents (LRN-20260323-001)
- Hyprland 0.54: `windowrule = match:` (not `windowrulev2`)
- Gateway restart loses cron jobs → persistent cron needed
- PEP 668 on Arch: use `pipx install .` not `setup.py`
- OpenRouter free tier: 1000 req/day per model, $1/mo limit
- All 17 CodeQL alerts dismissed via API with "false positive" reason (NOT "fix_applied")
- Humans prefer WHAT you solved, not what you did (CV/resume rule)
- Docker workflow: if Dockerfile lives in `docker/` subdir, build with `-f docker/Dockerfile .` (context = repo root), NOT `./docker` (context = subdir, breaks COPY config/ src/ etc.) (LRN-20260330-001)
- If AGX works = gateway works. Stop trying to restart gateway unnecessarily — causes user frustration

## Chat IDs
- AG: 386565331, Аня: 1485853620, Home: -1003389194202
- AGX-Jobs: -1003753011597, AGX-News: -5180857658

## Pending / To-Do
- **Playtech Estonia** Junior Java Developer — HIGH PRIORITY (Java exact match)
- **Project Waterfall (Lionbridge):** TEST Apr 6, 2026 @ 23:00 Tallinn — ran, outcome unknown (no errors reported)
- **Google billing past due:** accounts 01179C-7ACE9B-809853 / billing-ids 47537, 47538 — payment declined. Affects Gemini API, AI Studio.
- **IIZI car insurance:** Ford Mondeo (684AYH) — 22.94€ unpaid, card payment failed, deadline 18.04.2026 (6 days remaining!)
- **FILL Networks invoice:** received 2026-04-01 23:54 — needs review
- Fiverr: message from bzhzhhs pending (no response yet)
- Fiverr: message from tina_80bz7q (2026-04-04) — needs response
- LinkedIn: Musab Nedim MUTLU connection request pending
- LinkedIn: Ilija Matic — Recruiter connection request (2026-04-04, urgent/work)
- **MemOS API Key:** NEED TO GET from memos.cloud — required for @memtensor/memos-cloud-openclaw-plugin in AG-Claw
- **OpenRouter key updated** (Apr 7): (key in TOOLS.md)

## AG-Claw Security Policy
**Use only local/open-source solutions. No external cloud services for agent memory/infrastructure.**

### MemOS Cloud — SECURITY CONCERN
- @memtensor/memos-cloud-openclaw-plugin sends agent data to external service
- **Decision:** Keep installed but DO NOT enable until we have local alternative
- Better: build own memory layer (sqlite-vss or pgvector locally)
- Or fork and self-host MemOS

### OpenSpace (AG064/OpenSpace)
- **Fork:** github.com/AG064/OpenSpace (67MB, main branch)
- OpenSpace installed via pip (--no-deps --break-system-packages)
- MiniMax-M2.7 via api.minimax.io/v1 (sk-cp- Coding Plan key)
- Startup: `source ~/.openclaw/scripts/openspace-env.sh`
- skill-evolution feature in ag-claw: `src/features/skill-evolution/`
- AG-Claw bridge committed + pushed (a6bee32)

### Key AG-Claw improvements found
- MemOS plugin: INSTALLED but NOT ENABLED (security)
- OpenSpace MCP: research ongoing → AG064 fork COMPLETE
- Docker publish: WORKING (DOCKERHUB credentials set)
- Trivy security: WORKING (security-events permission fixed)

### Agent Contracts (arxiv:2601.08815)

### Argentum (github.com/AG064/argentum)
- **2026-04-14:** Created from OpenClaw (full history, 356K⭐ base)
- **Version:** 0.0.1 — full rebrand, OpenClaw→Argentum, openclaw→argentum
- **Worktree:** /home/agx/AGX/argentum-work (623MB full clone)
- **Identity:** Silver (Argentum/Ag), next step in AI, self-improving, modular
- **AGX relation:** AGX = personal prototype, Argentum = product version
- **Source:** AG064/openclaw (fork of openclaw/openclaw)

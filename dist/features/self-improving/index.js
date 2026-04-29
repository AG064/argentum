"use strict";
/**
 * Self-Improving Loop Feature
 *
 * The reflection engine that makes Argentum get smarter over time.
 * Runs 5 phases during idle time or on schedule.
 *
 * Phase 1: Error Analysis     — Review failed tasks, user corrections
 * Phase 2: Skill Creation    — Create reusable skills from patterns
 * Phase 3: Memory Consolidation — FTS5-powered memory refresh
 * Phase 4: User Model Update  — Refine understanding of user preferences
 * Phase 5: Self-Correction    — Update SOUL.md and response strategies
 *
 * CLI:
 *   argentum improve           — Run full loop
 *   argentum improve --phase skill   — Run specific phase
 *   argentum improve --dry-run      — Show what would change
 *   argentum improve --force        — Force run even if recently ran
 *   argentum learnings           — Show accumulated lessons
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
const fs = __importStar(require("fs"));
const fs_1 = require("fs");
const path = __importStar(require("path"));
const analyzer_1 = require("./analyzer");
const skill_creator_1 = require("./skill-creator");
// ─── Constants ────────────────────────────────────────────────────────────────
const DEFAULT_CONFIG = {
    enabled: true,
    schedule: 'nightly',
    nightlyTime: '03:00',
    idleThreshold: 120,
    skillCreationThreshold: 5,
    maxSkillsPerRun: 3,
    autoPublishToHub: false,
    verbose: false,
    dryRun: false,
    forceRun: false,
};
const LAST_RUN_FILE = 'last-improve-run.txt';
const CONFIG_FILE = 'self-improving-config.json';
// ─── Feature ─────────────────────────────────────────────────────────────────
class SelfImprovingLoop {
    meta = {
        name: 'self-improving',
        version: '0.0.4',
        description: 'Reflection engine that analyzes behavior and continuously improves Argentum',
        dependencies: ['sqlite-memory', 'user-modeling'],
    };
    ctx;
    config = { ...DEFAULT_CONFIG };
    workDir = '';
    memoryDir = '';
    skillsDir = '';
    sessionsDbPath = '';
    initialized = false;
    lastRunTime = 0;
    // Sub-modules
    analyzer;
    skillCreator;
    // ─── Lifecycle ─────────────────────────────────────────────────────────────
    async init(config, context) {
        this.ctx = context;
        // Determine paths
        this.workDir =
            config['workDir'] ||
                process.env.AGCLAW_WORKDIR ||
                path.join(process.env.HOME || '~', '.openclaw', 'workspace');
        this.memoryDir = path.join(this.workDir, 'memory');
        this.skillsDir = path.join(this.workDir, 'skills');
        this.sessionsDbPath = path.join(this.workDir, 'data', 'sessions.db');
        // Load config
        this.loadConfig();
        this.loadLastRun();
        // Override from feature config if provided
        if (config['enabled'] !== undefined)
            this.config.enabled = Boolean(config['enabled']);
        if (config['schedule'])
            this.config.schedule = config['schedule'];
        if (config['nightlyTime'])
            this.config.nightlyTime = String(config['nightlyTime']);
        if (config['verbose'])
            this.config.verbose = true;
        // Init sub-modules
        this.analyzer = new analyzer_1.ErrorAnalyzer(this.sessionsDbPath, this.memoryDir);
        this.skillCreator = new skill_creator_1.SkillCreator(this.skillsDir, this.memoryDir, this.config.skillCreationThreshold);
        // Create directories
        (0, fs_1.mkdirSync)(path.join(this.memoryDir, 'self-improvement'), { recursive: true });
        this.initialized = true;
        this.ctx.logger?.info('SelfImprovingLoop initialized', {
            workDir: this.workDir,
            lastRun: this.lastRunTime ? new Date(this.lastRunTime).toISOString() : 'never',
        });
    }
    async start() {
        this.ctx.logger?.info('SelfImprovingLoop active', {
            schedule: this.config.schedule,
            nightlyTime: this.config.nightlyTime,
        });
    }
    async stop() {
        // Nothing to clean up
    }
    async healthCheck() {
        const lessonsPath = path.join(this.memoryDir, 'self-improvement', 'lessons.md');
        const lessonsCount = (0, fs_1.existsSync)(lessonsPath)
            ? fs.readFileSync(lessonsPath, 'utf8').split('\n##').length - 1
            : 0;
        return {
            healthy: this.config.enabled,
            details: {
                enabled: this.config.enabled,
                schedule: this.config.schedule,
                lastRun: this.lastRunTime ? new Date(this.lastRunTime).toISOString() : 'never',
                autoSkillsCount: this.skillCreator.listAutoSkills().length,
                lessonsLogged: lessonsCount,
            },
        };
    }
    // ─── Public API ────────────────────────────────────────────────────────────
    /**
     * Run the full self-improving loop or a specific phase
     */
    async run(phase = 'all', options = {}) {
        const startTime = Date.now();
        const phases = [];
        // Apply options
        const originalDryRun = this.config.dryRun;
        const originalForceRun = this.config.forceRun;
        const originalVerbose = this.config.verbose;
        if (options.dryRun !== undefined)
            this.config.dryRun = options.dryRun;
        if (options.force !== undefined)
            this.config.forceRun = options.force;
        if (options.verbose !== undefined)
            this.config.verbose = options.verbose;
        this.log('Starting self-improving loop', { phase: String(phase), dryRun: this.config.dryRun });
        // Check if recently ran (skip if --force not set)
        if (!this.config.forceRun && !this.config.dryRun) {
            const hoursSinceLastRun = this.lastRunTime
                ? (Date.now() - this.lastRunTime) / (1000 * 60 * 60)
                : Infinity;
            if (hoursSinceLastRun < 6) {
                this.log(`Skipping: ran ${hoursSinceLastRun.toFixed(1)}h ago (use --force to override)`);
                return {
                    phases: [],
                    totalDuration: Date.now() - startTime,
                    skillsCreated: 0,
                    lessonsLearned: 0,
                    correctionsApplied: 0,
                    dryRun: this.config.dryRun,
                    timestamp: Date.now(),
                };
            }
        }
        try {
            // Run requested phases
            if (phase === 'all' || phase === 'error') {
                phases.push(await this.runErrorAnalysis());
            }
            if (phase === 'all' || phase === 'skill') {
                phases.push(await this.runSkillCreation());
            }
            if (phase === 'all' || phase === 'memory') {
                phases.push(await this.runMemoryConsolidation());
            }
            if (phase === 'all' || phase === 'model') {
                phases.push(await this.runUserModelUpdate());
            }
            if (phase === 'all' || phase === 'correction') {
                phases.push(await this.runSelfCorrection());
            }
        }
        finally {
            // Restore config
            this.config.dryRun = originalDryRun;
            this.config.forceRun = originalForceRun;
            this.config.verbose = originalVerbose;
            // Update last run time (unless dry-run)
            if (!this.config.dryRun) {
                this.lastRunTime = Date.now();
                this.saveLastRun();
            }
        }
        const result = {
            phases,
            totalDuration: Date.now() - startTime,
            skillsCreated: phases.find((p) => p.phase === 'skill')?.itemsChanged ?? 0,
            lessonsLearned: phases.reduce((sum, p) => sum + p.details.length, 0),
            correctionsApplied: phases.find((p) => p.phase === 'correction')?.itemsChanged ?? 0,
            dryRun: this.config.dryRun,
            timestamp: Date.now(),
        };
        this.logResult(result);
        return result;
    }
    /**
     * Get formatted learnings/lessons log
     */
    getLearnings() {
        const lessonsPath = path.join(this.memoryDir, 'self-improvement', 'lessons.md');
        const entries = [];
        if (!(0, fs_1.existsSync)(lessonsPath)) {
            return entries;
        }
        try {
            const content = fs.readFileSync(lessonsPath, 'utf8');
            const sections = content.split(/^## /m).filter(Boolean);
            for (const section of sections) {
                const lines = section.trim().split('\n');
                const timestamp = lines[0]?.trim() || '';
                let category = 'insight';
                const lessons = [];
                const tags = [];
                for (const line of lines.slice(1)) {
                    const trimmed = line.trim();
                    if (trimmed.startsWith('### ')) {
                        const pattern = trimmed.slice(4).toLowerCase();
                        if (pattern.includes('error') || pattern.includes('wrong')) {
                            category = 'mistake';
                            tags.push('error');
                        }
                        else if (pattern.includes('pattern')) {
                            category = 'pattern';
                            tags.push('pattern');
                        }
                    }
                    if (trimmed.startsWith('- lesson:') || trimmed.startsWith('- ')) {
                        const lesson = trimmed.replace(/^-\s*(lesson:)?\s*/, '');
                        if (lesson)
                            lessons.push(lesson);
                    }
                }
                for (const lesson of lessons) {
                    entries.push({
                        id: `lesson:${entries.length}:${Date.now()}`,
                        timestamp: new Date(timestamp).getTime() || Date.now(),
                        category,
                        title: lesson.slice(0, 60),
                        description: lesson,
                        tags,
                        autoApplied: false,
                    });
                }
            }
        }
        catch {
            // Ignore
        }
        return entries.sort((a, b) => b.timestamp - a.timestamp);
    }
    /**
     * Show what would change without making changes
     */
    async dryRun() {
        return this.run('all', { dryRun: true });
    }
    /**
     * Show current configuration
     */
    getConfig() {
        return { ...this.config };
    }
    // ─── Phase 1: Error Analysis ──────────────────────────────────────────────
    async runErrorAnalysis() {
        const start = Date.now();
        const phase = {
            phase: 'error',
            success: false,
            duration: 0,
            itemsProcessed: 0,
            itemsChanged: 0,
            details: [],
            errors: [],
        };
        try {
            this.log('Phase 1: Analyzing errors and corrections...');
            const analyses = await this.analyzer.analyzeErrors();
            phase.itemsProcessed = analyses.length;
            if (analyses.length > 0) {
                if (!this.config.dryRun) {
                    this.analyzer.logLessons(analyses);
                }
                for (const a of analyses) {
                    phase.details.push(`[${a.pattern}] ${a.description}`);
                    if (this.config.verbose) {
                        this.log(`  • ${a.pattern}: ${a.fixSuggestion}`);
                    }
                }
                phase.itemsChanged = analyses.filter((a) => a.frequency > 1).length;
            }
            else {
                phase.details.push('No new error patterns detected');
            }
            phase.success = true;
        }
        catch (err) {
            phase.errors.push(err instanceof Error ? err.message : String(err));
            this.log(`Phase 1 error: ${phase.errors[0]}`, { error: true });
        }
        phase.duration = Date.now() - start;
        return phase;
    }
    // ─── Phase 2: Skill Creation ──────────────────────────────────────────────
    async runSkillCreation() {
        const start = Date.now();
        const phase = {
            phase: 'skill',
            success: false,
            duration: 0,
            itemsProcessed: 0,
            itemsChanged: 0,
            details: [],
            errors: [],
        };
        try {
            this.log('Phase 2: Creating skills from patterns...');
            // Gather session content for pattern detection
            const sessionContent = await this.gatherSessionContent();
            phase.itemsProcessed = sessionContent.length;
            // Detect patterns
            const templates = await this.skillCreator.detectPatterns(sessionContent);
            const eligible = templates.filter((t) => t.complexity >= 7 || t.frequency >= this.config.skillCreationThreshold);
            if (eligible.length === 0) {
                phase.details.push('No patterns meet skill creation threshold');
                phase.success = true;
                phase.duration = Date.now() - start;
                return phase;
            }
            const toCreate = eligible.slice(0, this.config.maxSkillsPerRun);
            if (this.config.dryRun) {
                phase.details.push(`Would create ${toCreate.length} skills:`);
                for (const t of toCreate) {
                    phase.details.push(`  • ${t.name}: ${t.description}`);
                }
            }
            else {
                const created = await this.skillCreator.createSkills(toCreate);
                phase.itemsChanged = created.length;
                phase.details.push(`Created ${created.length} skills:`);
                for (const c of created) {
                    phase.details.push(`  • ${c.name}`);
                }
            }
            phase.success = true;
        }
        catch (err) {
            phase.errors.push(err instanceof Error ? err.message : String(err));
            this.log(`Phase 2 error: ${phase.errors[0]}`, { error: true });
        }
        phase.duration = Date.now() - start;
        return phase;
    }
    // ─── Phase 3: Memory Consolidation ────────────────────────────────────────
    async runMemoryConsolidation() {
        const start = Date.now();
        const phase = {
            phase: 'memory',
            success: false,
            duration: 0,
            itemsProcessed: 0,
            itemsChanged: 0,
            details: [],
            errors: [],
        };
        try {
            this.log('Phase 3: Consolidating memory...');
            // Read recent daily memory files
            const memoryFiles = this.findRecentMemoryFiles();
            phase.itemsProcessed = memoryFiles.length;
            if (memoryFiles.length === 0) {
                phase.details.push('No recent memory files to consolidate');
                phase.success = true;
                phase.duration = Date.now() - start;
                return phase;
            }
            // In dry-run, just show what would be consolidated
            if (this.config.dryRun) {
                phase.details.push(`Would consolidate ${memoryFiles.length} memory files:`);
                for (const f of memoryFiles.slice(0, 5)) {
                    phase.details.push(`  • ${path.basename(f)}`);
                }
                if (memoryFiles.length > 5) {
                    phase.details.push(`  ... and ${memoryFiles.length - 5} more`);
                }
            }
            else {
                // Consolidate: merge insights into MEMORY.md
                const insights = await this.extractInsights(memoryFiles);
                if (insights.length > 0) {
                    this.mergeIntoMemory(insights);
                    phase.details.push(`Merged ${insights.length} insights into MEMORY.md`);
                    phase.itemsChanged = insights.length;
                }
                else {
                    phase.details.push('No new insights to merge');
                }
            }
            phase.success = true;
        }
        catch (err) {
            phase.errors.push(err instanceof Error ? err.message : String(err));
            this.log(`Phase 3 error: ${phase.errors[0]}`, { error: true });
        }
        phase.duration = Date.now() - start;
        return phase;
    }
    // ─── Phase 4: User Model Update ───────────────────────────────────────────
    async runUserModelUpdate() {
        const start = Date.now();
        const phase = {
            phase: 'model',
            success: false,
            duration: 0,
            itemsProcessed: 0,
            itemsChanged: 0,
            details: [],
            errors: [],
        };
        try {
            this.log('Phase 4: Updating user model...');
            // Analyze recent conversation patterns
            const patterns = await this.analyzeUserPatterns();
            phase.itemsProcessed = patterns.length;
            if (patterns.length === 0) {
                phase.details.push('No new user patterns detected');
            }
            else {
                if (this.config.dryRun) {
                    phase.details.push(`Would update user model with ${patterns.length} changes:`);
                    for (const p of patterns) {
                        phase.details.push(`  • ${p}`);
                    }
                }
                else {
                    // Update user-modeling.md
                    this.updateUserModelingMd(patterns);
                    phase.details.push(`Applied ${patterns.length} user model updates`);
                    phase.itemsChanged = patterns.length;
                }
            }
            phase.success = true;
        }
        catch (err) {
            phase.errors.push(err instanceof Error ? err.message : String(err));
            this.log(`Phase 4 error: ${phase.errors[0]}`, { error: true });
        }
        phase.duration = Date.now() - start;
        return phase;
    }
    // ─── Phase 5: Self-Correction ─────────────────────────────────────────────
    async runSelfCorrection() {
        const start = Date.now();
        const phase = {
            phase: 'correction',
            success: false,
            duration: 0,
            itemsProcessed: 0,
            itemsChanged: 0,
            details: [],
            errors: [],
        };
        try {
            this.log('Phase 5: Self-correction...');
            // Load recent analyses to determine corrections
            const analyses = await this.analyzer.analyzeErrors();
            const corrections = this.determineCorrections(analyses);
            phase.itemsProcessed = corrections.length;
            if (corrections.length === 0) {
                phase.details.push('No corrections needed');
            }
            else {
                if (this.config.dryRun) {
                    phase.details.push(`Would apply ${corrections.length} self-corrections:`);
                    for (const c of corrections) {
                        phase.details.push(`  • [${c.area}] ${c.whatWillChange}`);
                    }
                }
                else {
                    const applied = this.applyCorrections(corrections);
                    phase.itemsChanged = applied;
                    phase.details.push(`Applied ${applied} self-corrections`);
                }
            }
            phase.success = true;
        }
        catch (err) {
            phase.errors.push(err instanceof Error ? err.message : String(err));
            this.log(`Phase 5 error: ${phase.errors[0]}`, { error: true });
        }
        phase.duration = Date.now() - start;
        return phase;
    }
    // ─── Helper Methods ───────────────────────────────────────────────────────
    log(msg, opts = {}) {
        if (opts.error) {
            this.ctx.logger?.error(`[self-improving] ${msg}`);
        }
        else {
            this.ctx.logger?.info(`[self-improving] ${msg}`);
        }
        // Also print to stdout in verbose mode
        if (this.config.verbose) {
            process.stdout.write(`[self-improving] ${msg}\n`);
        }
    }
    logResult(result) {
        const status = result.dryRun ? '[DRY-RUN] ' : '';
        const duration = (result.totalDuration / 1000).toFixed(1);
        this.log(`${status}Self-improving loop complete (${duration}s)`);
        this.log(`  Skills created: ${result.skillsCreated}`);
        this.log(`  Lessons learned: ${result.lessonsLearned}`);
        this.log(`  Corrections applied: ${result.correctionsApplied}`);
        for (const phase of result.phases) {
            const icon = phase.success ? '✓' : '✗';
            const changed = phase.itemsChanged > 0 ? ` (${phase.itemsChanged} changed)` : '';
            this.log(`  ${icon} ${phase.phase}: ${phase.itemsProcessed} processed${changed}`);
        }
    }
    gatherSessionContent() {
        return new Promise((resolve) => {
            const content = [];
            if (!(0, fs_1.existsSync)(this.sessionsDbPath)) {
                resolve(content);
                return;
            }
            try {
                const Database = require('better-sqlite3');
                const db = new Database(this.sessionsDbPath, { readonly: true });
                const cutoff = Date.now() - 7 * 24 * 60 * 60 * 1000; // last 7 days
                const messages = db
                    .prepare(`SELECT content FROM messages WHERE timestamp >= ? ORDER BY timestamp DESC LIMIT 200`)
                    .all(cutoff);
                for (const m of messages) {
                    if (m.content)
                        content.push(m.content);
                }
                db.close();
            }
            catch {
                // Ignore errors
            }
            resolve(content);
        });
    }
    findRecentMemoryFiles() {
        const memoryFiles = [];
        const memoryBase = path.join(this.memoryDir);
        if (!(0, fs_1.existsSync)(memoryBase))
            return memoryFiles;
        try {
            const entries = fs.readdirSync(memoryBase, { withFileTypes: true });
            const sevenDaysAgo = Date.now() - 7 * 24 * 60 * 60 * 1000;
            for (const entry of entries) {
                if (entry.isFile() && entry.name.endsWith('.md')) {
                    const fullPath = path.join(memoryBase, entry.name);
                    const stat = fs.statSync(fullPath);
                    if (stat.mtimeMs > sevenDaysAgo) {
                        memoryFiles.push(fullPath);
                    }
                }
            }
        }
        catch {
            // Ignore
        }
        return memoryFiles.sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs);
    }
    async extractInsights(files) {
        const insights = [];
        for (const file of files) {
            try {
                const content = fs.readFileSync(file, 'utf8');
                // Extract lines that look like insights (bullet points, conclusions, etc.)
                const lines = content.split('\n');
                for (const line of lines) {
                    const trimmed = line.trim();
                    if (trimmed.startsWith('- ') &&
                        !trimmed.startsWith('- lesson:') &&
                        trimmed.length > 20 &&
                        trimmed.length < 200) {
                        insights.push(trimmed.slice(2));
                    }
                }
            }
            catch {
                // Ignore
            }
        }
        return [...new Set(insights)].slice(0, 10); // Dedupe and limit
    }
    mergeIntoMemory(insights) {
        const memoryPath = path.join(this.workDir, 'MEMORY.md');
        if (!(0, fs_1.existsSync)(memoryPath))
            return;
        try {
            let content = fs.readFileSync(memoryPath, 'utf8');
            const timestamp = new Date().toISOString().slice(0, 10);
            // Find a good insertion point (before any trailing comments or at end)
            const insertSection = `\n\n## Consolidated Insights (${timestamp})\n`;
            // Find last "##" section
            const lastSection = content.lastIndexOf('\n## ');
            if (lastSection !== -1) {
                content = content.slice(0, lastSection) + insertSection + content.slice(lastSection);
            }
            else {
                content += insertSection;
            }
            for (const insight of insights) {
                content += `- ${insight}\n`;
            }
            fs.writeFileSync(memoryPath, content, 'utf8');
        }
        catch {
            // Ignore
        }
    }
    async analyzeUserPatterns() {
        const patterns = [];
        if (!(0, fs_1.existsSync)(this.sessionsDbPath))
            return patterns;
        try {
            const Database = require('better-sqlite3');
            const db = new Database(this.sessionsDbPath, { readonly: true });
            const cutoff = Date.now() - 7 * 24 * 60 * 60 * 1000;
            const messages = db
                .prepare(`SELECT content, role FROM messages WHERE timestamp >= ? ORDER BY timestamp DESC LIMIT 100`)
                .all(cutoff);
            // Analyze message length patterns
            const userMessages = messages.filter((m) => m.role === 'user' && m.content);
            if (userMessages.length > 0) {
                const avgLength = userMessages.reduce((sum, m) => sum + (m.content?.length ?? 0), 0) /
                    userMessages.length;
                if (avgLength < 50) {
                    patterns.push('User prefers brief messages (avg < 50 chars)');
                }
                else if (avgLength > 300) {
                    patterns.push('User writes detailed messages (avg > 300 chars)');
                }
            }
            // Check for questions
            const questionCount = userMessages.filter((m) => (m.content ?? '').includes('?')).length;
            if (questionCount > userMessages.length * 0.5) {
                patterns.push('User frequently asks questions (>50% of messages)');
            }
            db.close();
        }
        catch {
            // Ignore
        }
        return patterns;
    }
    updateUserModelingMd(patterns) {
        const modelingPath = path.join(this.memoryDir, 'user-modeling.md');
        try {
            let content = (0, fs_1.existsSync)(modelingPath) ? fs.readFileSync(modelingPath, 'utf8') : '';
            const timestamp = new Date().toISOString().slice(0, 10);
            content += `\n## Inferred Patterns (${timestamp})\n`;
            for (const p of patterns) {
                content += `- ${p}\n`;
            }
            fs.writeFileSync(modelingPath, content, 'utf8');
        }
        catch {
            // Ignore
        }
    }
    determineCorrections(analyses) {
        const corrections = [];
        for (const a of analyses) {
            if (a.frequency >= 2 && a.lessons.length > 0) {
                corrections.push({
                    area: this.patternToArea(a.pattern),
                    whatWentWrong: a.rootCause,
                    whatWillChange: a.lessons[0] ?? a.fixSuggestion,
                });
            }
        }
        return corrections;
    }
    patternToArea(pattern) {
        const map = {
            output_formatting: 'response_strategy',
            wrong_answer: 'response_strategy',
            response_length: 'response_strategy',
            clarification_needed: 'communication',
            code_quality: 'skills',
            incomplete_response: 'response_strategy',
        };
        return map[pattern] ?? 'general';
    }
    applyCorrections(corrections) {
        let applied = 0;
        for (const c of corrections) {
            if (c.area === 'response_strategy') {
                // Log to self-correction log for review
                this.logSelfCorrection(c);
                applied++;
            }
        }
        return applied;
    }
    logSelfCorrection(correction) {
        const logPath = path.join(this.memoryDir, 'self-improvement', 'self-corrections.md');
        const timestamp = new Date().toISOString();
        const entry = `\n## ${timestamp}\n- Area: ${correction.area}\n- What went wrong: ${correction.whatWentWrong}\n- What will change: ${correction.whatWillChange}\n`;
        try {
            fs.appendFileSync(logPath, entry, 'utf8');
        }
        catch {
            // Ignore
        }
    }
    // ─── Config Management ───────────────────────────────────────────────────
    loadConfig() {
        const configPath = path.join(this.workDir, CONFIG_FILE);
        if ((0, fs_1.existsSync)(configPath)) {
            try {
                const loaded = JSON.parse(fs.readFileSync(configPath, 'utf8'));
                this.config = { ...DEFAULT_CONFIG, ...loaded };
            }
            catch {
                this.config = { ...DEFAULT_CONFIG };
            }
        }
    }
    saveConfig() {
        const configPath = path.join(this.workDir, CONFIG_FILE);
        try {
            fs.writeFileSync(configPath, JSON.stringify(this.config, null, 2), 'utf8');
        }
        catch {
            // Ignore
        }
    }
    loadLastRun() {
        const lastRunPath = path.join(this.memoryDir, LAST_RUN_FILE);
        if ((0, fs_1.existsSync)(lastRunPath)) {
            try {
                this.lastRunTime = parseInt(fs.readFileSync(lastRunPath, 'utf8').trim(), 10);
            }
            catch {
                this.lastRunTime = 0;
            }
        }
    }
    saveLastRun() {
        const lastRunPath = path.join(this.memoryDir, LAST_RUN_FILE);
        try {
            fs.writeFileSync(lastRunPath, String(this.lastRunTime), 'utf8');
        }
        catch {
            // Ignore
        }
    }
}
// ─── Singleton & Export ──────────────────────────────────────────────────────
const instance = new SelfImprovingLoop();
exports.default = instance;
//# sourceMappingURL=index.js.map
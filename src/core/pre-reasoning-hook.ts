/**
 * Pre-Reasoning Hook (ReMe Pattern)
 *
 * Called before each reasoning step to compact tool results and prevent context overflow.
 * Extracts goal, progress, decisions, next steps, and critical context from tool results.
 */

export interface ReasoningContext {
  goal: string;
  progress: string;
  decisions: string[];
  nextSteps: string;
  criticalContext: string;
}

const GOAL_PATTERNS = [
  /goal[:\s]+(.+)/i,
  /intention[:\s]+(.+)/i,
  /objective[:\s]+(.+)/i,
  /^→\s*(.+)/,
];

const DECISION_PATTERNS = [
  /decided[:\s]+(.+)/i,
  /choice[:\s]+(.+)/i,
  /chose[:\s]+(.+)/i,
  /selected[:\s]+(.+)/i,
  /\[DECISION\]\s*(.+)/i,
];

const NEXT_PATTERNS = [
  /next[:\s]+(.+)/i,
  /接下来[:\s]+(.+)/i,
  /will\s+(?:do|try|attempt|check)/i,
  /\/\/\s*TODO[:\s]*(.+)/i,
  /^→\s*.+\s*→/,
];

const CRITICAL_PATTERNS = [
  /critical[:\s]+(.+)/i,
  /important[:\s]+(.+)/i,
  /warning[:\s]+(.+)/i,
  /error[:\s]+(.+)/i,
  /fail(ed)?[:\s]+(.+)/i,
];

/**
 * Extract the primary goal from tool results
 */
function extractGoal(toolResults: string[]): string {
  for (const result of toolResults) {
    for (const pattern of GOAL_PATTERNS) {
      const match = result.match(pattern);
      if (match && match[1]) {
        return match[1].trim().slice(0, 200);
      }
    }
  }
  return 'Unspecified';
}

/**
 * Extract decisions made from tool results
 */
function extractDecisions(toolResults: string[]): string[] {
  const decisions: string[] = [];
  for (const result of toolResults) {
    for (const pattern of DECISION_PATTERNS) {
      const match = result.match(pattern);
      if (match && match[1]) {
        const decision = match[1].trim();
        if (decision && decision.length > 2 && decision.length < 300) {
          decisions.push(decision);
        }
      }
    }
  }
  return [...new Set(decisions)].slice(0, 10);
}

/**
 * Infer next steps from tool results
 */
function inferNextSteps(toolResults: string[]): string {
  for (const result of toolResults) {
    for (const pattern of NEXT_PATTERNS) {
      const match = result.match(pattern);
      if (match && match[1]) {
        return match[1].trim().slice(0, 300);
      }
    }
    // Look for TODO comments
    const todoMatch = result.match(/\/\/\s*TODO[:\s]*(.+)/);
    if (todoMatch && todoMatch[1]) {
      return todoMatch[1].trim().slice(0, 300);
    }
  }
  return 'Continue with main task flow';
}

/**
 * Extract critical context (errors, warnings, important notes)
 */
function extractCritical(toolResults: string[]): string {
  const critical: string[] = [];
  for (const result of toolResults) {
    for (const pattern of CRITICAL_PATTERNS) {
      const match = result.match(pattern);
      if (match && match[1]) {
        critical.push(match[1].trim().slice(0, 150));
      }
    }
    // Look for error-like patterns
    const errorMatch = result.match(/error[:\s]+(.+)/i);
    if (errorMatch && errorMatch[1] && !critical.includes(errorMatch[1])) {
      critical.push(`ERROR: ${errorMatch[1]}`.trim().slice(0, 140));
    }
  }
  return critical.slice(0, 5).join(' | ') || 'None';
}

/**
 * Compact progress into a summary within token budget
 */
function compactProgress(toolResults: string[], maxTokens: number): string {
  const avgCharsPerToken = 4;
  const maxChars = maxTokens * avgCharsPerToken;

  if (toolResults.length === 0) {
    return 'No tool results available';
  }

  // Join all results with separator
  const combined = toolResults.join('\n---\n');

  if (combined.length <= maxChars) {
    return combined;
  }

  // Summarize each result and concatenate until we hit budget
  const summaries: string[] = [];
  let currentLength = 0;

  for (const result of toolResults) {
    const lines = result.split('\n').filter((l) => l.trim());
    let summary = '';

    // Take first and last meaningful lines as summary
    if (lines.length > 4) {
      const first = lines[0] ?? '';
      const last = lines[lines.length - 1] ?? '';
      summary = `${first.slice(0, 100)}...${last.slice(0, 100)}`;
    } else {
      const first = lines[0] ?? '';
      const second = lines[1] ?? '';
      summary = `${first} ${second}`.slice(0, 150);
    }

    if (currentLength + summary.length > maxChars) {
      break;
    }
    summaries.push(summary);
    currentLength += summary.length + 20;
  }

  const compact = summaries.join(' | ');
  if (compact.length > combined.length) {
    return combined.slice(0, maxChars);
  }
  return compact;
}

/**
 * Pre-Reasoning Hook - called before each reasoning step
 * Compacts tool results to prevent context overflow
 *
 * @param toolResults - Array of tool result strings
 * @param maxTokens - Maximum tokens for progress summary (default 500)
 * @returns ReasoningContext with goal, progress, decisions, next steps, critical context
 */
export function preReasoningHook(
  toolResults: string[],
  maxTokens: number = 500,
): ReasoningContext {
  return {
    goal: extractGoal(toolResults),
    progress: compactProgress(toolResults, maxTokens),
    decisions: extractDecisions(toolResults),
    nextSteps: inferNextSteps(toolResults),
    criticalContext: extractCritical(toolResults),
  };
}

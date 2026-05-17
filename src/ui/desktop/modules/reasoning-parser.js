const REASONING_TAG_PATTERN = /<(think|reasoning)>/gi;

function normalizeVisibleText(text) {
  return String(text || '').replace(/\n{3,}/g, '\n\n').trim();
}

function normalizeReasoningText(parts) {
  return parts
    .map((part) => part.trim())
    .filter(Boolean)
    .join('\n\n')
    .trim();
}

export function parseReasoningBlocks(source) {
  const rawBody = String(source || '');
  const reasoning = [];
  let visible = '';
  let index = 0;

  REASONING_TAG_PATTERN.lastIndex = 0;
  while (index < rawBody.length) {
    REASONING_TAG_PATTERN.lastIndex = index;
    const openMatch = REASONING_TAG_PATTERN.exec(rawBody);
    if (!openMatch) {
      visible += rawBody.slice(index);
      break;
    }

    const tag = String(openMatch[1] || '').toLowerCase();
    visible += rawBody.slice(index, openMatch.index);

    const contentStart = REASONING_TAG_PATTERN.lastIndex;
    const closeToken = `</${tag}>`;
    const closeIndex = rawBody.toLowerCase().indexOf(closeToken, contentStart);

    if (closeIndex === -1) {
      const partialReasoning = rawBody.slice(contentStart);
      if (partialReasoning.trim()) reasoning.push(`${tag}: ${partialReasoning.trim()}`);
      index = rawBody.length;
      break;
    }

    const content = rawBody.slice(contentStart, closeIndex).trim();
    if (content) reasoning.push(`${tag}: ${content}`);
    index = closeIndex + closeToken.length;
  }

  return {
    rawBody,
    body: normalizeVisibleText(visible),
    reasoning: normalizeReasoningText(reasoning),
  };
}

export function createReasoningStreamParser() {
  let rawBody = '';

  return {
    push(chunk) {
      rawBody += String(chunk || '');
      return parseReasoningBlocks(rawBody);
    },
    snapshot() {
      return parseReasoningBlocks(rawBody);
    },
    reset() {
      rawBody = '';
      return parseReasoningBlocks(rawBody);
    },
  };
}

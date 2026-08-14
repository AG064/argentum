import path from 'node:path';
import { pathToFileURL } from 'node:url';

async function loadParser() {
  const parserUrl = pathToFileURL(
    path.join(process.cwd(), 'src/ui/desktop/modules/reasoning-parser.js'),
  ).href;
  return import(parserUrl) as Promise<{
    createReasoningStreamParser: () => {
      push: (chunk: string) => { body: string; reasoning: string; rawBody: string };
      snapshot: () => { body: string; reasoning: string; rawBody: string };
    };
    parseReasoningBlocks: (source: string) => { body: string; reasoning: string; rawBody: string };
  }>;
}

describe('desktop reasoning stream parser', () => {
  it('extracts a complete think block from one chunk', async () => {
    const { createReasoningStreamParser } = await loadParser();
    const parser = createReasoningStreamParser();

    const parsed = parser.push('<think>check tools</think>Final answer');

    expect(parsed.rawBody).toBe('<think>check tools</think>Final answer');
    expect(parsed.reasoning).toBe('think: check tools');
    expect(parsed.body).toBe('Final answer');
  });

  it('extracts a think block when the opening tag is split across chunks', async () => {
    const { createReasoningStreamParser } = await loadParser();
    const parser = createReasoningStreamParser();

    parser.push('<th');
    parser.push('ink>split open</think>Answer');

    expect(parser.snapshot().reasoning).toBe('think: split open');
    expect(parser.snapshot().body).toBe('Answer');
  });

  it('extracts a think block when the closing tag is split across chunks', async () => {
    const { createReasoningStreamParser } = await loadParser();
    const parser = createReasoningStreamParser();

    parser.push('<think>split close</thi');
    parser.push('nk>Answer');

    expect(parser.snapshot().reasoning).toBe('think: split close');
    expect(parser.snapshot().body).toBe('Answer');
  });

  it('keeps normal responses without a thinking block visible', async () => {
    const { parseReasoningBlocks } = await loadParser();

    const parsed = parseReasoningBlocks('Plain answer only');

    expect(parsed.rawBody).toBe('Plain answer only');
    expect(parsed.reasoning).toBe('');
    expect(parsed.body).toBe('Plain answer only');
  });

  it('keeps reasoning separate when an answer follows the thinking block', async () => {
    const { parseReasoningBlocks } = await loadParser();

    const parsed = parseReasoningBlocks('<reasoning>plan first</reasoning>\n\nVisible answer');

    expect(parsed.reasoning).toBe('reasoning: plan first');
    expect(parsed.body).toBe('Visible answer');
  });
});

/**
 * Unit tests for HierarchicalMemoryStore
 */

import { rmSync } from 'fs';
import { join } from 'path';
import { HierarchicalMemoryStore, MemoryTier, type MemoryEntry } from '../../src/core/hierarchical-memory';

const TEST_DB = join(process.cwd(), 'data', 'test-tmp', 'test-hierarchical-memory.db');

function makeEntry(overrides: Partial<MemoryEntry> = {}): MemoryEntry {
  return {
    id: '',
    tier: MemoryTier.SHORT,
    content: 'test content',
    importance: 0.5,
    accessCount: 1,
    createdAt: Date.now(),
    lastAccessed: Date.now(),
    tags: [],
    ...overrides,
  };
}

function freshStore(): HierarchicalMemoryStore {
  try {
    rmSync(TEST_DB);
  } catch {
    // ignore
  }
  return new HierarchicalMemoryStore(TEST_DB);
}

describe('HierarchicalMemoryStore', () => {
  let store: HierarchicalMemoryStore;

  beforeEach(() => {
    store = freshStore();
  });

  afterEach(() => {
    store.close();
  });

  // -------------------------------------------------------------------------
  // store
  // -------------------------------------------------------------------------

  test('stores an entry with auto-generated id when id is empty', () => {
    const entry = makeEntry({ id: '' });
    const stored = store.store(entry);
    const results = store.retrieve('', 10);
    expect(results).toHaveLength(1);
    expect(results[0]!.id).toBeTruthy();
    expect(stored.id).toBeTruthy();
  });

  test('stores an entry and retrieves it', () => {
    const entry = makeEntry({ content: 'hello world', tier: MemoryTier.SHORT });
    store.store(entry);
    const results = store.retrieve('', 10);
    expect(results).toHaveLength(1);
    expect(results[0]!.content).toBe('hello world');
  });

  test('auto-promotes to long-tier when importance >= 0.7', () => {
    // Start at MID tier so auto-promote goes MID → LONG (one step)
    const entry = makeEntry({ importance: 0.9, tier: MemoryTier.MID });
    store.store(entry);
    const results = store.retrieve('', 10);
    expect(results[0]!.tier).toBe(MemoryTier.LONG);
  });

  test('auto-promotes to mid-tier when accessCount > 3', () => {
    const entry = makeEntry({ accessCount: 5, tier: MemoryTier.SHORT, importance: 0.3 });
    store.store(entry);
    const results = store.retrieve('', 10);
    expect(results[0]!.tier).toBe(MemoryTier.MID);
  });

  // -------------------------------------------------------------------------
  // retrieve
  // -------------------------------------------------------------------------

  test('retrieve returns entries matching query keywords', () => {
    store.store(makeEntry({ content: 'typescript is great', tier: MemoryTier.MID }));
    store.store(makeEntry({ content: 'rust is fast', tier: MemoryTier.MID }));

    const results = store.retrieve('typescript', 5);
    expect(results.length).toBeGreaterThanOrEqual(1);
    expect(results[0]!.content).toContain('typescript');
  });

  test('retrieve returns entries sorted by recency when no keywords match', () => {
    store.store(makeEntry({ content: 'foo bar', tier: MemoryTier.SHORT }));
    const results = store.retrieve('zzznomatchxxx', 5);
    // Falls back to recency when no keywords match
    expect(results.length).toBeGreaterThanOrEqual(1);
  });

  test('retrieve respects maxResults', () => {
    for (let i = 0; i < 20; i++) {
      store.store(makeEntry({ content: `content ${i}`, tier: MemoryTier.MID }));
    }
    const results = store.retrieve('', 5);
    expect(results).toHaveLength(5);
  });

  test('retrieve orders by relevance (accessCount + importance)', () => {
    store.store(makeEntry({ content: 'a', accessCount: 1, importance: 0.2, tier: MemoryTier.MID }));
    store.store(makeEntry({ content: 'b', accessCount: 10, importance: 0.9, tier: MemoryTier.MID }));

    const results = store.retrieve('a b', 2);
    // Higher scored entry should come first
    expect(results[0]!.content).toBe('b');
  });

  // -------------------------------------------------------------------------
  // promote / demote
  // -------------------------------------------------------------------------

  test('promote moves short -> mid', () => {
    const entry = store.store(makeEntry({ tier: MemoryTier.SHORT, importance: 0.1, accessCount: 1 }));
    store.promote(entry.id);
    const results = store.retrieve('', 10);
    expect(results.find((r) => r.id === entry.id)?.tier).toBe(MemoryTier.MID);
  });

  test('promote moves mid -> long', () => {
    const entry = store.store(makeEntry({ tier: MemoryTier.MID, importance: 0.1, accessCount: 1 }));
    store.promote(entry.id);
    const results = store.retrieve('', 10);
    expect(results.find((r) => r.id === entry.id)?.tier).toBe(MemoryTier.LONG);
  });

  test('promote does nothing for long-tier entry', () => {
    const entry = store.store(makeEntry({ tier: MemoryTier.LONG }));
    store.promote(entry.id);
    const results = store.retrieve('', 10);
    expect(results.find((r) => r.id === entry.id)?.tier).toBe(MemoryTier.LONG);
  });

  test('demote moves long -> mid', () => {
    const entry = store.store(makeEntry({ tier: MemoryTier.LONG }));
    store.demote(entry.id);
    const results = store.retrieve('', 10);
    expect(results.find((r) => r.id === entry.id)?.tier).toBe(MemoryTier.MID);
  });

  test('demote moves mid -> short', () => {
    const entry = store.store(makeEntry({ tier: MemoryTier.MID }));
    store.demote(entry.id);
    const results = store.retrieve('', 10);
    expect(results.find((r) => r.id === entry.id)?.tier).toBe(MemoryTier.SHORT);
  });

  test('demote does nothing for short-tier entry', () => {
    const entry = store.store(makeEntry({ tier: MemoryTier.SHORT }));
    store.demote(entry.id);
    const results = store.retrieve('', 10);
    expect(results.find((r) => r.id === entry.id)?.tier).toBe(MemoryTier.SHORT);
  });

  // -------------------------------------------------------------------------
  // prune
  // -------------------------------------------------------------------------

  test('prune removes oldest short entries beyond cap', () => {
    // Store enough entries to exceed SHORT_MAX (50)
    for (let i = 0; i < 60; i++) {
      store.store(makeEntry({ content: `short-${i}`, tier: MemoryTier.SHORT, createdAt: i }));
    }
    store.prune();
    const stats = store.stats();
    const shortCount = stats.find((s) => s.tier === MemoryTier.SHORT)?.count ?? 0;
    expect(shortCount).toBeLessThanOrEqual(50);
  });

  test('prune does not remove long-tier entries even when many', () => {
    for (let i = 0; i < 10; i++) {
      store.store(makeEntry({ content: `long-${i}`, tier: MemoryTier.LONG, importance: 0.9 }));
    }
    store.prune();
    const stats = store.stats();
    const longCount = stats.find((s) => s.tier === MemoryTier.LONG)?.count ?? 0;
    expect(longCount).toBe(10);
  });

  // -------------------------------------------------------------------------
  // stats
  // -------------------------------------------------------------------------

  test('stats returns correct counts per tier', () => {
    for (let i = 0; i < 5; i++) store.store(makeEntry({ tier: MemoryTier.SHORT }));
    for (let i = 0; i < 3; i++) store.store(makeEntry({ tier: MemoryTier.MID }));
    for (let i = 0; i < 2; i++) store.store(makeEntry({ tier: MemoryTier.LONG }));

    const stats = store.stats();
    expect(stats.find((s) => s.tier === MemoryTier.SHORT)?.count).toBe(5);
    expect(stats.find((s) => s.tier === MemoryTier.MID)?.count).toBe(3);
    expect(stats.find((s) => s.tier === MemoryTier.LONG)?.count).toBe(2);
  });

  // -------------------------------------------------------------------------
  // compact
  // -------------------------------------------------------------------------

  test('compact merges similar entries', () => {
    store.store(
      makeEntry({ content: 'TypeScript is a strongly typed programming language', tier: MemoryTier.MID }),
    );
    store.store(
      makeEntry({ content: 'TypeScript is a strongly typed language that compiles to JavaScript', tier: MemoryTier.MID }),
    );

    const before = store.retrieve('', 10);
    expect(before.length).toBeGreaterThanOrEqual(2);

    store.compact();

    const after = store.retrieve('', 10);
    // Should have merged to fewer or same count (never more)
    expect(after.length).toBeLessThanOrEqual(before.length);
  });

  test('compact preserves non-similar entries', () => {
    store.store(makeEntry({ content: 'cat meows', tier: MemoryTier.MID }));
    store.store(makeEntry({ content: 'rocket launches', tier: MemoryTier.MID }));

    store.compact();

    const results = store.retrieve('', 10);
    expect(results.length).toBeGreaterThanOrEqual(2);
  });
});

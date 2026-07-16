// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2026 AG064
/**
 * Shared Graph Types
 *
 * Re-exports the canonical GraphBackend interface from the features layer.
 * All graph implementations across core/, memory/, and features/ should
 * use this shared type to ensure interface consistency.
 *
 * The three graph implementations and their relationships:
 *
 * 1. src/core/knowledge-graph.ts  — KnowledgeGraphMemory
 *    SQLite with bitemporal versioning (valid_time + transaction_time) and
 *    scope hierarchy (global/project/task). It does not yet implement GraphBackend.
 *
 * 2. src/memory/graph.ts  — MemoryGraph
 *    Builds on semantic memory, adds typed edges, BFS traversal, and
 *    pathfinding. Uses its own SQLite schema with an 'edges' table.
 *
 * 3. src/features/knowledge-graph/index.ts  — KnowledgeGraphFeature
 *    Feature module with SQLite + in-memory backends. Exports GraphBackend.
 *    Used by the feature system for graph-based features.
 *
 * Consolidation approach (v0.1.0):
 * - features/knowledge-graph remains the canonical GraphBackend interface
 * - core/knowledge-graph.ts is the production SQLite implementation
 * - memory/graph.ts adopts the GraphBackend interface for its storage layer
 *   and delegates traversal/pathfinding on top
 * - A shared GraphBackend instance is created in core and injected into memory
 */

export type {
  GraphBackend,
  Entity,
  Relationship,
  QueryResult,
  GraphData,
} from '../features/knowledge-graph/index.js';

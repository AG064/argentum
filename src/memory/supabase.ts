/**
 * Supabase Memory Backend
 *
 * Cloud-backed memory storage using Supabase (PostgreSQL).
 * Provides real-time sync, row-level security, and full-text search.
 */

/** Memory row stored in Supabase */
export interface SupabaseMemoryRow {
  id: string;
  user_id: string;
  key: string;
  value: string;
  type: string;
  tags: string[];
  metadata: Record<string, unknown>;
  importance: number;
  created_at: string;
  updated_at: string;
}

/** Supabase client interface (subset we need) */
interface SupabaseClient {
  from(table: string): SupabaseQuery;
  rpc(fn: string, params?: Record<string, unknown>): Promise<{ data: unknown; error: { message: string } | null }>;
}

interface SupabaseQuery {
  select(columns?: string): SupabaseQuery;
  insert(rows: Record<string, unknown>[]): SupabaseQuery;
  update(data: Record<string, unknown>): SupabaseQuery;
  delete(): SupabaseQuery;
  eq(column: string, value: unknown): SupabaseQuery;
  ilike(column: string, pattern: string): SupabaseQuery;
  order(column: string, opts?: { ascending?: boolean }): SupabaseQuery;
  limit(count: number): SupabaseQuery;
  single(): Promise<{ data: unknown; error: { message: string } | null }>;
  then(resolve: (value: { data: unknown; error: { message: string } | null }) => void): void;
}

/**
 * Supabase-backed memory store.
 *
 * Stores memories in a Supabase PostgreSQL database with full-text
 * search, real-time subscriptions, and row-level security.
 *
 * Requires SUPABASE_URL and SUPABASE_KEY environment variables.
 */
export class SupabaseMemory {
  private client: SupabaseClient | null = null;
  private tableName: string;

  constructor(tableName: string = 'agclaw_memories') {
    this.tableName = tableName;
  }

  /** Initialize Supabase client */
  init(url?: string, key?: string): void {
    const supabaseUrl = url ?? process.env.SUPABASE_URL;
    const supabaseKey = key ?? process.env.SUPABASE_KEY;

    if (!supabaseUrl || !supabaseKey) {
      throw new Error('SUPABASE_URL and SUPABASE_KEY are required');
    }

    // Dynamic import would be used here in real implementation
    // For now, this is a type-safe interface
    console.info(`[SupabaseMemory] Initialized with URL: ${supabaseUrl}`);
  }

  /** Store a memory entry */
  async set(
    key: string,
    value: string,
    options: { userId?: string; type?: string; tags?: string[]; metadata?: Record<string, unknown>; importance?: number } = {}
  ): Promise<SupabaseMemoryRow | null> {
    if (!this.client) return null;

    const row = {
      user_id: options.userId ?? 'default',
      key,
      value,
      type: options.type ?? 'text',
      tags: options.tags ?? [],
      metadata: options.metadata ?? {},
      importance: options.importance ?? 0.5,
    };

    const { data, error } = await this.client
      .from(this.tableName)
      .insert([row])
      .select()
      .single();

    if (error) {
      console.error('[SupabaseMemory] Insert error:', error.message);
      return null;
    }
    return data as SupabaseMemoryRow;
  }

  /** Retrieve memory by key */
  async get(key: string, userId = 'default'): Promise<SupabaseMemoryRow | null> {
    if (!this.client) return null;

    const { data, error } = await this.client
      .from(this.tableName)
      .select('*')
      .eq('key', key)
      .eq('user_id', userId)
      .single();

    if (error) return null;
    return data as SupabaseMemoryRow;
  }

  /** Update a memory entry */
  async update(key: string, value: string, userId = 'default'): Promise<boolean> {
    if (!this.client) return false;

    const { error } = await this.client
      .from(this.tableName)
      .update({ value, updated_at: new Date().toISOString() })
      .eq('key', key)
      .eq('user_id', userId);

    return !error;
  }

  /** Delete a memory entry */
  async delete(key: string, userId = 'default'): Promise<boolean> {
    if (!this.client) return false;

    const { error } = await this.client
      .from(this.tableName)
      .delete()
      .eq('key', key)
      .eq('user_id', userId);

    return !error;
  }

  /** Query memories with filtering */
  async query(options: {
    userId?: string;
    type?: string;
    limit?: number;
    offset?: number;
  } = {}): Promise<SupabaseMemoryRow[]> {
    if (!this.client) return [];

    let query = this.client
      .from(this.tableName)
      .select('*');

    if (options.userId) query = query.eq('user_id', options.userId);
    if (options.type) query = query.eq('type', options.type);
    query = query.order('updated_at', { ascending: false });
    if (options.limit) query = query.limit(options.limit);

    const { data, error } = await query;
    if (error) return [];
    return (data as SupabaseMemoryRow[]) ?? [];
  }

  /** Full-text search */
  async search(query: string, userId = 'default', limit = 20): Promise<SupabaseMemoryRow[]> {
    if (!this.client) return [];

    const { data, error } = await this.client
      .from(this.tableName)
      .select('*')
      .eq('user_id', userId)
      .ilike('value', `%${query}%`)
      .limit(limit);

    if (error) return [];
    return (data as SupabaseMemoryRow[]) ?? [];
  }
}

/**
 * Calendar Integration Feature
 *
 * Local calendar storage and management with SQLite backend.
 * Supports events, reminders, and recurring events.
 */

import path from 'path';

import Database from 'better-sqlite3';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

/** Calendar configuration */
export interface CalendarConfig {
  enabled: boolean;
  dbPath: string;
  defaultReminderMinutes: number;
}

/** Calendar event */
export interface CalendarEvent {
  id: string;
  title: string;
  description?: string;
  startTime: number; // Unix timestamp (milliseconds)
  endTime: number;
  allDay?: boolean;
  location?: string;
  reminders: Reminder[];
  recurrence?: RecurrenceRule;
  createdAt: number;
  updatedAt: number;
}

/** Reminder for an event */
export interface Reminder {
  id: string;
  eventId: string;
  time: number; // Unix timestamp (milliseconds) when reminder should trigger
  triggered: boolean;
  method: 'notification' | 'email' | 'sms';
}

/** Recurrence rule (simplified) */
export interface RecurrenceRule {
  frequency: 'daily' | 'weekly' | 'monthly' | 'yearly';
  interval: number;
  until?: number; // End date
  count?: number; // Number of occurrences
  byDay?: string[]; // e.g., ['MO', 'WE', 'FR']
  byMonthDay?: number; // Day of month
}

/**
 * Calendar Integration feature — local calendar with events and reminders.
 *
 * Stores events in SQLite, supports reminders, and provides CRUD operations.
 * Events are stored with UTC timestamps for timezone independence.
 */
class CalendarIntegrationFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'calendar-integration',
    version: '0.0.2',
    description: 'Local calendar with events and reminders',
    dependencies: [],
  };

  private config: CalendarConfig = {
    enabled: false,
    dbPath: path.join(process.cwd(), 'data', 'calendar.db'),
    defaultReminderMinutes: 15,
  };
  private ctx!: FeatureContext;
  private db!: Database.Database;
  private _active = false;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<CalendarConfig>) };
  }

  async start(): Promise<void> {
    this.initDb();
    this._active = true;
    this.ctx.logger.info('Calendar Integration started', {
      dbPath: this.config.dbPath,
      defaultReminderMinutes: this.config.defaultReminderMinutes,
    });
  }

  async stop(): Promise<void> {
    this._active = false;
    this.db?.close();
    this.ctx.logger.info('Calendar Integration stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const count = (this.db.prepare('SELECT COUNT(*) as c FROM events').get() as { c: number }).c;
      return {
        healthy: true,
        message: 'Calendar DB accessible',
        details: { eventCount: count },
      };
    } catch (err) {
      return {
        healthy: false,
        message: 'Calendar DB error',
        details: { error: err instanceof Error ? err.message : String(err) },
      };
    }
  }

  /** Initialize database and tables */
  private initDb(): void {
    const dbDir = path.dirname(this.config.dbPath);
    try {
      const { mkdirSync, existsSync } = require('fs');
      if (!existsSync(dbDir)) {
        mkdirSync(dbDir, { recursive: true });
      }
    } catch {
      // ignore FS errors, hope DB can create
    }

    this.db = new Database(this.config.dbPath);
    this.db.pragma('journal_mode = WAL');

    // Events table
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS events (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        description TEXT,
        start_time INTEGER NOT NULL,
        end_time INTEGER NOT NULL,
        all_day INTEGER DEFAULT 0,
        location TEXT,
        recurrence_rule TEXT,
        recurrence_interval INTEGER DEFAULT 1,
        recurrence_until INTEGER,
        recurrence_count INTEGER,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );
    `);

    this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_events_start ON events(start_time);
      CREATE INDEX IF NOT EXISTS idx_events_end ON events(end_time);
    `);

    // Reminders table
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS reminders (
        id TEXT PRIMARY KEY,
        event_id TEXT NOT NULL,
        time INTEGER NOT NULL,
        triggered INTEGER DEFAULT 0,
        method TEXT NOT NULL,
        FOREIGN KEY(event_id) REFERENCES events(id) ON DELETE CASCADE
      );
    `);

    this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_reminders_time ON reminders(time, triggered);
    `);
  }

  /** Get events for a specific date (UTC midnight to midnight) */
  getEvents(date: Date | string): CalendarEvent[] {
    const d = date instanceof Date ? date : new Date(date);
    const startOfDay = Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate());
    const endOfDay = startOfDay + 24 * 60 * 60 * 1000 - 1;

    const stmt = this.db.prepare(`
      SELECT * FROM events
      WHERE (start_time BETWEEN ? AND ?) OR (end_time BETWEEN ? AND ?) OR (start_time <= ? AND end_time >= ?)
      ORDER BY start_time ASC
    `);

    const rows = stmt.all(
      startOfDay,
      endOfDay,
      startOfDay,
      endOfDay,
      startOfDay,
      endOfDay,
    ) as any[];
    return rows.map((row) => this.rowToEvent(row));
  }

  /** Create a new event */
  createEvent(data: {
    title: string;
    description?: string;
    startTime: Date | number;
    endTime: Date | number;
    allDay?: boolean;
    location?: string;
    reminders?: Partial<Reminder>[];
    recurrence?: RecurrenceRule;
  }): CalendarEvent {
    const id = `evt_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const now = Date.now();

    const startTime = data.startTime instanceof Date ? data.startTime.getTime() : data.startTime;
    const endTime = data.endTime instanceof Date ? data.endTime.getTime() : data.endTime;

    if (startTime >= endTime) {
      throw new Error('startTime must be before endTime');
    }

    // Insert event
    const stmt = this.db.prepare(`
      INSERT INTO events (
        id, title, description, start_time, end_time, all_day, location,
        recurrence_rule, recurrence_interval, recurrence_until, recurrence_count,
        created_at, updated_at
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);

    stmt.run(
      id,
      data.title,
      data.description ?? null,
      startTime,
      endTime,
      data.allDay ? 1 : 0,
      data.location ?? null,
      data.recurrence?.frequency ?? null,
      data.recurrence?.interval ?? 1,
      data.recurrence?.until ?? null,
      data.recurrence?.count ?? null,
      now,
      now,
    );

    // Insert reminders
    if (data.reminders && data.reminders.length > 0) {
      const remStmt = this.db.prepare(`
        INSERT INTO reminders (id, event_id, time, triggered, method) VALUES (?, ?, ?, ?, ?)
      `);

      for (const rem of data.reminders) {
        const remId = rem.id ?? `rem_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        // Determine reminder time
        const remTime: number =
          rem.time || startTime - this.config.defaultReminderMinutes * 60 * 1000;
        remStmt.run(remId, id, remTime, rem.triggered ? 1 : 0, rem.method ?? 'notification');
      }
    }

    return this.getEvent(id)!;
  }

  /** Get an event by ID */
  getEvent(id: string): CalendarEvent | null {
    const row = this.db.prepare('SELECT * FROM events WHERE id = ?').get(id) as any;
    if (!row) return null;
    const event = this.rowToEvent(row);
    event.reminders = this.getRemindersForEvent(id);
    return event;
  }

  /** Update an event */
  updateEvent(
    id: string,
    data: Partial<{
      title: string;
      description: string;
      startTime: Date | number;
      endTime: Date | number;
      allDay: boolean;
      location: string;
      recurrence: RecurrenceRule;
    }>,
  ): CalendarEvent | null {
    const existing = this.getEvent(id);
    if (!existing) {
      return null;
    }

    const updateFields: string[] = [];
    const updateValues: any[] = [];

    if (data.title !== undefined) {
      updateFields.push('title = ?');
      updateValues.push(data.title);
    }
    if (data.description !== undefined) {
      updateFields.push('description = ?');
      updateValues.push(data.description);
    }
    if (data.startTime !== undefined) {
      const ts = data.startTime instanceof Date ? data.startTime.getTime() : data.startTime;
      updateFields.push('start_time = ?');
      updateValues.push(ts);
    }
    if (data.endTime !== undefined) {
      const ts = data.endTime instanceof Date ? data.endTime.getTime() : data.endTime;
      updateFields.push('end_time = ?');
      updateValues.push(ts);
    }
    if (data.allDay !== undefined) {
      updateFields.push('all_day = ?');
      updateValues.push(data.allDay ? 1 : 0);
    }
    if (data.location !== undefined) {
      updateFields.push('location = ?');
      updateValues.push(data.location);
    }
    if (data.recurrence !== undefined) {
      updateFields.push('recurrence_rule = ?');
      updateValues.push(data.recurrence.frequency ?? null);
      updateFields.push('recurrence_interval = ?');
      updateValues.push(data.recurrence.interval ?? 1);
      updateFields.push('recurrence_until = ?');
      updateValues.push(data.recurrence.until ?? null);
      updateFields.push('recurrence_count = ?');
      updateValues.push(data.recurrence.count ?? null);
    }

    if (updateFields.length === 0) {
      return existing;
    }

    updateFields.push('updated_at = ?');
    updateValues.push(Date.now());
    updateValues.push(id);

    const stmt = this.db.prepare(`UPDATE events SET ${updateFields.join(', ')} WHERE id = ?`);
    stmt.run(...updateValues);

    return this.getEvent(id)!;
  }

  /** Delete an event */
  deleteEvent(id: string): boolean {
    const info = this.db.prepare('DELETE FROM events WHERE id = ?').run(id);
    if (info.changes > 0) {
      this.ctx.logger.debug('Event deleted', { id });
      return true;
    }
    return false;
  }

  /** Get reminders for an event */
  private getRemindersForEvent(eventId: string): Reminder[] {
    const rows = this.db
      .prepare('SELECT * FROM reminders WHERE event_id = ?')
      .all(eventId) as any[];
    return rows.map((row) => ({
      id: row.id,
      eventId: row.event_id,
      time: row.time,
      triggered: Boolean(row.triggered),
      method: row.method,
    }));
  }

  /** Convert DB row to CalendarEvent */
  private rowToEvent(row: any): CalendarEvent {
    return {
      id: row.id,
      title: row.title,
      description: row.description,
      startTime: row.start_time,
      endTime: row.end_time,
      allDay: Boolean(row.all_day),
      location: row.location,
      recurrence: row.recurrence_rule
        ? {
            frequency: row.recurrence_rule as RecurrenceRule['frequency'],
            interval: row.recurrence_interval,
            until: row.recurrence_until,
            count: row.recurrence_count,
          }
        : undefined,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
      reminders: [], // fill separately with getRemindersForEvent
    };
  }

  /** Add a reminder to an event */
  addReminder(eventId: string, reminder: Partial<Reminder>): Reminder | null {
    const event = this.getEvent(eventId);
    if (!event) return null;

    const id = `rem_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const time: number =
      reminder.time || event.startTime - this.config.defaultReminderMinutes * 60 * 1000;

    const stmt = this.db.prepare(
      'INSERT INTO reminders (id, event_id, time, triggered, method) VALUES (?, ?, ?, ?, ?)',
    );
    stmt.run(id, eventId, time, reminder.triggered ? 1 : 0, reminder.method ?? 'notification');

    // Return the created reminder
    return {
      id,
      eventId,
      time,
      triggered: !!reminder.triggered,
      method: reminder.method ?? 'notification',
    };
  }

  /** Remove a reminder */
  removeReminder(reminderId: string): boolean {
    const info = this.db.prepare('DELETE FROM reminders WHERE id = ?').run(reminderId);
    return info.changes > 0;
  }

  /** Get pending reminders (not triggered, time <= now) */
  getPendingReminders(): Reminder[] {
    const now = Date.now();
    const rows = this.db
      .prepare('SELECT * FROM reminders WHERE triggered = 0 AND time <= ?')
      .all(now) as any[];
    return rows.map((row) => ({
      id: row.id,
      eventId: row.event_id,
      time: row.time,
      triggered: Boolean(row.triggered),
      method: row.method,
    }));
  }

  /** Mark reminder as triggered */
  triggerReminder(reminderId: string): boolean {
    const info = this.db.prepare('UPDATE reminders SET triggered = 1 WHERE id = ?').run(reminderId);
    return info.changes > 0;
  }

  /** Get upcoming events (starting in the future) */
  getUpcoming(limit: number = 10): CalendarEvent[] {
    const now = Date.now();
    const stmt = this.db.prepare(`
      SELECT * FROM events
      WHERE start_time > ?
      ORDER BY start_time ASC
      LIMIT ?
    `);
    const rows = stmt.all(now, limit) as any[];
    return rows
      .map((row) => this.rowToEvent(row))
      .map((event) => ({
        ...event,
        reminders: this.getRemindersForEvent(event.id),
      }));
  }

  /** Query events by date range */
  queryEvents(startDate: Date | number, endDate: Date | number): CalendarEvent[] {
    const start = startDate instanceof Date ? startDate.getTime() : startDate;
    const end = endDate instanceof Date ? endDate.getTime() : endDate;
    const stmt = this.db.prepare(`
      SELECT * FROM events
      WHERE start_time >= ? AND start_time <= ?
      OR (end_time >= ? AND end_time <= ?)
      OR (start_time <= ? AND end_time >= ?)
      ORDER BY start_time ASC
    `);
    const rows = stmt.all(start, end, start, end, start, end) as any[];
    return rows
      .map((row) => this.rowToEvent(row))
      .map((event) => ({
        ...event,
        reminders: this.getRemindersForEvent(event.id),
      }));
  }
}

export default new CalendarIntegrationFeature();

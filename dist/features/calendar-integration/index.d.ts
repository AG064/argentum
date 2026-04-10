/**
 * Calendar Integration Feature
 *
 * Local calendar storage and management with SQLite backend.
 * Supports events, reminders, and recurring events.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
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
    startTime: number;
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
    time: number;
    triggered: boolean;
    method: 'notification' | 'email' | 'sms';
}
/** Recurrence rule (simplified) */
export interface RecurrenceRule {
    frequency: 'daily' | 'weekly' | 'monthly' | 'yearly';
    interval: number;
    until?: number;
    count?: number;
    byDay?: string[];
    byMonthDay?: number;
}
/**
 * Calendar Integration feature — local calendar with events and reminders.
 *
 * Stores events in SQLite, supports reminders, and provides CRUD operations.
 * Events are stored with UTC timestamps for timezone independence.
 */
declare class CalendarIntegrationFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private _active;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Initialize database and tables */
    private initDb;
    /** Get events for a specific date (UTC midnight to midnight) */
    getEvents(date: Date | string): CalendarEvent[];
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
    }): CalendarEvent;
    /** Get an event by ID */
    getEvent(id: string): CalendarEvent | null;
    /** Update an event */
    updateEvent(id: string, data: Partial<{
        title: string;
        description: string;
        startTime: Date | number;
        endTime: Date | number;
        allDay: boolean;
        location: string;
        recurrence: RecurrenceRule;
    }>): CalendarEvent | null;
    /** Delete an event */
    deleteEvent(id: string): boolean;
    /** Get reminders for an event */
    private getRemindersForEvent;
    /** Convert DB row to CalendarEvent */
    private rowToEvent;
    /** Add a reminder to an event */
    addReminder(eventId: string, reminder: Partial<Reminder>): Reminder | null;
    /** Remove a reminder */
    removeReminder(reminderId: string): boolean;
    /** Get pending reminders (not triggered, time <= now) */
    getPendingReminders(): Reminder[];
    /** Mark reminder as triggered */
    triggerReminder(reminderId: string): boolean;
    /** Get upcoming events (starting in the future) */
    getUpcoming(limit?: number): CalendarEvent[];
    /** Query events by date range */
    queryEvents(startDate: Date | number, endDate: Date | number): CalendarEvent[];
}
declare const _default: CalendarIntegrationFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map
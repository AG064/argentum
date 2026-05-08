/**
 * Morning Briefing Feature
 *
 * Generates a daily briefing with calendar events, weather,
 * news, tasks, and personalized insights every morning.
 */

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

/** Morning briefing configuration */
export interface MorningBriefingConfig {
  enabled: boolean;
  deliveryTime: string; // HH:MM format
  timezone: string;
  includeWeather: boolean;
  includeCalendar: boolean;
  includeNews: boolean;
  includeTasks: boolean;
  includeInsights: boolean;
  weatherLocation: string;
  newsTopics: string[];
  maxNewsItems: number;
}

/** Briefing section */
export interface BriefingSection {
  type: 'weather' | 'calendar' | 'news' | 'tasks' | 'insights' | 'custom';
  title: string;
  items: BriefingItem[];
  priority: number;
}

/** Briefing item */
export interface BriefingItem {
  title: string;
  detail?: string;
  time?: string;
  icon?: string;
  link?: string;
  metadata?: Record<string, unknown>;
}

/** Generated briefing */
export interface MorningBriefing {
  id: string;
  date: string;
  generatedAt: number;
  sections: BriefingSection[];
  summary: string;
  delivered: boolean;
}

/** Briefing handler — called when briefing is generated */
export type BriefingHandler = (briefing: MorningBriefing) => Promise<void>;

/**
 * Morning Briefing feature — daily personalized morning summary.
 *
 * Aggregates calendar, weather, news, and tasks into a concise
 * morning briefing delivered at a configured time.
 */
class MorningBriefingFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'morning-briefing',
    version: '0.0.5',
    description: 'Daily personalized morning briefing generator',
    dependencies: [],
  };

  private config: MorningBriefingConfig = {
    enabled: false,
    deliveryTime: '08:00',
    timezone: 'UTC',
    includeWeather: true,
    includeCalendar: true,
    includeNews: true,
    includeTasks: true,
    includeInsights: true,
    weatherLocation: 'New York, US',
    newsTopics: ['technology', 'science'],
    maxNewsItems: 5,
  };
  private ctx!: FeatureContext;
  private handlers: BriefingHandler[] = [];
  private briefingHistory: MorningBriefing[] = [];
  private timer: ReturnType<typeof setInterval> | null = null;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<MorningBriefingConfig>) };
  }

  async start(): Promise<void> {
    // Schedule daily briefing
    this.scheduleNextBriefing();
    this.ctx.logger.info('Morning Briefing active', {
      deliveryTime: this.config.deliveryTime,
      timezone: this.config.timezone,
    });
  }

  async stop(): Promise<void> {
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    const lastBriefing = this.briefingHistory[this.briefingHistory.length - 1];
    return {
      healthy: true,
      details: {
        briefingsGenerated: this.briefingHistory.length,
        lastBriefing: lastBriefing?.date ?? 'never',
        scheduledTime: this.config.deliveryTime,
      },
    };
  }

  /** Register a handler for briefing delivery */
  onBriefing(handler: BriefingHandler): void {
    this.handlers.push(handler);
  }

  /** Generate a morning briefing */
  async generateBriefing(): Promise<MorningBriefing> {
    const today = new Date().toISOString().split('T')[0] ?? new Date().toDateString();
    const sections: BriefingSection[] = [];

    if (this.config.includeWeather) {
      sections.push(await this.getWeatherSection());
    }
    if (this.config.includeCalendar) {
      sections.push(await this.getCalendarSection());
    }
    if (this.config.includeNews) {
      sections.push(await this.getNewsSection());
    }
    if (this.config.includeTasks) {
      sections.push(await this.getTasksSection());
    }
    if (this.config.includeInsights) {
      sections.push(await this.getInsightsSection());
    }

    sections.sort((a, b) => a.priority - b.priority);

    const briefing: MorningBriefing = {
      id: `briefing_${Date.now()}`,
      date: today,
      generatedAt: Date.now(),
      sections,
      summary: this.generateSummary(sections),
      delivered: false,
    };

    this.briefingHistory.push(briefing);

    // Deliver to handlers
    for (const handler of this.handlers) {
      try {
        await handler(briefing);
      } catch (err) {
        this.ctx.logger.error('Briefing handler error', {
          error: err instanceof Error ? err.message : String(err),
        });
      }
    }

    briefing.delivered = true;
    return briefing;
  }

  /** Get briefing history */
  getHistory(limit = 7): MorningBriefing[] {
    return this.briefingHistory.slice(-limit);
  }

  /** Schedule next briefing */
  private scheduleNextBriefing(): void {
    const now = new Date();
    const [hours, minutes] = this.config.deliveryTime.split(':').map(Number);
    const target = new Date(now);
    target.setHours(hours!, minutes, 0, 0);

    if (target <= now) {
      target.setDate(target.getDate() + 1);
    }

    const delay = target.getTime() - now.getTime();
    this.timer = setTimeout(() => {
      void this.generateBriefing()
        .then(() => {
          this.scheduleNextBriefing();
        })
        .catch((error: unknown) => {
          this.ctx.logger.error('Morning briefing failed', {
            error: error instanceof Error ? error.message : String(error),
          });
          this.scheduleNextBriefing();
        });
    }, delay);
  }

  private async getWeatherSection(): Promise<BriefingSection> {
    // Would fetch real weather data
    return {
      type: 'weather',
      title: 'Weather',
      priority: 1,
      items: [{ title: this.config.weatherLocation, detail: 'Weather data pending integration' }],
    };
  }

  private async getCalendarSection(): Promise<BriefingSection> {
    return {
      type: 'calendar',
      title: "Today's Schedule",
      priority: 2,
      items: [],
    };
  }

  private async getNewsSection(): Promise<BriefingSection> {
    return {
      type: 'news',
      title: 'News',
      priority: 3,
      items: [],
    };
  }

  private async getTasksSection(): Promise<BriefingSection> {
    return {
      type: 'tasks',
      title: 'Tasks',
      priority: 4,
      items: [],
    };
  }

  private async getInsightsSection(): Promise<BriefingSection> {
    return {
      type: 'insights',
      title: 'Insights',
      priority: 5,
      items: [],
    };
  }

  private generateSummary(sections: BriefingSection[]): string {
    const totalItems = sections.reduce((sum, s) => sum + s.items.length, 0);
    return `Good morning! Your briefing includes ${sections.length} sections with ${totalItems} items.`;
  }
}

export default new MorningBriefingFeature();

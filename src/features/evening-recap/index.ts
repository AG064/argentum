/**
 * Evening Recap Feature
 *
 * End-of-day summary with accomplishments, pending tasks,
 * tomorrow's preview, and daily metrics.
 */

import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';

/** Evening recap configuration */
export interface EveningRecapConfig {
  enabled: boolean;
  deliveryTime: string;
  timezone: string;
  includeAccomplishments: boolean;
  includePendingTasks: boolean;
  includeTomorrowPreview: boolean;
  includeMetrics: boolean;
  includeReflection: boolean;
}

/** Daily metrics */
export interface DailyMetrics {
  tasksCompleted: number;
  tasksPending: number;
  messagesExchanged: number;
  featuresUsed: string[];
  activeTimeMinutes: number;
  focusScore: number; // 0-100
}

/** Evening recap */
export interface EveningRecap {
  id: string;
  date: string;
  generatedAt: number;
  accomplishments: string[];
  pendingTasks: Array<{ title: string; priority: string; dueDate?: string }>;
  tomorrowPreview: Array<{ title: string; time?: string }>;
  metrics: DailyMetrics;
  reflection: string;
  delivered: boolean;
}

/** Recap handler */
export type RecapHandler = (recap: EveningRecap) => Promise<void>;

/**
 * Evening Recap feature — end-of-day summary and reflection.
 *
 * Generates a nightly recap with accomplishments, pending work,
 * tomorrow's preview, and productivity metrics.
 */
class EveningRecapFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'evening-recap',
    version: '0.1.0',
    description: 'End-of-day summary with accomplishments and metrics',
    dependencies: [],
  };

  private config: EveningRecapConfig = {
    enabled: false,
    deliveryTime: '21:00',
    timezone: 'UTC',
    includeAccomplishments: true,
    includePendingTasks: true,
    includeTomorrowPreview: true,
    includeMetrics: true,
    includeReflection: true,
  };
  private ctx!: FeatureContext;
  private handlers: RecapHandler[] = [];
  private recapHistory: EveningRecap[] = [];
  private timer: ReturnType<typeof setInterval> | null = null;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<EveningRecapConfig>) };
  }

  async start(): Promise<void> {
    this.scheduleNextRecap();
    this.ctx.logger.info('Evening Recap active', {
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
    return {
      healthy: true,
      details: {
        recapsGenerated: this.recapHistory.length,
        scheduledTime: this.config.deliveryTime,
      },
    };
  }

  /** Register a recap delivery handler */
  onRecap(handler: RecapHandler): void {
    this.handlers.push(handler);
  }

  /** Generate evening recap */
  async generateRecap(): Promise<EveningRecap> {
    const today = new Date().toISOString().split('T')[0];

    const recap: EveningRecap = {
      id: `recap_${Date.now()}`,
      date: today!,
      generatedAt: Date.now(),
      accomplishments: await this.getAccomplishments(),
      pendingTasks: await this.getPendingTasks(),
      tomorrowPreview: await this.getTomorrowPreview(),
      metrics: await this.getDailyMetrics(),
      reflection: await this.generateReflection(),
      delivered: false,
    };

    this.recapHistory.push(recap);

    for (const handler of this.handlers) {
      try {
        await handler(recap);
      } catch (err) {
        this.ctx.logger.error('Recap handler error', {
          error: err instanceof Error ? err.message : String(err),
        });
      }
    }

    recap.delivered = true;
    return recap;
  }

  /** Get recap history */
  getHistory(limit = 7): EveningRecap[] {
    return this.recapHistory.slice(-limit);
  }

  /** Record an accomplishment for today */
  recordAccomplishment(description: string): void {
    const today = new Date().toISOString().split('T')[0];
    const todayRecap = this.recapHistory.find(r => r.date === today);
    if (todayRecap) {
      todayRecap.accomplishments.push(description);
    }
  }

  /** Schedule next recap */
  private scheduleNextRecap(): void {
    const now = new Date();
    const [hours, minutes] = this.config.deliveryTime.split(':').map(Number);
    const target = new Date(now);
    target.setHours(hours!, minutes, 0, 0);

    if (target <= now) {
      target.setDate(target.getDate() + 1);
    }

    const delay = target.getTime() - now.getTime();
    this.timer = setTimeout(async () => {
      await this.generateRecap();
      this.scheduleNextRecap();
    }, delay);
  }

  private async getAccomplishments(): Promise<string[]> {
    // Would pull from activity logs
    return [];
  }

  private async getPendingTasks(): Promise<EveningRecap['pendingTasks']> {
    return [];
  }

  private async getTomorrowPreview(): Promise<EveningRecap['tomorrowPreview']> {
    return [];
  }

  private async getDailyMetrics(): Promise<DailyMetrics> {
    return {
      tasksCompleted: 0,
      tasksPending: 0,
      messagesExchanged: 0,
      featuresUsed: [],
      activeTimeMinutes: 0,
      focusScore: 0,
    };
  }

  private async generateReflection(): Promise<string> {
    return 'Take a moment to reflect on your day. What went well? What could be improved?';
  }
}

export default new EveningRecapFeature();

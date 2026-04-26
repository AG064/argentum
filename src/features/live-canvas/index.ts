/**
 * Live Canvas Feature
 *
 * Real-time collaborative drawing and visualization canvas.
 * Supports shapes, freehand drawing, text, and image overlays.
 */

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

/** Live Canvas configuration */
export interface LiveCanvasConfig {
  enabled: boolean;
  maxWidth: number;
  maxHeight: number;
  maxLayers: number;
  defaultBackground: string;
}

/** Canvas element types */
export type CanvasElementType = 'rectangle' | 'circle' | 'line' | 'path' | 'text' | 'image';

/** Canvas element */
export interface CanvasElement {
  id: string;
  type: CanvasElementType;
  x: number;
  y: number;
  width?: number;
  height?: number;
  points?: Array<{ x: number; y: number }>;
  text?: string;
  src?: string;
  style: {
    fill?: string;
    stroke?: string;
    strokeWidth?: number;
    fontSize?: number;
    fontFamily?: string;
    opacity?: number;
  };
  layer: number;
  locked: boolean;
  createdAt: number;
}

/** Canvas state */
export interface Canvas {
  id: string;
  name: string;
  width: number;
  height: number;
  background: string;
  elements: Map<string, CanvasElement>;
  layers: string[];
  collaborators: Set<string>;
  createdAt: number;
  updatedAt: number;
}

/** Canvas operation */
export interface CanvasOperation {
  type: 'add' | 'update' | 'delete' | 'move' | 'resize';
  elementId: string;
  data?: Partial<CanvasElement>;
  userId: string;
  timestamp: number;
}

/**
 * Live Canvas feature — real-time collaborative drawing canvas.
 *
 * Provides a shared canvas for drawing, annotating, and visualizing
 * data with multi-user collaboration support.
 */
class LiveCanvasFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'live-canvas',
    version: '0.0.2',
    description: 'Real-time collaborative drawing and visualization canvas',
    dependencies: [],
  };

  private config: LiveCanvasConfig = {
    enabled: false,
    maxWidth: 1920,
    maxHeight: 1080,
    maxLayers: 10,
    defaultBackground: '#ffffff',
  };
  private ctx!: FeatureContext;
  private canvases: Map<string, Canvas> = new Map();
  private operationListeners: Map<string, Set<(op: CanvasOperation) => void>> = new Map();

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<LiveCanvasConfig>) };
  }

  async start(): Promise<void> {
    this.ctx.logger.info('Live Canvas active');
  }

  async stop(): Promise<void> {
    this.canvases.clear();
    this.operationListeners.clear();
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: true,
      details: { canvases: this.canvases.size },
    };
  }

  /** Create a new canvas */
  createCanvas(name: string, width?: number, height?: number): Canvas {
    const id = `canvas_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const canvas: Canvas = {
      id,
      name,
      width: width ?? this.config.maxWidth,
      height: height ?? this.config.maxHeight,
      background: this.config.defaultBackground,
      elements: new Map(),
      layers: ['default'],
      collaborators: new Set(),
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };
    this.canvases.set(id, canvas);
    return canvas;
  }

  /** Get a canvas by ID */
  getCanvas(id: string): Canvas | undefined {
    return this.canvases.get(id);
  }

  /** Add an element to canvas */
  addElement(
    canvasId: string,
    element: Omit<CanvasElement, 'id' | 'createdAt'>,
  ): CanvasElement | null {
    const canvas = this.canvases.get(canvasId);
    if (!canvas) return null;

    const fullElement: CanvasElement = {
      ...element,
      id: `el_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
      createdAt: Date.now(),
    };
    canvas.elements.set(fullElement.id, fullElement);
    canvas.updatedAt = Date.now();

    this.emitOperation(canvasId, {
      type: 'add',
      elementId: fullElement.id,
      data: fullElement,
      userId: 'system',
      timestamp: Date.now(),
    });

    return fullElement;
  }

  /** Update an element */
  updateElement(canvasId: string, elementId: string, updates: Partial<CanvasElement>): boolean {
    const canvas = this.canvases.get(canvasId);
    if (!canvas) return false;

    const element = canvas.elements.get(elementId);
    if (!element) return false;

    Object.assign(element, updates);
    canvas.updatedAt = Date.now();

    this.emitOperation(canvasId, {
      type: 'update',
      elementId,
      data: updates,
      userId: 'system',
      timestamp: Date.now(),
    });

    return true;
  }

  /** Delete an element */
  deleteElement(canvasId: string, elementId: string): boolean {
    const canvas = this.canvases.get(canvasId);
    if (!canvas) return false;

    const deleted = canvas.elements.delete(elementId);
    if (deleted) {
      canvas.updatedAt = Date.now();
      this.emitOperation(canvasId, {
        type: 'delete',
        elementId,
        userId: 'system',
        timestamp: Date.now(),
      });
    }
    return deleted;
  }

  /** Subscribe to canvas operations */
  onOperation(canvasId: string, handler: (op: CanvasOperation) => void): () => void {
    const listeners = this.operationListeners.get(canvasId) ?? new Set();
    listeners.add(handler);
    this.operationListeners.set(canvasId, listeners);
    return () => listeners.delete(handler);
  }

  /** Emit operation to listeners */
  private emitOperation(canvasId: string, op: CanvasOperation): void {
    const listeners = this.operationListeners.get(canvasId);
    if (!listeners) return;
    for (const handler of listeners) {
      try {
        handler(op);
      } catch {
        // Swallow listener errors
      }
    }
  }

  /** Export canvas as JSON */
  exportCanvas(canvasId: string): string | null {
    const canvas = this.canvases.get(canvasId);
    if (!canvas) return null;
    return JSON.stringify({
      ...canvas,
      elements: Array.from(canvas.elements.entries()),
      collaborators: Array.from(canvas.collaborators),
    });
  }
}

export default new LiveCanvasFeature();
